//! Shading ramps and sub-cell glyphs, ported from fetch.c.

use crate::ansi::utf8_char_len;

pub const RAMP_ASCII: &str = ".,-~:;=!*#$@";
pub const RAMP_BLOCKS: &str = "░▒▓█";

// Quadrants by coverage mask, bit 0 top-left through bit 3 bottom-right
pub const QUADRANT_GLYPHS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

// Sextants run U+1FB00..U+1FB3B in mask order (bit 0 top-left through bit 5
// bottom-right), skipping the two masks Unicode already had as half blocks.
// The UTF-8 bytes are built once and leaked as &'static str.
pub fn sextant_glyph(mask: u32) -> Option<&'static str> {
    if mask == 0 || mask >= 63 {
        return None;
    }
    if mask == 21 {
        return Some("▌");
    }
    if mask == 42 {
        return Some("▐");
    }
    static GLYPHS: std::sync::OnceLock<[Option<&'static str>; 64]> = std::sync::OnceLock::new();
    let table = GLYPHS.get_or_init(|| {
        let mut t = [None; 64];
        for m in 1..63 {
            if m == 21 || m == 42 {
                continue;
            }
            let cp: u32 = 0x1FB00 + m - 1 - (m > 21) as u32 - (m > 42) as u32;
            let ch = char::from_u32(cp).expect("valid sextant codepoint");
            let leaked: &'static str = std::boxed::Box::leak(ch.to_string().into_boxed_str());
            t[m as usize] = Some(leaked);
        }
        t
    });
    table[mask as usize]
}

const MAX_SHADING: usize = 64;

pub struct Shading {
    pub chars: Vec<String>,
}

impl Shading {
    pub fn new() -> Self {
        Self {
            chars: vec![".".to_string()],
        }
    }

    pub fn parse(&mut self, str: &str) {
        self.chars.clear();
        let mut p = str.as_bytes();
        while !p.is_empty() && self.chars.len() < MAX_SHADING {
            let len = utf8_char_len(p[0]).min(p.len());
            let actual = len;
            self.chars
                .push(std::str::from_utf8(&p[..actual]).unwrap_or(".").to_string());
            p = &p[actual..];
        }
        if self.chars.is_empty() {
            self.chars.push(".".to_string());
        }
    }

    pub fn ramp(&self, index: usize) -> &str {
        if self.chars.is_empty() {
            "."
        } else {
            &self.chars[index.min(self.chars.len() - 1)]
        }
    }
}

/// `char_weight` for UTF-8 codepoints — how much ink a glyph carries.
pub fn char_weight_utf8(ch: &str) -> f32 {
    let bytes = ch.as_bytes();
    // Single-byte ASCII
    if bytes[0] < 0x80 {
        return match bytes[0] {
            b'M' => 1.00,
            b'N' => 0.88,
            b'm' => 0.76,
            b'd' => 0.66,
            b'h' => 0.56,
            b'b' => 0.56,
            b'y' => 0.46,
            b'o' => 0.38,
            b'n' => 0.38,
            b's' => 0.30,
            b'+' => 0.22,
            b':' => 0.18,
            b'=' => 0.22,
            b'-' => 0.14,
            b'`' => 0.08,
            b'.' => 0.10,
            b'/' => 0.12,
            b'\'' => 0.06,
            b' ' => 0.0,
            c if (b'A'..=b'Z').contains(&c) => 0.80,
            c if (b'a'..=b'z').contains(&c) => 0.50,
            c if (b'0'..=b'9').contains(&c) => 0.40,
            _ => 0.15,
        };
    }

    // Multi-byte UTF-8: compare raw bytes for common block elements
    if bytes == b"\xe2\x96\x88" {
        // Full block U+2588
        return 1.00;
    }
    if bytes == b"\xe2\x96\x93" {
        // Dark shade U+2593
        return 0.75;
    }
    if bytes == b"\xe2\x96\x92" {
        // Medium shade U+2592
        return 0.50;
    }
    if bytes == b"\xe2\x96\x91" {
        // Light shade U+2591
        return 0.25;
    }

    // Half blocks (U+2580-258F)
    if bytes == b"\xe2\x96\x80"
        || bytes == b"\xe2\x96\x84"
        || bytes == b"\xe2\x96\x8c"
        || bytes == b"\xe2\x96\x90"
    {
        return 0.50;
    }

    // 3/4 blocks
    if bytes == b"\xe2\x96\x9b"
        || bytes == b"\xe2\x96\x9c"
        || bytes == b"\xe2\x96\x99"
        || bytes == b"\xe2\x96\x9f"
    {
        return 0.75;
    }

    // 1/4 blocks
    if bytes == b"\xe2\x96\x96"
        || bytes == b"\xe2\x96\x97"
        || bytes == b"\xe2\x96\x98"
        || bytes == b"\xe2\x96\x9d"
    {
        return 0.25;
    }

    // Box drawing chars (U+2500-257F): E2 94 xx / E2 95 xx
    if bytes.len() >= 2 && bytes[0] == 0xE2 && (bytes[1] == 0x94 || bytes[1] == 0x95) {
        return 0.20;
    }

    // Braille (U+2800-28FF): E2 A0-A3 xx
    if bytes.len() >= 3 && bytes[0] == 0xE2 && bytes[1] >= 0xA0 && bytes[1] <= 0xA3 {
        // Weight by number of dots (popcount of last byte)
        let b = bytes[2];
        let dots = b.count_ones() as f32;
        return dots / 8.0;
    }

    // Default for unknown multi-byte: treat as medium fill
    0.30
}

pub fn select_shading(
    mode: Option<&str>,
    chars: Option<&str>,
    sub: &mut (usize, usize),
    shading: &mut Shading,
) -> bool {
    let mode = mode.unwrap_or("ascii");
    match mode {
        "sextants" => {
            sub.0 = 3;
            sub.1 = 2;
            // build_sextant_glyphs() is implicit via sextant_glyph()
        }
        "blocks" => {
            sub.0 = 2;
            sub.1 = 2;
        }
        "ascii" => {}
        _ => return false,
    }
    if let Some(chars) = chars {
        shading.parse(chars);
    } else {
        let default = if mode == "ascii" {
            RAMP_ASCII
        } else {
            RAMP_BLOCKS
        };
        shading.parse(default);
    }
    true
}
