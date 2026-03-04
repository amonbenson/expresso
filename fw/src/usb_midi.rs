/// USB MIDI driver task.
///
/// Owns the USB peripheral and presents a USB-MIDI 1.0 device to the host.
///
/// - **Inbound** (USB → bus): bytes received from the host are parsed into
///   [`MidiEvent`]s and placed on the shared event bus via `to_bus`.
/// - **Outbound** (bus → USB): [`MidiEvent`]s delivered by the router via
///   `from_router` are serialised and sent to the host.
///
/// Pins (from expresso.ioc):
///   PA11 = USB_DM, PA12 = USB_DP
use defmt::info;
use embassy_stm32::{peripherals, Peri};

use crate::midi::{MidiReceiver, MidiSender};

#[embassy_executor::task]
pub async fn task(
    usb: Peri<'static, peripherals::USB>,
    dp: Peri<'static, peripherals::PA12>,
    dm: Peri<'static, peripherals::PA11>,
    // Events from the router to forward to the USB host.
    from_router: MidiReceiver<'static>,
    // Events received from the USB host to place on the bus.
    to_bus: MidiSender<'static>,
) {
    // Suppress unused-variable warnings while this is a stub.
    let _ = (usb, dp, dm, from_router, to_bus);

    info!("USB MIDI task started");

    // TODO: Initialise embassy-usb with the USB MIDI 1.0 class and enter the
    //       event loop. Rough structure:
    //
    //   let driver = Driver::new(usb, crate::Irqs, dp, dm);
    //   let config = embassy_usb::Config::new(0x1209, 0x2156);
    //   // ... set manufacturer / product strings ...
    //   let mut config_descriptor = [0u8; 256];
    //   let mut bos_descriptor   = [0u8; 256];
    //   let mut control_buf      = [0u8; 64];
    //   let mut builder = Builder::new(
    //       driver, config,
    //       &mut config_descriptor, &mut bos_descriptor,
    //       &mut [], &mut control_buf,
    //   );
    //   let mut midi_class = MidiClass::new(&mut builder, 1, 1, 64);
    //   let mut usb_dev = builder.build();
    //
    //   join(usb_dev.run(), midi_io_loop(&mut midi_class, &to_bus, &from_router)).await;

    core::future::pending::<()>().await;
}
