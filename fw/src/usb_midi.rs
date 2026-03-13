use defmt::{info, warn};
use embassy_futures::select::{Either, select};
use embassy_stm32::usb::Driver;
use embassy_stm32::{Peri, peripherals};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_usb::Builder;
use embassy_usb::UsbDevice;
use embassy_usb::class::midi::MidiClass;
use expresso::midi::{
    DecodeResult, MidiDecoder, MidiEncoder, MidiEndpoint, UsbMidiDecoder, UsbMidiEncoder,
};
use expresso::sysex::{
    SYSEX_CMD_SETTINGS_PATCH, SYSEX_CMD_SETTINGS_SET, SysexDispatcher, SysexResponse,
};
use static_cell::StaticCell;

use crate::collector::Collector;
use crate::config::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};
use crate::types::{InMsgSender, MsgReceiver, SettingsMutex, StatusEvent, StatusSender};

pub type StaticDriver = Driver<'static, peripherals::USB>;
pub type StaticDevice = UsbDevice<'static, StaticDriver>;
pub type UsbMidi = MidiClass<'static, StaticDriver>;

type PacketBuffer<const N: usize> = Collector<N, [u8; 4]>;
type SysexChannel = Channel<CriticalSectionRawMutex, SysexResponse, 4>;
type SysexSender = Sender<'static, CriticalSectionRawMutex, SysexResponse, 4>;

pub fn build(
    usb: Peri<'static, peripherals::USB>,
    dp: Peri<'static, peripherals::PA12>,
    dm: Peri<'static, peripherals::PA11>,
) -> (StaticDevice, UsbMidi) {
    let driver = Driver::new(usb, crate::Irqs, dp, dm);

    let usb_config = {
        let mut c = embassy_usb::Config::new(0x1209, 0x2156);
        c.manufacturer = Some("Amon Benson");
        c.product = Some("Expresso");
        c.serial_number = Some("62638335");
        c.max_power = 100;
        c.max_packet_size_0 = 64;
        c
    };

    let mut builder = {
        static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESC: StaticCell<[u8; 32]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        Builder::new(
            driver,
            usb_config,
            CONFIG_DESC.init([0; 256]),
            BOS_DESC.init([0; 32]),
            &mut [],
            CONTROL_BUF.init([0; 64]),
        )
    };

    let midi = MidiClass::new(&mut builder, 1, 1, 64);
    let device = builder.build();

    (device, midi)
}

#[embassy_executor::task]
pub async fn device_task(mut usb: StaticDevice) -> ! {
    usb.run().await
}

#[embassy_executor::task]
pub async fn io_task(
    midi: UsbMidi,
    from_router: MsgReceiver,
    to_router: InMsgSender,
    settings: &'static SettingsMutex,
    status: StatusSender,
) {
    info!("USB MIDI IO task started");

    static SYSEX_CH: StaticCell<SysexChannel> = StaticCell::new();
    let sysex_ch = SYSEX_CH.init(Channel::new());
    let sysex_tx: SysexSender = sysex_ch.sender();
    let sysex_rx = sysex_ch.receiver();

    let mut sysex = SysexDispatcher::new(FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH);
    let mut decoder = UsbMidiDecoder::<320>::new();

    let (mut tx, mut rx) = midi.split();

    loop {
        tx.wait_connection().await;
        let _ = status.try_send(StatusEvent::UsbConnected(true));
        info!("USB MIDI host connected");

        let tx_fut = async {
            let mut encoder = UsbMidiEncoder;
            loop {
                match select(from_router.receive(), sysex_rx.receive()).await {
                    Either::First(message) => {
                        // Regular MIDI out — signal activity
                        let _ = status.try_send(StatusEvent::MidiUsbOut);
                        let mut buffer = PacketBuffer::<4>::new();
                        let _ = encoder.emit(&message, &mut buffer);
                        for i in 0..buffer.len() {
                            if tx.write_packet(buffer.get(i)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Either::Second(response) => {
                        // SysEx response — stream packets directly
                        let payload = &response.data[..response.len];
                        let mut i = 0;
                        while i < payload.len() {
                            let remaining = payload.len() - i;
                            let packet = match remaining {
                                1 => [0x05, payload[i], 0x00, 0x00],
                                2 => [0x06, payload[i], payload[i + 1], 0x00],
                                r if r >= 3 && i + 3 >= payload.len() => {
                                    [0x07, payload[i], payload[i + 1], payload[i + 2]]
                                }
                                _ => [0x04, payload[i], payload[i + 1], payload[i + 2]],
                            };
                            if tx.write_packet(&packet).await.is_err() {
                                return;
                            }
                            i += 3;
                        }
                    }
                }
            }
        };

        let rx_fut = async {
            let mut buf = [0u8; 64];
            loop {
                let n = match rx.read_packet(&mut buf).await {
                    Ok(n) => n,
                    Err(_) => return,
                };
                for chunk in buf[..n].chunks_exact(4) {
                    let packet = [chunk[0], chunk[1], chunk[2], chunk[3]];
                    match decoder.feed(packet) {
                        Some(DecodeResult::Message(msg)) => {
                            let _ = status.try_send(StatusEvent::MidiUsbIn);
                            if to_router.try_send((msg, MidiEndpoint::Usb)).is_err() {
                                warn!("USB MIDI RX: channel full, message dropped");
                            }
                        }
                        Some(DecodeResult::Sysex(payload)) => {
                            // Detect settings-write commands before consuming the payload
                            let cmd = payload.get(6).copied().unwrap_or(0);
                            let response =
                                settings.lock(|s| sysex.handle(payload, &mut s.borrow_mut()));
                            if let Some(response) = response {
                                if cmd == SYSEX_CMD_SETTINGS_SET || cmd == SYSEX_CMD_SETTINGS_PATCH
                                {
                                    let _ = status.try_send(StatusEvent::SettingsUpdate);
                                }
                                if sysex_tx.try_send(response).is_err() {
                                    warn!("SysEx: response channel full");
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
        };

        match select(tx_fut, rx_fut).await {
            Either::First(_) => warn!("USB MIDI TX loop exited"),
            Either::Second(_) => warn!("USB MIDI RX loop exited"),
        }

        let _ = status.try_send(StatusEvent::UsbConnected(false));
        decoder.reset();
        info!("USB MIDI host disconnected, waiting for reconnect");
    }
}
