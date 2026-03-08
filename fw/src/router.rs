use defmt::warn;
use expresso::midi::{MidiEndpoint, MidiHandler, MidiMessage, MidiSink};
use expresso::router::Router;

use crate::{InMsgReceiver, MsgSender, SettingsMutex};

// Routes decoded messages to the appropriate output channel based on the
// target endpoint chosen by the library's Router component.
struct RouterSink {
    to_usb: MsgSender,
    to_din: MsgSender,
}

impl MidiSink for RouterSink {
    fn emit(&mut self, message: MidiMessage, target: Option<MidiEndpoint>) {
        match target {
            Some(MidiEndpoint::Usb) => {
                if self.to_usb.try_send(message).is_err() {
                    warn!("Router: USB output full, message dropped");
                }
            }
            Some(MidiEndpoint::Din) => {
                if self.to_din.try_send(message).is_err() {
                    warn!("Router: DIN output full, message dropped");
                }
            }
            _ => {}
        }
    }
}

#[embassy_executor::task]
pub async fn task(
    from: InMsgReceiver,
    to_usb: MsgSender,
    to_din: MsgSender,
    settings: &'static SettingsMutex,
) {
    let mut router = Router::new();
    let mut sink = RouterSink { to_usb, to_din };

    loop {
        let (message, source) = from.receive().await;
        settings.lock(|s| {
            router
                .handle_message(message, source, &mut sink, &mut s.borrow_mut())
                .unwrap();
        });
    }
}
