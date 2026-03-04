pub const DIN_MIDI_BAUD: u32 = 31_250;

/// How often the expression inputs are sampled.
pub const EXPRESSION_POLL_HZ: u64 = 100;

/// MIDI CC number for each expression channel (jacks 0-3).
pub const EXPRESSION_CC: [u8; 4] = [11, 1, 7, 74];

/// MIDI channel for all expression output messages (0-indexed, 0 = channel 1).
pub const EXPRESSION_MIDI_CH: u8 = 0;
