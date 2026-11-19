//! Logo loading and codepoint-cell parsing, ported from fetch.c.

use std::path::Path;

use crate::ansi::utf8_char_len;

const MAX_LOGO_ROWS: usize = 64;

/// One parsed logo cell: the codepoint text and the ANSI fg color in effect.
pub type Cell = (String, i32);

pub struct Logo {
    pub rows: Vec<String>,
    pub cells: Vec<Vec<Cell>>,
    pub cols: usize,
    pub has_ansi: bool,
    pub distro_hint: String,
}

impl Logo {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            cells: Vec::new(),
            cols: 0,
            has_ansi: false,
            distro_hint: String::new(),
        }
    }
}

// Process a logo row: split into codepoints, extracting ANSI colors.
fn process_logo_row(row: &str, cells: &mut Vec<Cell>, has_ansi: &mut bool) {
    cells.clear();
    let mut cur_color = 0;
    let bytes = row.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Parse ANSI escapes for color info
        if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            let mut num = 0i32;
            let mut has_num = false;
            while j < bytes.len() && ((bytes[j] as char).is_ascii_digit() || bytes[j] == b';') {
                if bytes[j] as char >= '0' && bytes[j] as char <= '9' {
                    num = num * 10 + (bytes[j] - b'0') as i32;
                    has_num = true;
                } else if bytes[j] == b';' {
                    if has_num
                        && ((num >= 30 && num <= 37) || num == 39 || (num >= 90 && num <= 97))
                    {
                        cur_color = num;
                    }
                    // bold flag — ignored; colors are always output bold
                    if has_num && num == 1 {
                        // nothing
                    }
                    if has_num && (num == 0 || num == 22) {
                        cur_color = 0;
                    }
                    num = 0;
                    has_num = false;
                }
                j += 1;
            }
            if has_num && ((num >= 30 && num <= 37) || num == 39 || (num >= 90 && num <= 97)) {
                cur_color = num;
            }
            if has_num && num == 0 {
                cur_color = 0;
            }
            if j < bytes.len() {
                j += 1; // skip the final letter
            }
            if cur_color > 0 {
                *has_ansi = true;
            }
            i = j;
            continue;
        }
        let len = utf8_char_len(bytes[i]).min(bytes.len() - i);
        let actual = len;
        let ch = std::str::from_utf8(&bytes[i..i + actual])
            .unwrap_or("?")
            .to_string();
        cells.push((ch, cur_color));
        i += actual;
    }
}

/// Process all loaded logo rows into per-cell codepoints.
pub fn process_logo(logo: &mut Logo) {
    logo.cols = 0;
    logo.cells.clear();
    for row in &logo.rows {
        let mut cells = Vec::new();
        process_logo_row(row, &mut cells, &mut logo.has_ansi);
        if cells.len() > logo.cols {
            logo.cols = cells.len();
        }
        logo.cells.push(cells);
    }
}

/// Try loading a logo from ~/.config/fetch/logo.txt.
pub fn load_logo_file(home: &str, logo: &mut Logo) -> bool {
    let path = Path::new(home).join(".config/fetch/logo.txt");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let mut first = true;
    for line in text.lines() {
        let mut line = line.to_string();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        if first && line.starts_with("# distro:") {
            let mut val = line[9..].to_string();
            while val.starts_with(' ') {
                val.remove(0);
            }
            logo.distro_hint = val;
            first = false;
            continue;
        }
        first = false;
        if line.is_empty() && logo.rows.is_empty() {
            continue;
        }
        logo.rows.push(line);
        if logo.rows.len() >= MAX_LOGO_ROWS {
            break;
        }
    }
    while logo.rows.last().map(|r| r.is_empty()).unwrap_or(false) {
        logo.rows.pop();
    }
    !logo.rows.is_empty()
}

/// Check if an ANSI escape is a cursor movement (not a color/SGR escape).
fn is_cursor_escape(p: &[u8]) -> bool {
    if p.len() < 2 || p[0] != 0x1B || p[1] != b'[' {
        return false;
    }
    let mut i = 2;
    while i < p.len() && ((p[i] as char).is_ascii_digit() || p[i] == b';') {
        i += 1;
    }
    i < p.len() && p[i] != b'm'
}

/// Try loading a logo from fastfetch colored output.
pub fn load_logo_ff_colored(name: &str, logo: &mut Logo) -> bool {
    let output = match std::process::Command::new("fastfetch")
        .args(["-c", "none", "-l", name, "-s", "break", "--pipe", "false"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let mut line = line.to_string();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }

        // Find last SGR escape end before any cursor movement (marks end of logo content)
        let bytes = line.as_bytes();
        let mut truncated = false;
        let mut last_sgr_end: Option<usize> = None;
        let mut i = 0;
        while i + 2 < bytes.len() {
            if is_cursor_escape(&bytes[i..]) {
                let cut = last_sgr_end.unwrap_or(i);
                line.truncate(cut);
                truncated = true;
                break;
            }
            if bytes[i] == 0x1B && bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < bytes.len() && ((bytes[j] as char).is_ascii_digit() || bytes[j] == b';') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'm' {
                    last_sgr_end = Some(j + 1);
                    i = j;
                }
            }
            i += 1;
        }

        if line.is_empty() && logo.rows.is_empty() {
            continue;
        }
        if line.is_empty() && truncated {
            break;
        }

        logo.rows.push(line);
        if logo.rows.len() >= MAX_LOGO_ROWS {
            break;
        }
    }
    while logo.rows.last().map(|r| r.is_empty()).unwrap_or(false) {
        logo.rows.pop();
    }
    !logo.rows.is_empty()
}

/// Fallback: load from --print-logos (no colors, but works on older fastfetch)
pub fn load_logo_ff_plain(name: &str, logo: &mut Logo) -> bool {
    let output = match std::process::Command::new("fastfetch")
        .args(["-c", "none", "--print-logos"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let name_len = name.len();
    let mut found = false;
    for line in text.lines() {
        let mut line = line.to_string();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        let len = line.len();

        if !found {
            if len > 0 && len <= name_len + 1 && line.ends_with(':') {
                let mut header = line.clone();
                header.pop(); // remove ':'
                if header.eq_ignore_ascii_case(name) {
                    found = true;
                }
            }
            continue;
        }

        // Detect next logo header
        if len > 1 && len < 40 && line.ends_with(':') && !logo.rows.is_empty() {
            let is_header = line.bytes().all(|b| b != 0x1B)
                && ((line.as_bytes()[0] as char).is_ascii_alphabetic());
            if is_header {
                break;
            }
        }

        if logo.rows.len() >= MAX_LOGO_ROWS {
            break;
        }
        logo.rows.push(line);
    }
    while logo.rows.last().map(|r| r.is_empty()).unwrap_or(false) {
        logo.rows.pop();
    }
    !logo.rows.is_empty()
}

pub fn load_logo_fastfetch(name: &str, logo: &mut Logo) -> bool {
    load_logo_ff_colored(name, logo) || load_logo_ff_plain(name, logo)
}

/// Built-in default logo (Gentoo).
pub fn load_default_logo(logo: &mut Logo) {
    let gentoo: &[&str] = &[
        "         -/oyddmdhs+:.            ",
        "     -odNMMMMMMMMNNmhy+-`         ",
        "   -yNMMMMMMMMMMMNNNmmdhy+-       ",
        " `omMMMMMMMMMMMMNmdmmmmddhhy/`    ",
        " omMMMMMMMMMMMNhhyyyohmdddhhhdo`  ",
        ".ydMMMMMMMMMMdhs++so/smdddhhhhdm+`",
        " oyhdmNMMMMMMMNdyooydMddddhhhhyhNd.",
        "  :oyhhdNNMMMMMMMNNMMMdddhhhhhyymMh",
        "    .:+sydNMMMMMNNMMMMdddhhhhhhmMmy",
        "       /mMMMMMMNNNMMMdddhhhhhmMNhs:",
        "    `oNMMMMMMMNNNMMMddddhhdmMNhs+` ",
        "  `sNMMMMMMMMNNNMMMdddddmNMmhs/.   ",
        " /NMMMMMMMMNNNNMMMdddmNMNdso:`     ",
        "+MMMMMMMNNNNNMMMMdMNMNdso/-        ",
        "yMMNNNNNNNMMMMMNNMmhs+/-`          ",
        "/hMMNNNNNNNNMNdhs++/-`             ",
        "`/ohdmmddhys+++/:.`                ",
        "  `-//////:--.                     ",
    ];
    logo.rows = gentoo.iter().map(|s| s.to_string()).collect();
    logo.cols = 0;
}

#[cfg(test)]
mod tests {

    #[test]
    fn char_weight_space_is_zero() {
        assert_eq!(crate::shading::char_weight_utf8(" "), 0.0);
        assert_eq!(crate::shading::char_weight_utf8("M"), 1.0);
        assert_eq!(crate::shading::char_weight_utf8("\u{2588}"), 1.0);
    }
}
