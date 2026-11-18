//! Small shared helpers used by multiple info-gathering modules.

/// Strip a trailing newline/cr and surrounding quotes from a string.
pub fn strip_quotes(s: &str) -> String {
    let s = s.trim_end_matches(['\n', '\r']);
    let s = s.trim_start_matches(['\'', '"']);
    s.trim_end_matches(['\'', '"']).to_string()
}

/// Read the first line of a file, trimming line endings.
pub fn read_first_line(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|t| {
        t.lines()
            .next()
            .map(|l| l.trim_end_matches(['\n', '\r']).to_string())
    })
}

/// Read a NUL-terminated C string from a fixed-size `i8` buffer, lossily
/// converting to UTF-8 so system strings are never silently dropped.
pub fn cstr_from_buf<const N: usize>(buf: &[i8; N]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(N);
    let bytes: Vec<u8> = buf[..len].iter().copied().map(|b| b as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Read a NUL-terminated C string from a raw pointer, lossily converting to
/// UTF-8 so system strings are never silently dropped.
pub unsafe fn cstr_from_ptr(ptr: *const i8) -> String {
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
