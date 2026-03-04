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
    info!("Router task started");

    // loop {
    //     let event = from_sources.receive().await;
    //     let (send_usb, send_din) = routing.route(&event);

    //     if send_usb {
    //         if to_usb.try_send(event).is_err() {
    //             warn!("USB output channel full. MIDI event dropped");
    //         }
    //     }
    //     if send_din {
    //         if to_din.try_send(event).is_err() {
    //             warn!("DIN output channel full. MIDI event dropped");
    //         }
    //     }
    // }

    core::future::pending::<()>().await;
}
