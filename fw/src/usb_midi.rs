use defmt::{info, warn};
use embassy_futures::select::{Either, select};
use embassy_stm32::usb::Driver;
use embassy_stm32::{Peri, peripherals};
use embassy_usb::Builder;
use embassy_usb::UsbDevice;
use embassy_usb::class::midi::MidiClass;
use expresso::midi::{MidiDecoder, MidiEncoder, PacketSink, UsbMidiDecoder, UsbMidiEncoder};
use static_cell::StaticCell;

use crate::{MsgReceiver, MsgSender};

pub type StaticDriver = Driver<'static, peripherals::USB>;
pub type StaticDevice = UsbDevice<'static, StaticDriver>;
pub type UsbMidi = MidiClass<'static, StaticDriver>;

// Local sink that buffers encoded USB packets ([u8; 4]) for async write.
struct PacketBuf<const N: usize> {
    buf: [[u8; 4]; N],
    len: usize,
}

impl<const N: usize> PacketBuf<N> {
    fn new() -> Self {
        Self { buf: [[0; 4]; N], len: 0 }
    }
}

impl<const N: usize> PacketSink for PacketBuf<N> {
    type Packet = [u8; 4];
    type Error = core::convert::Infallible;

    fn emit(&mut self, packet: [u8; 4]) -> Result<(), Self::Error> {
        if self.len < N {
            self.buf[self.len] = packet;
            self.len += 1;
        }
        Ok(())
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
        c.serial_number = Some("12345678");
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
pub async fn io_task(midi: UsbMidi, from_router: MsgReceiver, to_router: MsgSender) {
    info!("USB MIDI IO task started");

    let (mut tx, mut rx) = midi.split();
    let mut decoder = UsbMidiDecoder::<64>::new();

    loop {
        tx.wait_connection().await;
        info!("USB MIDI host connected");

        let tx_fut = async {
            let mut encoder = UsbMidiEncoder;
            loop {
                let message = from_router.receive().await;
                let mut buf = PacketBuf::<8>::new();
                let _ = encoder.emit(&message, &mut buf);
                for i in 0..buf.len {
                    if tx.write_packet(&buf.buf[i]).await.is_err() {
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
                    if let Some(msg) = decoder.feed(packet) {
                        if let Some(static_msg) = crate::to_static(msg) {
                            if to_router.try_send(static_msg).is_err() {
                                warn!("USB MIDI RX: channel full, message dropped");
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
