use defmt::{info, warn};
use embassy_futures::select::{select, Either};
use embassy_stm32::usb::Driver;
use embassy_stm32::{peripherals, Peri};
use embassy_usb::UsbDevice;
use embassy_usb::class::midi::{MidiClass, Receiver as MidiRx, Sender as MidiTx};
use embassy_usb::Builder;
use static_cell::StaticCell;

use crate::midi::{MidiEvent, MidiMessage, MidiPeripheral, MidiReceiver, MidiSender};

// ---------------------------------------------------------------------------
// Convenience type aliases
// ---------------------------------------------------------------------------

pub type UsbDriver  = Driver<'static, peripherals::USB>;
pub type UsbDev     = UsbDevice<'static, UsbDriver>;
pub type UsbMidi    = MidiClass<'static, UsbDriver>;

// ---------------------------------------------------------------------------
// Build — mirrors the inline CDC setup in main.rs, adapted for MIDI
// ---------------------------------------------------------------------------

/// Initialise the USB driver, build the USB device, and register the
/// USB MIDI 1.0 class.  Returns the device handle (for [`device_task`])
/// and the MIDI class handle (for [`io_task`]).
///
/// Must be called exactly once; uses `StaticCell` for descriptor buffers.
pub fn build(
    usb: Peri<'static, peripherals::USB>,
    dp:  Peri<'static, peripherals::PA12>,
    dm:  Peri<'static, peripherals::PA11>,
) -> (UsbDev, UsbMidi) {
    let driver = Driver::new(usb, crate::Irqs, dp, dm);

    let usb_config = {
        let mut c = embassy_usb::Config::new(0x1209, 0x2156);
        c.manufacturer    = Some("Amon Benson");
        c.product         = Some("Expresso");
        c.serial_number   = Some("12345678");
        c.max_power       = 100;
        c.max_packet_size_0 = 64;
        c
    };

    let mut builder = {
        static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESC:    StaticCell<[u8; 32]>  = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]>  = StaticCell::new();

        Builder::new(
            driver,
            usb_config,
            CONFIG_DESC.init([0; 256]),
            BOS_DESC.init([0; 32]),
            &mut [], // no MSOS descriptors
            CONTROL_BUF.init([0; 64]),
        )
    };

    let midi = MidiClass::new(&mut builder, 1, 1, 64);
    let device = builder.build();

    (device, midi)
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

/// Runs the USB device stack — equivalent to `usb_task` in main.rs.
#[embassy_executor::task]
pub async fn device_task(mut usb: UsbDev) -> ! {
    usb.run().await
}

/// Handles USB MIDI I/O: forwards events from the router to the USB host
/// (TX) and from the USB host to the event bus (RX).
///
/// Re-connects automatically whenever the host disconnects.
#[embassy_executor::task]
pub async fn io_task(
    midi: UsbMidi,
    from_router: MidiReceiver<'static>,
    to_bus: MidiSender<'static>,
) {
    info!("USB MIDI IO task started");

    let (mut tx, mut rx) = midi.split();

    loop {
        tx.wait_connection().await;
        info!("USB MIDI host connected");

        match select(tx_loop(&mut tx, &from_router), rx_loop(&mut rx, &to_bus)).await {
            Either::First(_)  => warn!("USB MIDI TX loop exited"),
            Either::Second(_) => warn!("USB MIDI RX loop exited"),
        }

        info!("USB MIDI host disconnected, waiting for reconnect");
    }
}

// ---------------------------------------------------------------------------
// TX — bus → USB host
// ---------------------------------------------------------------------------

async fn tx_loop(tx: &mut MidiTx<'static, UsbDriver>, from_router: &MidiReceiver<'static>) {
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

async fn rx_loop(rx: &mut MidiRx<'static, UsbDriver>, to_bus: &MidiSender<'static>) {
    let mut buf = [0u8; 64];
    loop {
        let n = match rx.read_packet(&mut buf).await {
            Ok(n)  => n,
            Err(_) => return,
        };

        for chunk in buf[..n].chunks_exact(4) {
            if let Some(message) = usb_packet_to_message([chunk[0], chunk[1], chunk[2], chunk[3]]) {
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

fn event_to_usb_packet(event: &MidiEvent) -> Option<[u8; 4]> {
    match event.message {
        MidiMessage::NoteOn { channel, note, velocity } =>
            Some([0x09, 0x90 | (channel & 0x0F), note, velocity]),
        MidiMessage::NoteOff { channel, note, velocity } =>
            Some([0x08, 0x80 | (channel & 0x0F), note, velocity]),
        MidiMessage::ControlChange { channel, control, value } =>
            Some([0x0B, 0xB0 | (channel & 0x0F), control, value]),
        MidiMessage::ProgramChange { channel, program } =>
            Some([0x0C, 0xC0 | (channel & 0x0F), program, 0x00]),
        MidiMessage::PitchBend { channel, value } => {
            let u = (value + 8192) as u16;
            Some([0x0E, 0xE0 | (channel & 0x0F), (u & 0x7F) as u8, ((u >> 7) & 0x7F) as u8])
        }
        MidiMessage::ActiveSensing => Some([0x0F, 0xFE, 0x00, 0x00]),
        MidiMessage::TimingClock   => Some([0x0F, 0xF8, 0x00, 0x00]),
    }
}

fn usb_packet_to_message(packet: [u8; 4]) -> Option<MidiMessage> {
    let cin     = packet[0] & 0x0F;
    let status  = packet[1];
    let d1      = packet[2];
    let d2      = packet[3];
    let channel = status & 0x0F;

    match cin {
        0x08 => Some(MidiMessage::NoteOff { channel, note: d1, velocity: d2 }),
        0x09 => {
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
            Some(MidiMessage::PitchBend { channel, value: raw as i16 - 8192 })
        }
        0x0F => match status {
            0xF8 => Some(MidiMessage::TimingClock),
            0xFE => Some(MidiMessage::ActiveSensing),
            _    => None,
        },
        _ => None,
    }
}
