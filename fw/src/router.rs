use defmt::{info, warn};
use embassy_futures::select::{Either3, select3};

use crate::midi::{MidiMessageReceiver, MidiMessageSender};

enum MidiSource {
    Usb,
    Din,
    Exp,
}

#[embassy_executor::task]
pub async fn task(
    usb_midi_in: MidiMessageReceiver<'static>,
    din_midi_in: MidiMessageReceiver<'static>,
    exp_midi_in: MidiMessageReceiver<'static>,
    usb_midi_out: MidiMessageSender<'static>,
    din_midi_out: MidiMessageSender<'static>,
) {
    info!("Router task started (loopback: USB in → USB out)");

    loop {
        let (source, message) = match select3(
            usb_midi_in.receive(),
            din_midi_in.receive(),
            exp_midi_in.receive(),
        )
        .await
        {
            Either3::First(message) => (MidiSource::Usb, message),
            Either3::Second(message) => (MidiSource::Din, message),
            Either3::Third(message) => (MidiSource::Exp, message),
        };

        // full Loopback: send any message from any source to all outputs
        if usb_midi_out.try_send(message).is_err() {
            warn!("Router: USB output channel full, message dropped");
        }
        if din_midi_out.try_send(message).is_err() {
            warn!("Router: DIN output channel full, message dropped");
        }
    }
}
