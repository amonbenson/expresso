use defmt::info;
use embassy_stm32::{peripherals, Peri};

use crate::midi::{MidiBridge, MidiReceiver, MidiSender};

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
        let _ = (self.usb, self.dp, self.dm, from_router, to_bus);

        info!("USB MIDI task started");

        // TODO: Initialise embassy-usb with the USB MIDI 1.0 class and enter the
        //       event loop. Rough structure:
        //
        //   let driver = Driver::new(self.usb, crate::Irqs, self.dp, self.dm);
        //   let config = embassy_usb::Config::new(0x1209, 0x2156);
        //   let mut builder = Builder::new(driver, config, /* descriptor bufs */);
        //   let mut midi_class = MidiClass::new(&mut builder, 1, 1, 64);
        //   let mut usb_dev = builder.build();
        //   join(usb_dev.run(), midi_io_loop(&mut midi_class, &from_router, &to_bus)).await;

        core::future::pending::<()>().await;
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
