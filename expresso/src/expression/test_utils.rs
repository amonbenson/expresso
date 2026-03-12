use crate::midi::{MidiEndpoint, MidiMessage, MidiSink};

pub struct MessageCollector {
    pub messages: [(u8, u8, u8); 32],
    pub count: usize,
}

impl MessageCollector {
    pub fn new() -> Self {
        Self {
            messages: [(0, 0, 0); 32],
            count: 0,
        }
    }

    pub fn last(&self) -> (u8, u8, u8) {
        self.messages[self.count - 1]
    }
}

impl MidiSink for MessageCollector {
    fn emit(&mut self, message: MidiMessage, _target: Option<MidiEndpoint>) {
        if let MidiMessage::ControlChange {
            channel,
            control,
            value,
        } = message
        {
            self.messages[self.count] = (channel, control, value);
            self.count += 1;
        }
    }
}
