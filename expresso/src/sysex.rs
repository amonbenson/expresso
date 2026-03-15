use crate::settings::{Settings, SettingsPatch};
use crate::status::StatusEvent;
use serde::{Deserialize, Serialize};

pub const SYSEX_MFID: u8 = 0x7D;
pub const SYSEX_MAGIC: [u8; 4] = [0x6F, 0x2E, 0x55, 0x00];

pub const SYSEX_CMD_VERSION_REQUEST: u8 = 0x00;
pub const SYSEX_CMD_SETTINGS_GET: u8 = 0x01;
pub const SYSEX_CMD_SETTINGS_SET: u8 = 0x02;
pub const SYSEX_CMD_SETTINGS_PATCH: u8 = 0x03;
/// Push notification: firmware → host. Carries a serialized [`StatusEvent`].
pub const SYSEX_CMD_STATUS: u8 = 0x04;
pub const SYSEX_RESPONSE_BIT: u8 = 0x40;

// Settings: 4 expression channels × ~57 bytes + StatusSettings ~24 bytes ≈ 252 bytes.
// Use 320 to leave headroom for future additions.
pub const MAX_SETTINGS_BYTES: usize = 320;

// Worst-case 7-bit-encoded size of MAX_SETTINGS_BYTES:
//   ceil(320 / 7) * 8 = 46 * 8 = 368 bytes
// Plus SysEx framing: 0xF0 + MFID + cmd + <data> + 0xF7 = 7 + 368 + 1 = 376 bytes.
pub const SYSEX_RESPONSE_BUF_SIZE: usize = 400;

pub struct SysexResponse {
    pub data: [u8; SYSEX_RESPONSE_BUF_SIZE],
    pub len: usize,
}

impl Default for SysexResponse {
    fn default() -> Self {
        Self {
            data: [0; SYSEX_RESPONSE_BUF_SIZE],
            len: Default::default(),
        }
    }
}

/// Dispatches incoming SysEx messages and produces responses.
pub struct SysexDispatcher {
    version: (u8, u8, u8),
}

pub mod codec_7bit {
    pub fn encode(src: &[u8], dst: &mut [u8]) -> usize {
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

    pub fn decode(src: &[u8], dst: &mut [u8]) -> Option<usize> {
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
}

/// Encode a [`StatusEvent`] as a SysEx push notification (firmware → host).
///
/// The resulting frame uses [`SYSEX_CMD_STATUS`] with the response bit set
/// since it is always an unsolicited notification rather than a reply.
/// Returns `None` if the internal postcard serialization buffer overflows
/// (should never happen for the current event set).
pub fn encode_status_event(event: StatusEvent) -> Option<SysexResponse> {
    let mut res = SysexResponse::default();
    res.data[0] = 0xF0;
    res.data[1] = SYSEX_MFID;
    res.data[2] = SYSEX_MAGIC[0];
    res.data[3] = SYSEX_MAGIC[1];
    res.data[4] = SYSEX_MAGIC[2];
    res.data[5] = SYSEX_MAGIC[3];
    res.data[6] = SYSEX_CMD_STATUS | SYSEX_RESPONSE_BIT;

    // StatusEvent serialises to at most a few bytes; 16 is more than enough.
    let mut postcard_buf = [0u8; 16];
    let serialized = postcard::to_slice(&event, &mut postcard_buf).ok()?;
    let encoded_len = codec_7bit::encode(serialized, &mut res.data[7..]);
    res.data[7 + encoded_len] = 0xF7;
    res.len = 7 + encoded_len + 1;
    Some(res)
}

impl SysexDispatcher {
    pub fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            version: (major, minor, patch),
        }
    }

    pub fn handle(&mut self, req: &[u8], settings: &mut Settings) -> Option<SysexResponse>
    where
        Settings: Serialize + for<'de> Deserialize<'de>,
    {
        // Check the packet format:
        // F0 <MFID> <M0> <M1> <M2> <M3> <cmd> ... 7F
        if req.len() < 8 || req[0] != 0xF0 || req[1] != SYSEX_MFID || req[2..6] != SYSEX_MAGIC {
            return None;
        }

        // Prepare the response buffer
        let cmd = req[6];
        let mut res = SysexResponse::default();
        res.data[0] = 0xF0;
        res.data[1] = SYSEX_MFID;
        res.data[2] = SYSEX_MAGIC[0];
        res.data[3] = SYSEX_MAGIC[1];
        res.data[4] = SYSEX_MAGIC[2];
        res.data[5] = SYSEX_MAGIC[3];
        res.data[6] = cmd | 0x40; // set the response bit

        match cmd {
            SYSEX_CMD_VERSION_REQUEST => {
                let (major, minor, patch) = self.version;
                res.data[7] = major;
                res.data[8] = minor;
                res.data[9] = patch;
                res.data[10] = 0xF7;
                res.len = 11;
                Some(res)
            }

            SYSEX_CMD_SETTINGS_GET => {
                let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
                let serialized = postcard::to_slice(settings, &mut postcard_buf).ok()?;
                let serialized_len = serialized.len();

                let encoded_len =
                    codec_7bit::encode(&postcard_buf[..serialized_len], &mut res.data[7..]);
                res.data[7 + encoded_len] = 0xF7;
                res.len = 7 + encoded_len + 1;
                Some(res)
            }

            SYSEX_CMD_SETTINGS_SET => {
                let data = &req[7..req.len() - 1];
                let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
                let decoded_len = codec_7bit::decode(data, &mut postcard_buf)?;
                *settings = postcard::from_bytes(&postcard_buf[..decoded_len]).ok()?;

                res.data[7] = 0xF7;
                res.len = 8;
                Some(res)
            }

            SYSEX_CMD_SETTINGS_PATCH => {
                let data = &req[7..req.len() - 1];
                let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
                let decoded_len = codec_7bit::decode(data, &mut postcard_buf)?;
                let patch: SettingsPatch =
                    postcard::from_bytes(&postcard_buf[..decoded_len]).ok()?;
                settings.apply_patch(patch);

                res.data[7] = 0xF7;
                res.len = 8;
                Some(res)
            }

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::status::StatusEvent;

    #[test]
    fn version_request() {
        let mut d = SysexDispatcher::new(1, 2, 3);
        let mut s = Settings::default();
        let r = d
            .handle(
                &[
                    0xF0,
                    SYSEX_MFID,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    SYSEX_CMD_VERSION_REQUEST,
                    0xF7,
                ],
                &mut s,
            )
            .unwrap();
        assert_eq!(
            &r.data[..r.len],
            &[
                0xF0,
                SYSEX_MFID,
                SYSEX_MAGIC[0],
                SYSEX_MAGIC[1],
                SYSEX_MAGIC[2],
                SYSEX_MAGIC[3],
                SYSEX_CMD_VERSION_REQUEST | 0x40,
                1,
                2,
                3,
                0xF7
            ]
        );
    }

    #[test]
    fn unknown_command_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(
            d.handle(
                &[
                    0xF0,
                    SYSEX_MFID,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    0xFF,
                    0xF7
                ],
                &mut s
            )
            .is_none()
        );
    }

    #[test]
    fn wrong_mfid_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(
            d.handle(
                &[
                    0xF0,
                    0x41,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    0x01,
                    0xF7
                ],
                &mut s
            )
            .is_none()
        );
    }

    #[test]
    fn too_short_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::default();
        assert!(
            d.handle(
                &[
                    0xF0,
                    SYSEX_MFID,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    0xF7
                ],
                &mut s
            )
            .is_none()
        );
    }

    #[test]
    fn encode_decode_7bit_roundtrip() {
        // All 256 byte values to exercise MSB handling
        let src: [u8; 256] = core::array::from_fn(|i| i as u8);
        // ceil(256/7)*8 = 296 bytes needed for encoded output
        let mut encoded = [0u8; 296];
        let enc_len = codec_7bit::encode(&src, &mut encoded);

        // All encoded bytes must be 7-bit safe
        for &b in &encoded[..enc_len] {
            assert!(b < 0x80, "encoded byte {b:#04x} is not 7-bit safe");
        }

        let mut decoded = [0u8; 256];
        let dec_len = codec_7bit::decode(&encoded[..enc_len], &mut decoded).unwrap();
        assert_eq!(dec_len, src.len());
        assert_eq!(&decoded[..dec_len], &src[..]);
    }

    #[test]
    fn settings_get_reply_is_7bit_safe() {
        let mut d = SysexDispatcher::new(1, 2, 3);
        let mut s = Settings::default();
        let r = d
            .handle(
                &[
                    0xF0,
                    SYSEX_MFID,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    SYSEX_CMD_SETTINGS_GET,
                    0xF7,
                ],
                &mut s,
            )
            .unwrap();
        assert_eq!(r.data[0], 0xF0);
        assert_eq!(r.data[1], SYSEX_MFID);
        assert_eq!(r.data[2], SYSEX_MAGIC[0]);
        assert_eq!(r.data[3], SYSEX_MAGIC[1]);
        assert_eq!(r.data[4], SYSEX_MAGIC[2]);
        assert_eq!(r.data[5], SYSEX_MAGIC[3]);
        assert_eq!(r.data[6], SYSEX_CMD_SETTINGS_GET | 0x40);
        assert_eq!(r.data[r.len - 1], 0xF7);
        // All data bytes between cmd and 0xF7 must be 7-bit safe
        for &b in &r.data[7..r.len - 1] {
            assert!(b < 0x80, "data byte {b:#04x} is not 7-bit safe");
        }
    }

    #[test]
    fn settings_patch_applies_single_field() {
        use crate::settings::{ExpressionChannelSettingsPatch, SettingsPatch};

        let mut d = SysexDispatcher::new(1, 0, 0);
        let mut s = Settings::default();
        s.expression.channels[0].cc = 10;
        s.expression.channels[1].cc = 20;

        // Build patch: set channel 0 CC to 77
        let patch = SettingsPatch::ExpressionChannel(0, ExpressionChannelSettingsPatch::CC(77));
        let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
        let serialized = postcard::to_slice(&patch, &mut postcard_buf).unwrap();
        let serialized_len = serialized.len();

        // 7-bit encode into a SysEx frame
        let mut req = [0u8; SYSEX_RESPONSE_BUF_SIZE];
        req[0] = 0xF0;
        req[1] = SYSEX_MFID;
        req[2] = SYSEX_MAGIC[0];
        req[3] = SYSEX_MAGIC[1];
        req[4] = SYSEX_MAGIC[2];
        req[5] = SYSEX_MAGIC[3];
        req[6] = SYSEX_CMD_SETTINGS_PATCH;
        let encoded_len = codec_7bit::encode(&postcard_buf[..serialized_len], &mut req[7..]);
        req[7 + encoded_len] = 0xF7;
        let req_len = 7 + encoded_len + 1;

        let ack = d.handle(&req[..req_len], &mut s).unwrap();
        assert_eq!(ack.data[6], SYSEX_CMD_SETTINGS_PATCH | 0x40);
        assert_eq!(ack.len, 8);

        // Only channel 0 CC should have changed
        assert_eq!(s.expression.channels[0].cc, 77);
        assert_eq!(s.expression.channels[1].cc, 20);
    }

    #[test]
    fn encode_status_event_is_valid_sysex() {
        use crate::midi::{MidiEndpoint, MidiMessage};
        use crate::status::MidiDirection;
        let cc = MidiMessage::ControlChange {
            channel: 0,
            control: 0,
            value: 0,
        };
        let events = [
            StatusEvent::Power(true),
            StatusEvent::Power(false),
            StatusEvent::UsbConnected(true),
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: cc,
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Din,
                direction: MidiDirection::Out,
                message: cc,
            },
            StatusEvent::SettingsUpdate,
        ];
        for event in events {
            let r = encode_status_event(event).unwrap();
            assert_eq!(r.data[0], 0xF0, "missing SysEx start for {event:?}");
            assert_eq!(r.data[1], SYSEX_MFID);
            assert_eq!(r.data[2..6], SYSEX_MAGIC);
            assert_eq!(
                r.data[6],
                SYSEX_CMD_STATUS | SYSEX_RESPONSE_BIT,
                "wrong cmd byte for {event:?}"
            );
            assert_eq!(r.data[r.len - 1], 0xF7, "missing SysEx end for {event:?}");
            // All data bytes between header and 0xF7 must be 7-bit safe
            for &b in &r.data[7..r.len - 1] {
                assert!(
                    b < 0x80,
                    "data byte {b:#04x} is not 7-bit safe for {event:?}"
                );
            }
        }
    }

    #[test]
    fn encode_status_event_roundtrip() {
        use crate::midi::{MidiEndpoint, MidiMessage};
        use crate::status::{MidiDirection, StatusEvent};
        let cc = MidiMessage::ControlChange {
            channel: 1,
            control: 7,
            value: 64,
        };
        let events = [
            StatusEvent::Power(true),
            StatusEvent::Power(false),
            StatusEvent::UsbConnected(false),
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: cc,
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::Out,
                message: cc,
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Din,
                direction: MidiDirection::In,
                message: cc,
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Din,
                direction: MidiDirection::Out,
                message: cc,
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Expression,
                direction: MidiDirection::Out,
                message: cc,
            },
            StatusEvent::SettingsUpdate,
        ];
        for event in events {
            let r = encode_status_event(event).unwrap();
            // Decode: strip framing (bytes 7..len-1 are 7-bit encoded payload)
            let encoded_payload = &r.data[7..r.len - 1];
            let mut postcard_buf = [0u8; 16];
            let decoded_len = codec_7bit::decode(encoded_payload, &mut postcard_buf).unwrap();
            let decoded: StatusEvent = postcard::from_bytes(&postcard_buf[..decoded_len]).unwrap();
            assert_eq!(decoded, event);
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
        let get_reply = d
            .handle(
                &[
                    0xF0,
                    SYSEX_MFID,
                    SYSEX_MAGIC[0],
                    SYSEX_MAGIC[1],
                    SYSEX_MAGIC[2],
                    SYSEX_MAGIC[3],
                    SYSEX_CMD_SETTINGS_GET,
                    0xF7,
                ],
                &mut s,
            )
            .unwrap();
        assert_eq!(get_reply.data[6], SYSEX_CMD_SETTINGS_GET | 0x40);

        // Use the reply payload as a set command: copy into a fixed buffer, swap the cmd byte.
        let mut set_payload = [0u8; SYSEX_RESPONSE_BUF_SIZE];
        set_payload[..get_reply.len].copy_from_slice(&get_reply.data[..get_reply.len]);
        set_payload[6] = SYSEX_CMD_SETTINGS_SET;

        // Apply to a fresh settings object
        let mut s2 = Settings::default();
        let ack = d.handle(&set_payload[..get_reply.len], &mut s2).unwrap();
        assert_eq!(ack.data[6], SYSEX_CMD_SETTINGS_SET | 0x40);
        assert_eq!(ack.len, 8);

        assert_eq!(s2.expression.channels[0].cc, 42);
        assert_eq!(s2.expression.channels[1].cc, 99);
    }
}
