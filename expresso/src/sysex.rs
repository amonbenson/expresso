use crate::settings::Settings;

// Single-byte non-commercial manufacturer ID (MIDI spec 0x7D).
pub const SYSEX_MFID: u8 = 0x7D;

const SYSEX_CMD_VERSION_REQUEST: u8 = 0x01;
const SYSEX_CMD_VERSION_REPLY: u8 = 0x02;

pub const SYSEX_RESPONSE_BUF_SIZE: usize = 32;

pub struct SysexResponse {
    pub data: [u8; SYSEX_RESPONSE_BUF_SIZE],
    pub len: usize,
}

/// Dispatches incoming SysEx messages and produces responses.
/// Holds the firmware version reported by the version inquiry command.
pub struct SysexDispatcher {
    version: (u8, u8, u8),
}

impl SysexDispatcher {
    pub fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            version: (major, minor, patch),
        }
    }

    /// Handle a received SysEx payload (must include leading 0xF0 and trailing 0xF7).
    /// Returns `Some(response)` if a reply should be sent back.
    pub fn handle<const C: usize>(&mut self, payload: &[u8], _settings: &mut Settings<C>) -> Option<SysexResponse> {
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
        let mut s = Settings::<4>::default();
        let r = d.handle(&[0xF0, SYSEX_MFID, 0x01, 0xF7], &mut s).unwrap();
        assert_eq!(&r.data[..r.len], &[0xF0, SYSEX_MFID, 0x02, 0, 1, 0, 0xF7]);
    }

    #[test]
    fn unknown_command_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::<4>::default();
        assert!(d.handle(&[0xF0, SYSEX_MFID, 0xFF, 0xF7], &mut s).is_none());
    }

    #[test]
    fn wrong_mfid_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::<4>::default();
        assert!(d.handle(&[0xF0, 0x41, 0x01, 0xF7], &mut s).is_none());
    }

    #[test]
    fn too_short_returns_none() {
        let mut d = SysexDispatcher::new(0, 1, 0);
        let mut s = Settings::<4>::default();
        assert!(d.handle(&[0xF0, SYSEX_MFID, 0xF7], &mut s).is_none());
    }
}
