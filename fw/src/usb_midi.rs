use defmt::{info, warn};
use embassy_futures::select::{Either, select};
use embassy_stm32::usb::Driver;
use embassy_stm32::{Peri, peripherals};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_usb::Builder;
use embassy_usb::UsbDevice;
use embassy_usb::class::midi::MidiClass;
use expresso::midi::types::MidiEndpoint;
use expresso::midi::{MidiDecoder, MidiEncoder, MidiMessage, UsbMidiDecoder, UsbMidiEncoder};
use static_cell::StaticCell;

use crate::collector::Collector;
use crate::{InMsgSender, MsgReceiver};

pub type StaticDriver = Driver<'static, peripherals::USB>;
pub type StaticDevice = UsbDevice<'static, StaticDriver>;
pub type UsbMidi = MidiClass<'static, StaticDriver>;

type PacketBuffer<const N: usize> = Collector<N, [u8; 4]>;

// SysEx: single-byte non-commercial manufacturer ID 0x7D
const SYSEX_MFID: u8 = 0x7D;
const SYSEX_CMD_VERSION_REQUEST: u8 = 0x01;
const SYSEX_CMD_VERSION_REPLY: u8 = 0x02;
const FW_VERSION_MAJOR: u8 = 0;
const FW_VERSION_MINOR: u8 = 1;
const FW_VERSION_PATCH: u8 = 0;

const SYSEX_BUF_SIZE: usize = 16;

struct SysexBuf {
    data: [u8; SYSEX_BUF_SIZE],
    len: usize,
}

type SysexChannel = Channel<CriticalSectionRawMutex, SysexBuf, 4>;
type SysexSender = Sender<'static, CriticalSectionRawMutex, SysexBuf, 4>;

// Checks payload for the known identifier prefix and dispatches commands.
// Payload includes the leading 0xF0 and trailing 0xF7.
fn handle_sysex(payload: &[u8], sysex_tx: SysexSender) {
    // Minimum: [0xF0, MFID, cmd, 0xF7]
    if payload.len() < 4 || payload[0] != 0xF0 || payload[1] != SYSEX_MFID {
        return;
    }
    match payload[2] {
        SYSEX_CMD_VERSION_REQUEST => {
            let mut buf = SysexBuf { data: [0; SYSEX_BUF_SIZE], len: 0 };
            buf.data[0] = 0xF0;
            buf.data[1] = SYSEX_MFID;
            buf.data[2] = SYSEX_CMD_VERSION_REPLY;
            buf.data[3] = FW_VERSION_MAJOR;
            buf.data[4] = FW_VERSION_MINOR;
            buf.data[5] = FW_VERSION_PATCH;
            buf.data[6] = 0xF7;
            buf.len = 7;
            if sysex_tx.try_send(buf).is_err() {
                warn!("SysEx: response channel full");
            }
        }
        _ => {}
    }
}

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
pub async fn io_task(midi: UsbMidi, from_router: MsgReceiver, to_router: InMsgSender) {
    info!("USB MIDI IO task started");

    static SYSEX_CH: StaticCell<SysexChannel> = StaticCell::new();
    let sysex_ch = SYSEX_CH.init(Channel::new());
    let sysex_tx = sysex_ch.sender();
    let sysex_rx = sysex_ch.receiver();

    let (mut tx, mut rx) = midi.split();
    let mut decoder = UsbMidiDecoder::<64>::new();

    loop {
        tx.wait_connection().await;
        info!("USB MIDI host connected");

        let tx_fut = async {
            let mut encoder = UsbMidiEncoder;
            loop {
                let mut buffer = PacketBuffer::<16>::new();
                match select(from_router.receive(), sysex_rx.receive()).await {
                    Either::First(message) => {
                        let _ = encoder.emit(&message, &mut buffer);
                    }
                    Either::Second(sysex_buf) => {
                        let _ = encoder.emit(
                            &MidiMessage::Sysex(&sysex_buf.data[..sysex_buf.len]),
                            &mut buffer,
                        );
                    }
                }
                for i in 0..buffer.len() {
                    if tx.write_packet(buffer.get(i)).await.is_err() {
                        return;
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
                    if let Some(message) = decoder.feed(packet) {
                        match message {
                            MidiMessage::Sysex(payload) => handle_sysex(payload, sysex_tx),
                            other => {
                                if let Some(msg) = other.to_static() {
                                    if to_router
                                        .try_send((msg, MidiEndpoint::Usb))
                                        .is_err()
                                    {
                                        warn!("USB MIDI RX: channel full, message dropped");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        match select(tx_fut, rx_fut).await {
            Either::First(_) => warn!("USB MIDI TX loop exited"),
            Either::Second(_) => warn!("USB MIDI RX loop exited"),
        }

        decoder.reset();
        info!("USB MIDI host disconnected, waiting for reconnect");
    }
}
