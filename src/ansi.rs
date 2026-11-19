//! UTF-8 and ANSI escape helpers, ported from fetch.c.

/// Returns byte length of a UTF-8 sequence from its leading byte.
pub fn utf8_char_len(c: u8) -> usize {
    match c {
        c if c < 0x80 => 1,
        c if (c & 0xE0) == 0xC0 => 2,
        c if (c & 0xF0) == 0xE0 => 3,
        c if (c & 0xF8) == 0xF0 => 4,
        _ => 1, // invalid, treat as 1
    }
}

/// Skip past an ANSI escape sequence (ESC [ ... letter).
/// Returns number of bytes to skip, or 0 if not an escape.
pub fn skip_ansi(p: &[u8]) -> usize {
    if p.len() < 2 || p[0] != 0x1B || p[1] != b'[' {
        return 0;
    }
    let mut i = 2;
    // CSI private-mode prefix (e.g. '?' in ESC[?25h, ESC[?1002h)
    if i < p.len() && matches!(p[i], b'?' | b'<' | b'=' | b'>') {
        i += 1;
    }
    while i < p.len() && ((p[i] as char).is_ascii_digit() || p[i] == b';') {
        i += 1;
    }
    if i < p.len() {
        i += 1; // skip the final letter
    }
    i
}

/// Visible columns of a string, ignoring ANSI escapes (codepoint = 1 column).
pub fn visible_width(s: &str) -> usize {
    let mut w = 0;
    let mut bytes = s.as_bytes();
    while !bytes.is_empty() {
        let a = skip_ansi(bytes);
        if a > 0 {
            bytes = &bytes[a..];
            continue;
        }
        let len = utf8_char_len(bytes[0]).min(bytes.len());
        bytes = &bytes[len..];
        w += 1;
    }
    w
}

/// Copy s into buf clipped to max_cols visible columns (ANSI passes through,
/// max_cols < 0 = no limit). Appends a reset if the clip cut a color short.
/// Returns the number of bytes written.
pub fn emit_clipped(buf: &mut [u8], s: &str, max_cols: i32) -> usize {
    // first pass: measure visible width to know if we need to clip
    let total_w = visible_width(s);
    let need_clip = max_cols >= 0 && (total_w as i32) > max_cols;
    let mut limit: i32 = max_cols;
    if need_clip && max_cols >= 6 {
        limit = max_cols - 3; // leave room for "..."
    }

    let s_bytes = s.as_bytes();
    let mut w = 0;
    let mut p = 0;
    let mut had_ansi = false;
    let end = buf.len();

    let mut i = 0;
    while i < s_bytes.len() && p + 8 < end {
        let a = skip_ansi(&s_bytes[i..]);
        if a > 0 {
            if p + a + 8 >= end {
                break;
            }
            buf[p..p + a].copy_from_slice(&s_bytes[i..i + a]);
            p += a;
            i += a;
            had_ansi = true;
            continue;
        }
        if limit >= 0 && w >= limit as usize {
            break;
        }
        let len = utf8_char_len(s_bytes[i]).min(s_bytes.len() - i);
        let actual = len; // all bytes are valid per utf8_char_len on leading byte
        buf[p..p + actual].copy_from_slice(&s_bytes[i..i + actual]);
        p += actual;
        i += actual;
        w += 1;
    }

    if need_clip && max_cols >= 6 && p + 3 < end {
        buf[p..p + 3].copy_from_slice(b"...");
        p += 3;
    }
    if had_ansi && need_clip && p + 4 < end {
        buf[p..p + 4].copy_from_slice(b"\x1b[0m");
        p += 4;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_width() {
        assert_eq!(visible_width("hello"), 5);
    }

    #[test]
    fn ansi_is_invisible() {
        assert_eq!(visible_width("\u{1b}[1;31mhello\u{1b}[0m"), 5);
    }

    #[test]
    fn utf8_width() {
        assert_eq!(visible_width("▓"), 1);
        assert_eq!(visible_width("a▓b"), 3);
    }

    #[test]
    fn clip() {
        let mut buf = [0u8; 64];
        let n = emit_clipped(&mut buf, "abcdefgh", 5);
        assert_eq!(&buf[..n], b"abcde");
    }
}
