use defmt::{info, warn};
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_stm32::usb::Driver;
use embassy_stm32::{peripherals, Peri};
use embassy_usb::class::midi::{MidiClass, Receiver as MidiReceiver_, Sender as MidiSender_};
use embassy_usb::Builder;
use static_cell::StaticCell;

use crate::midi::{MidiBridge, MidiEvent, MidiMessage, MidiPeripheral, MidiReceiver, MidiSender};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Construction parameters for [`UsbMidi`].
///
/// Pins (from expresso.ioc): PA11 = USB_DM, PA12 = USB_DP
pub struct UsbMidiConfig {
    pub usb: Peri<'static, peripherals::USB>,
    pub dp:  Peri<'static, peripherals::PA12>,
    pub dm:  Peri<'static, peripherals::PA11>,
}

// ---------------------------------------------------------------------------
// Driver struct
// ---------------------------------------------------------------------------

/// USB MIDI 1.0 peripheral driver.
///
/// Presents a USB-MIDI 1.0 device to the host.
///
/// - **Inbound** (USB → bus): bytes received from the host are parsed into
///   [`MidiEvent`]s and placed on the shared event bus via `to_bus`.
/// - **Outbound** (bus → USB): [`MidiEvent`]s delivered by the router via
///   `from_router` are serialised and sent to the host.
pub struct UsbMidi {
    usb: Peri<'static, peripherals::USB>,
    dp:  Peri<'static, peripherals::PA12>,
    dm:  Peri<'static, peripherals::PA11>,
}

impl UsbMidi {
    pub fn new(config: UsbMidiConfig) -> Self {
        Self { usb: config.usb, dp: config.dp, dm: config.dm }
    }
}

// ---------------------------------------------------------------------------
// MidiBridge impl
// ---------------------------------------------------------------------------

impl MidiBridge for UsbMidi {
    async fn run(self, from_router: MidiReceiver<'static>, to_bus: MidiSender<'static>) {
        info!("USB MIDI task started");

        // Static descriptor buffers. These must outlive the USB device, and
        // since we hold `Peri<'static, USB>` the driver is `'static` too.
        static CONFIG_DESC:  StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESC:     StaticCell<[u8; 32]>  = StaticCell::new();
        static MSOS_DESC:    StaticCell<[u8; 1]>   = StaticCell::new();
        static CONTROL_BUF:  StaticCell<[u8; 64]>  = StaticCell::new();

        let config_desc  = CONFIG_DESC.init([0u8; 256]);
        let bos_desc     = BOS_DESC.init([0u8; 32]);
        let msos_desc    = MSOS_DESC.init([0u8; 1]);
        let control_buf  = CONTROL_BUF.init([0u8; 64]);

        let driver = Driver::new(self.usb, crate::Irqs, self.dp, self.dm);

        let mut usb_config = embassy_usb::Config::new(0x1209, 0x2156);
        usb_config.manufacturer = Some("Expresso");
        usb_config.product      = Some("Expresso MIDI");
        usb_config.serial_number = Some("00000001");
        usb_config.max_power    = 100;

        let mut builder = Builder::new(
            driver,
            usb_config,
            config_desc,
            bos_desc,
            msos_desc,
            control_buf,
        );

        let midi = MidiClass::new(&mut builder, 1, 1, 64);
        let mut usb_dev = builder.build();

        join(usb_dev.run(), io_loop(midi, from_router, to_bus)).await;
    }
}

// ---------------------------------------------------------------------------
// IO loop — runs concurrently with usb_dev.run()
// ---------------------------------------------------------------------------

async fn io_loop(
    midi: MidiClass<'static, Driver<'static, peripherals::USB>>,
    from_router: MidiReceiver<'static>,
    to_bus: MidiSender<'static>,
) {
    let (mut tx, mut rx) = midi.split();

    loop {
        // Wait for the host to connect before doing any I/O.
        tx.wait_connection().await;
        info!("USB MIDI host connected");

        // Run TX and RX concurrently; exit to reconnect loop on any error.
        let result = select(
            tx_loop(&mut tx, &from_router),
            rx_loop(&mut rx, &to_bus),
        )
        .await;

        match result {
            Either::First(_)  => warn!("USB MIDI TX loop exited"),
            Either::Second(_) => warn!("USB MIDI RX loop exited"),
        }

        info!("USB MIDI host disconnected, waiting for reconnect");
    }
}

// ---------------------------------------------------------------------------
// TX — bus → USB host
// ---------------------------------------------------------------------------

async fn tx_loop(
    tx: &mut MidiSender_<'static, Driver<'static, peripherals::USB>>,
    from_router: &MidiReceiver<'static>,
) {
    loop {
        let event = from_router.receive().await;
        if let Some(packet) = event_to_usb_packet(&event) {
            if tx.write_packet(&packet).await.is_err() {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RX — USB host → bus
// ---------------------------------------------------------------------------

async fn rx_loop(
    rx: &mut MidiReceiver_<'static, Driver<'static, peripherals::USB>>,
    to_bus: &MidiSender<'static>,
) {
    let mut buf = [0u8; 64];
    loop {
        let n = match rx.read_packet(&mut buf).await {
            Ok(n)  => n,
            Err(_) => return,
        };

        // USB MIDI packets are 4 bytes each.
        for chunk in buf[..n].chunks_exact(4) {
            let packet = [chunk[0], chunk[1], chunk[2], chunk[3]];
            if let Some(message) = usb_packet_to_message(packet) {
                let event = MidiEvent::new(MidiPeripheral::Usb, message);
                if to_bus.try_send(event).is_err() {
                    warn!("USB MIDI RX: bus full, event dropped");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// USB MIDI 1.0 serialisation helpers
// ---------------------------------------------------------------------------

/// Encode a [`MidiEvent`] as a 4-byte USB MIDI 1.0 packet.
///
/// Returns `None` for messages that have no USB MIDI representation.
fn event_to_usb_packet(event: &MidiEvent) -> Option<[u8; 4]> {
    // Cable number 0 is used throughout; CIN occupies the low nibble of byte 0.
    match event.message {
        MidiMessage::NoteOn { channel, note, velocity } => {
            let status = 0x90 | (channel & 0x0F);
            Some([0x09, status, note, velocity])
        }
        MidiMessage::NoteOff { channel, note, velocity } => {
            let status = 0x80 | (channel & 0x0F);
            Some([0x08, status, note, velocity])
        }
        MidiMessage::ControlChange { channel, control, value } => {
            let status = 0xB0 | (channel & 0x0F);
            Some([0x0B, status, control, value])
        }
        MidiMessage::ProgramChange { channel, program } => {
            let status = 0xC0 | (channel & 0x0F);
            Some([0x0C, status, program, 0x00])
        }
        MidiMessage::PitchBend { channel, value } => {
            // Pitch bend: signed –8192…+8191 → unsigned 0…16383 (LSB first)
            let unsigned = (value + 8192) as u16;
            let lsb = (unsigned & 0x7F) as u8;
            let msb = ((unsigned >> 7) & 0x7F) as u8;
            let status = 0xE0 | (channel & 0x0F);
            Some([0x0E, status, lsb, msb])
        }
        MidiMessage::ActiveSensing => Some([0x0F, 0xFE, 0x00, 0x00]),
        MidiMessage::TimingClock   => Some([0x0F, 0xF8, 0x00, 0x00]),
    }
}

/// Decode a 4-byte USB MIDI 1.0 packet into a [`MidiMessage`].
///
/// Returns `None` for unrecognised or unsupported CIN codes.
fn usb_packet_to_message(packet: [u8; 4]) -> Option<MidiMessage> {
    let cin    = packet[0] & 0x0F; // cable-number is the high nibble; we ignore it
    let status = packet[1];
    let d1     = packet[2];
    let d2     = packet[3];

    let channel = status & 0x0F;

    match cin {
        0x08 => Some(MidiMessage::NoteOff { channel, note: d1, velocity: d2 }),
        0x09 => {
            // NoteOn with velocity 0 is conventionally treated as NoteOff.
            if d2 == 0 {
                Some(MidiMessage::NoteOff { channel, note: d1, velocity: 0 })
            } else {
                Some(MidiMessage::NoteOn { channel, note: d1, velocity: d2 })
            }
        }
        0x0B => Some(MidiMessage::ControlChange { channel, control: d1, value: d2 }),
        0x0C => Some(MidiMessage::ProgramChange { channel, program: d1 }),
        0x0E => {
            let raw = (d1 as u16) | ((d2 as u16) << 7);
            let value = raw as i16 - 8192;
            Some(MidiMessage::PitchBend { channel, value })
        }
        0x0F => match status {
            0xF8 => Some(MidiMessage::TimingClock),
            0xFE => Some(MidiMessage::ActiveSensing),
            _    => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn task(
    driver: UsbMidi,
    from_router: MidiReceiver<'static>,
    to_bus: MidiSender<'static>,
) {
    driver.run(from_router, to_bus).await;
}
