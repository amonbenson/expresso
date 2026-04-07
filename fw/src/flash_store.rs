use defmt::{info, warn};
use embassy_stm32::flash::{Blocking, Flash};
use expresso::settings::Settings;
use expresso::sysex::MAX_SETTINGS_BYTES;

/// Total flash size of the STM32G431CB.
const FLASH_SIZE: u32 = 128 * 1024;

/// Flash page size for STM32G4 (2 KiB).
const PAGE_SIZE: u32 = 2048;

/// Byte offset of the settings page — last page of flash.
pub const SETTINGS_OFFSET: u32 = FLASH_SIZE - PAGE_SIZE;

/// Magic marker at the start of a valid settings record.
const MAGIC: [u8; 4] = [0x45, 0x58, 0x50, 0x52]; // "EXPR"

/// Header layout: MAGIC (4 B) + payload length as u16 LE (2 B).
const HEADER_SIZE: usize = 6;

/// Minimum write granularity for STM32G4 flash (doubleword = 8 bytes).
const WRITE_ALIGN: usize = 8;

/// Try to read settings from the settings page.
///
/// Flash is memory-mapped, so no peripheral is needed for reads.
/// Returns `None` when the page is blank or the stored data is corrupt.
pub fn load() -> Option<Settings> {
    let base = (0x0800_0000u32 + SETTINGS_OFFSET) as *const u8;
    // Safety: address is within the device's flash memory map.
    let page = unsafe { core::slice::from_raw_parts(base, PAGE_SIZE as usize) };

    if page[..4] != MAGIC {
        info!("flash_store: no valid settings (magic mismatch)");
        return None;
    }

    let len = u16::from_le_bytes([page[4], page[5]]) as usize;
    if len == 0 || len > MAX_SETTINGS_BYTES {
        warn!("flash_store: invalid payload length {}", len);
        return None;
    }

    match postcard::from_bytes(&page[HEADER_SIZE..HEADER_SIZE + len]) {
        Ok(s) => {
            info!("flash_store: loaded settings ({} bytes)", len);
            Some(s)
        }
        Err(_) => {
            warn!("flash_store: deserialization failed");
            None
        }
    }
}

/// Erase the settings page and write the new settings.
///
/// Returns `true` on success.
pub fn save(flash: &mut Flash<'_, Blocking>, settings: &Settings) -> bool {
    let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
    let Ok(serialized) = postcard::to_slice(settings, &mut postcard_buf) else {
        warn!("flash_store: serialization failed");
        return false;
    };
    let len = serialized.len();

    // Pad total write length to WRITE_ALIGN (hardware requirement).
    let content_size = HEADER_SIZE + len;
    let write_size = (content_size + WRITE_ALIGN - 1) & !(WRITE_ALIGN - 1);

    let mut buf = [0xFFu8; PAGE_SIZE as usize];
    buf[..4].copy_from_slice(&MAGIC);
    buf[4..6].copy_from_slice(&(len as u16).to_le_bytes());
    buf[6..6 + len].copy_from_slice(&postcard_buf[..len]);

    if flash
        .blocking_erase(SETTINGS_OFFSET, SETTINGS_OFFSET + PAGE_SIZE)
        .is_err()
    {
        warn!("flash_store: erase failed");
        return false;
    }

    match flash.blocking_write(SETTINGS_OFFSET, &buf[..write_size]) {
        Ok(_) => {
            info!("flash_store: saved settings ({} bytes)", len);
            true
        }
        Err(_) => {
            warn!("flash_store: write failed");
            false
        }
    }
}
