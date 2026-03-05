use defmt::{info, warn};

use crate::midi::{MidiEvent, MidiReceiver, MidiSender, MidiPeripheral};

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn task(
    from_sources: MidiReceiver<'static>,
    to_usb: MidiSender<'static>,
    to_din: MidiSender<'static>,
) {
    info!("Router task started (loopback: USB in → USB out)");

    loop {
        let event = from_sources.receive().await;

        // Loopback: forward everything that arrives from USB back out to USB.
        let send_usb = matches!(event.source, MidiPeripheral::Usb);

        if send_usb {
            if to_usb.try_send(event).is_err() {
                warn!("Router: USB output channel full, event dropped");
            }
        }

        // to_din unused during loopback test — suppress the dead-code path.
        let _ = &to_din;
    }
}
