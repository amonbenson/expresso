pub const EXPRESSION_POLL_HZ: u64 = 100;

const fn parse_u8(s: &str) -> u8 {
    let b = s.as_bytes();
    let mut val: u8 = 0;
    let mut i = 0;
    while i < b.len() {
        val = val * 10 + (b[i] - b'0');
        i += 1;
    }
    val
}

pub const FW_VERSION_MAJOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MAJOR"));
pub const FW_VERSION_MINOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MINOR"));
pub const FW_VERSION_PATCH: u8 = parse_u8(env!("CARGO_PKG_VERSION_PATCH"));
