use crate::settings::Settings;
use serde::{Deserialize, Serialize};

// Single-byte non-commercial manufacturer ID (MIDI spec 0x7D).
pub const SYSEX_MFID: u8 = 0x7D;

const SYSEX_CMD_VERSION_REQUEST: u8 = 0x01;
const SYSEX_CMD_VERSION_REPLY: u8 = 0x41;

const SYSEX_CMD_SETTINGS_GET: u8 = 0x02;
const SYSEX_CMD_SETTINGS_GET_REPLY: u8 = 0x42;

const SYSEX_CMD_SETTINGS_SET: u8 = 0x03;
const SYSEX_CMD_SETTINGS_SET_REPLY: u8 = 0x43;

// Settings: 4 channels × ~51 bytes = ~204 bytes minimum.
const MAX_SETTINGS_BYTES: usize = 256;

// Worst-case 7-bit-encoded size of MAX_SETTINGS_BYTES:
//   ceil(256 / 7) * 8 = 37 * 8 = 296 bytes
// Plus SysEx framing: 0xF0 + MFID + cmd + <data> + 0xF7 = 3 + 296 + 1 = 300 bytes minimum.
pub const SYSEX_RESPONSE_BUF_SIZE: usize = 320;

pub struct SysexResponse {
    pub data: [u8; SYSEX_RESPONSE_BUF_SIZE],
    pub len: usize,
}

/// Dispatches incoming SysEx messages and produces responses.
pub struct SysexDispatcher {
    version: (u8, u8, u8),
}

/// Encode raw bytes into 7-bit-safe MIDI SysEx data.
///
/// Every group of up to 7 input bytes is encoded as one MSB-collector byte
/// followed by those bytes with their MSBs cleared. Returns the number of
/// bytes written to `dst`.
fn encode_7bit(src: &[u8], dst: &mut [u8]) -> usize {
    let mut out = 0;
    let mut i = 0;
    while i < src.len() {
        let group_len = (src.len() - i).min(7);
        let msb_pos = out;
        dst[out] = 0;
        out += 1;
        for j in 0..group_len {
            if src[i + j] & 0x80 != 0 {
                dst[msb_pos] |= 1 << j;
            }
            dst[out] = src[i + j] & 0x7F;
            out += 1;
        }
        i += group_len;
    }
    out
}

/// Decode 7-bit-encoded MIDI SysEx data back to raw bytes.
///
/// Returns the number of bytes written, or `None` if `dst` is too small.
fn decode_7bit(src: &[u8], dst: &mut [u8]) -> Option<usize> {
    let mut out = 0;
    let mut i = 0;
    while i < src.len() {
        let msb = src[i];
        i += 1;
        let group_len = (src.len() - i).min(7);
        for j in 0..group_len {
            if out >= dst.len() {
                return None;
            }
            dst[out] = (src[i + j] & 0x7F) | ((msb >> j & 1) << 7);
            out += 1;
        }
        i += group_len;
    }
    Some(out)
}

impl SysexDispatcher {
    pub fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            version: (major, minor, patch),
        }
    }

    /// Handle a received SysEx payload (must include leading 0xF0 and trailing 0xF7).
    /// Returns `Some(response)` if a reply should be sent back.
    pub fn handle(&mut self, payload: &[u8], settings: &mut Settings) -> Option<SysexResponse>
    where
        Settings: Serialize + for<'de> Deserialize<'de>,
    {
        // Minimum: [0xF0, MFID, cmd, 0xF7]
        if payload.len() < 4 || payload[0] != 0xF0 || payload[1] != SYSEX_MFID {
            return None;
        }
        match payload[2] {
            SYSEX_CMD_VERSION_REQUEST => {
                let (major, minor, patch) = self.version;
                let mut r = SysexResponse {
                    data: [0; SYSEX_RESPONSE_BUF_SIZE],
                    len: 0,
                };
                r.data[0] = 0xF0;
                r.data[1] = SYSEX_MFID;
                r.data[2] = SYSEX_CMD_VERSION_REPLY;
                r.data[3] = major;
                r.data[4] = minor;
                r.data[5] = patch;
                r.data[6] = 0xF7;
                r.len = 7;
                Some(r)
            }

            SYSEX_CMD_SETTINGS_GET => {
                let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
                let serialized = postcard::to_slice(settings, &mut postcard_buf).ok()?;
                let serialized_len = serialized.len();

                let mut r = SysexResponse {
                    data: [0; SYSEX_RESPONSE_BUF_SIZE],
                    len: 0,
                };
                r.data[0] = 0xF0;
                r.data[1] = SYSEX_MFID;
                r.data[2] = SYSEX_CMD_SETTINGS_GET_REPLY;
                // encode_7bit writes into r.data[3..], which has SYSEX_RESPONSE_BUF_SIZE-3 bytes.
                let encoded_len = encode_7bit(&postcard_buf[..serialized_len], &mut r.data[3..]);
                r.data[3 + encoded_len] = 0xF7;
                r.len = 3 + encoded_len + 1;
                Some(r)
            }

            SYSEX_CMD_SETTINGS_SET => {
                // payload: [0xF0, MFID, cmd, <7-bit data>, 0xF7]
                let data = &payload[3..payload.len() - 1];
                let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
                let decoded_len = decode_7bit(data, &mut postcard_buf)?;
                *settings = postcard::from_bytes(&postcard_buf[..decoded_len]).ok()?;

                let mut r = SysexResponse {
                    data: [0; SYSEX_RESPONSE_BUF_SIZE],
                    len: 0,
                };
                r.data[0] = 0xF0;
                r.data[1] = SYSEX_MFID;
                r.data[2] = SYSEX_CMD_SETTINGS_SET_REPLY;
                r.data[3] = 0xF7;
                r.len = 4;
                Some(r)
            }

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn version_request() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        let r = d.handle(&[0xF0, SYSEX_MFID, SYSEX_CMD_VERSION_REQUEST, 0xF7], &mut s).unwrap();
        assert_eq!(&r.data[..r.len], &[0xF0, SYSEX_MFID, SYSEX_CMD_VERSION_REPLY, 0, 1, 0, 0xF7]);
    }

    #[test]
    fn unknown_command_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(d.handle(&[0xF0, SYSEX_MFID, 0xFF, 0xF7], &mut s).is_none());
    }

    #[test]
    fn wrong_mfid_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(d.handle(&[0xF0, 0x41, 0x01, 0xF7], &mut s).is_none());
    }

    #[test]
    fn too_short_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(d.handle(&[0xF0, SYSEX_MFID, 0xF7], &mut s).is_none());
    }

    #[test]
    fn encode_decode_7bit_roundtrip() {
        // All 256 byte values to exercise MSB handling
        let src: [u8; 256] = core::array::from_fn(|i| i as u8);
        // ceil(256/7)*8 = 296 bytes needed for encoded output
        let mut encoded = [0u8; 296];
        let enc_len = encode_7bit(&src, &mut encoded);

        // All encoded bytes must be 7-bit safe
        for &b in &encoded[..enc_len] {
            assert!(b < 0x80, "encoded byte {b:#04x} is not 7-bit safe");
        }

        let mut decoded = [0u8; 256];
        let dec_len = decode_7bit(&encoded[..enc_len], &mut decoded).unwrap();
        assert_eq!(dec_len, src.len());
        assert_eq!(&decoded[..dec_len], &src[..]);
    }

    #[test]
    fn settings_get_reply_is_7bit_safe() {
        let mut d = SysexDispatcher::new(1, 2, 3);
        let mut s = Settings::default();
        let r = d.handle(&[0xF0, SYSEX_MFID, SYSEX_CMD_SETTINGS_GET, 0xF7], &mut s).unwrap();
        assert_eq!(r.data[0], 0xF0);
        assert_eq!(r.data[1], SYSEX_MFID);
        assert_eq!(r.data[2], SYSEX_CMD_SETTINGS_GET_REPLY);
        assert_eq!(r.data[r.len - 1], 0xF7);
        // All data bytes between cmd and 0xF7 must be 7-bit safe
        for &b in &r.data[3..r.len - 1] {
            assert!(b < 0x80, "data byte {b:#04x} is not 7-bit safe");
        }
    }

    #[test]
    fn settings_get_set_roundtrip() {
        let mut d = SysexDispatcher::new(1, 0, 0);
        let mut s = Settings::default();
        // Modify fields so the roundtrip is non-trivial
        s.expression.channels[0].cc = 42;
        s.expression.channels[1].cc = 99;

        // Get
        let get_reply = d.handle(&[0xF0, SYSEX_MFID, SYSEX_CMD_SETTINGS_GET, 0xF7], &mut s).unwrap();
        assert_eq!(get_reply.data[2], SYSEX_CMD_SETTINGS_GET_REPLY);

        // Use the reply payload as a set command: copy into a fixed buffer, swap the cmd byte.
        let mut set_payload = [0u8; SYSEX_RESPONSE_BUF_SIZE];
        set_payload[..get_reply.len].copy_from_slice(&get_reply.data[..get_reply.len]);
        set_payload[2] = SYSEX_CMD_SETTINGS_SET;

        // Apply to a fresh settings object
        let mut s2 = Settings::default();
        let ack = d.handle(&set_payload[..get_reply.len], &mut s2).unwrap();
        assert_eq!(ack.data[2], SYSEX_CMD_SETTINGS_SET_REPLY);
        assert_eq!(ack.len, 4);

        assert_eq!(s2.expression.channels[0].cc, 42);
        assert_eq!(s2.expression.channels[1].cc, 99);
    }
}
