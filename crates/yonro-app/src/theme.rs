//! Named colors for the page. Hex lives only in this module.

// `gpui::rgb` reads 0xRRGGBB and drops a leading byte. Keep these 24-bit.

/// Desk around the sheet.
pub const DESK: u32 = 0x1c_1816;
/// Warm paper.
pub const PAPER: u32 = 0x2c_261f;
/// Ink.
pub const INK: u32 = 0xf3_e6d0;
/// Quiet hint.
pub const QUIET: u32 = 0xa8_9880;
/// Selection wash.
pub const WASH: u32 = 0x5c_4a32;
/// Highlight behind a marked span.
pub const MARK: u32 = 0x6b_5338;
/// A second ink for colored text.
pub const ACCENT: u32 = 0xe0_b070;
