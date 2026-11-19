//! Small shared helpers used by multiple info-gathering modules.

use std::ffi::c_char;

/// Remove NUL bytes and other C0 control characters (except tab) from text
/// read from sysfs/DMI/firmware files, so embedded NUL padding never leaks
/// into terminal output.
fn strip_control_chars(s: &str) -> String {
    s.chars()
        .filter(|c| {
            let n = *c as u32;
            n == 0x09 || n > 0x1F // keep tab; drop NUL and C0 controls
        })
        .collect()
}

/// Strip a trailing newline/cr and surrounding quotes from a string.
pub fn strip_quotes(s: &str) -> String {
    let s = s.trim_end_matches(['\n', '\r']);
    let s = s.trim_start_matches(['\'', '"']);
    let s = s.trim_end_matches(['\'', '"']).to_string();
    strip_control_chars(&s)
}

/// Read the first line of a file, trimming line endings.
pub fn read_first_line(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|t| {
        t.lines()
            .next()
            .map(|l| strip_control_chars(l.trim_end_matches(['\n', '\r'])))
    })
}

/// Read a NUL-terminated C string from a fixed-size `i8` buffer, lossily
/// converting to UTF-8 so system strings are never silently dropped.
pub fn cstr_from_buf<const N: usize>(buf: &[c_char; N]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(N);
    let bytes: Vec<u8> = buf[..len].iter().copied().map(|b| b as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Read a NUL-terminated C string from a raw pointer, lossily converting to
/// UTF-8 so system strings are never silently dropped.
pub unsafe fn cstr_from_ptr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}

/// Read the PPID of the current process (Linux /proc).
#[cfg(target_os = "linux")]
pub fn read_ppid() -> Option<u32> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if line.starts_with("PPid:") {
            return line["PPid:".len()..].trim().parse().ok();
        }
    }
    None
}
#[cfg(not(target_os = "linux"))]
pub fn read_ppid() -> Option<u32> {
    None
}

/// Read the PPID of an arbitrary pid (Linux /proc).
#[cfg(target_os = "linux")]
pub fn read_ppid_for(pid: u32) -> Option<u32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in text.lines() {
        if line.starts_with("PPid:") {
            return line["PPid:".len()..].trim().parse().ok();
        }
    }
    None
}
#[cfg(not(target_os = "linux"))]
pub fn read_ppid_for(_pid: u32) -> Option<u32> {
    None
}

/// Read the `comm` of a process's parent and check it against a shell name set.
pub fn parent_comm(is_shell_name: impl Fn(&str) -> bool) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let ppid = read_ppid()?;
        let path = format!("/proc/{ppid}/comm");
        read_first_line(&path).filter(|s| is_shell_name(s))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = read_ppid();
        let _ = is_shell_name;
        None
    }
}
