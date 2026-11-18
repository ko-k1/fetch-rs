//! System info gatherers, ported from fetch.c.
//! Linux implementations use /proc, /sys and libc; other platforms get
//! best-effort or stubbed versions so the code compiles everywhere.

use crate::util::{cstr_from_buf, cstr_from_ptr, read_first_line, read_ppid_for, strip_quotes};

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

fn count_subdirs(path: &str) -> i32 {
    match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|s| !s.starts_with('.'))
                    .unwrap_or(false)
            })
            .count() as i32,
        Err(_) => 0,
    }
}

fn count_file_lines(path: &str, prefix: Option<&str>) -> i32 {
    match std::fs::read_to_string(path) {
        Ok(t) => t
            .lines()
            .filter(|l| prefix.is_none() || l.as_bytes().starts_with(prefix.unwrap().as_bytes()))
            .count() as i32,
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// uname
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn uname_fields() -> Option<(String, String, String)> {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } == 0 {
        Some((
            cstr_from_buf(&u.sysname),
            cstr_from_buf(&u.release),
            cstr_from_buf(&u.machine),
        ))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn uname_fields() -> Option<(String, String, String)> {
    None
}

// ---------------------------------------------------------------------------
// Run `<bin> --version` and extract the first version-looking token
// ---------------------------------------------------------------------------

pub fn get_cmd_version(bin: &str) -> Option<String> {
    if bin.is_empty()
        || !bin.bytes().all(|c| {
            (c as char).is_ascii_alphanumeric() || c == b'.' || c == b'/' || c == b'_' || c == b'-'
        })
    {
        return None;
    }
    let cmd = format!("{bin} --version 2>/dev/null");
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut buf = text.lines().next()?;
    let start = buf.bytes().position(|b| b.is_ascii_digit())?;
    buf = &buf[start..];
    let len = buf
        .bytes()
        .position(|b| b == b' ' || b == b'(' || b == b'\n')
        .unwrap_or(buf.len());
    let ver = &buf[..len.min(30)];
    if ver.is_empty() {
        None
    } else {
        Some(ver.to_string())
    }
}

// ---------------------------------------------------------------------------
// OS
// ---------------------------------------------------------------------------

pub fn gather_os() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0i8; 64];
        let mut len: libc::size_t = buf.len();
        let ok = unsafe {
            libc::sysctlbyname(
                b"kern.osproductversion\0".as_ptr(),
                buf.as_mut_ptr() as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        if ok {
            let version = cstr_from_buf(&buf);
            if let Some((sysname, _release, machine)) = uname_fields() {
                if sysname.eq_ignore_ascii_case("darwin") {
                    return Some(format!("macOS {version} {machine}"));
                }
            }
        }
        if let Some((sysname, _release, machine)) = uname_fields() {
            if sysname.eq_ignore_ascii_case("darwin") {
                return Some(format!("macOS {machine}"));
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut pretty = String::new();
        if let Ok(t) = std::fs::read_to_string("/etc/os-release") {
            for line in t.lines() {
                if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                    pretty = strip_quotes(rest);
                    break;
                }
            }
        }
        if pretty.is_empty() {
            pretty = "Linux".to_string();
        }
        if let Some((_sysname, _release, machine)) = uname_fields() {
            return Some(format!("{pretty} {machine}"));
        }
        Some(pretty)
    }
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

pub fn gather_host() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0i8; 128];
        let mut len: libc::size_t = buf.len();
        let ok = unsafe {
            libc::sysctlbyname(
                b"hw.model\0".as_ptr(),
                buf.as_mut_ptr() as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        if ok {
            let model = cstr_from_buf(&buf);
            if !model.is_empty() {
                return Some(model);
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut model = String::new();
        if let Some(m) = read_first_line("/proc/device-tree/model") {
            model = m;
        } else if let Some(m) = read_first_line("/sys/class/dmi/id/product_name") {
            model = m;
            if let Some(v) = read_first_line("/sys/class/dmi/id/sys_vendor")
                && !v.is_empty()
                && !model.to_lowercase().starts_with(&v.to_lowercase())
            {
                model = format!("{v} {model}");
            }
        }
        if model.is_empty() { None } else { Some(model) }
    }
}

// ---------------------------------------------------------------------------
// Kernel
// ---------------------------------------------------------------------------

pub fn gather_kernel() -> Option<String> {
    uname_fields().map(|(sysname, release, _machine)| format!("{sysname} {release}"))
}

// ---------------------------------------------------------------------------
// Uptime
// ---------------------------------------------------------------------------

pub fn gather_uptime() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let mut bt: libc::timeval = unsafe { std::mem::zeroed() };
        let mut len: libc::size_t = std::mem::size_of::<libc::timeval>();
        if unsafe {
            libc::sysctlbyname(
                b"kern.boottime\0".as_ptr(),
                &mut bt as *mut _ as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        let secs = unsafe { libc::time(std::ptr::null_mut()) } - bt.tv_sec;
        Some(format_uptime(secs))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string("/proc/uptime").ok()?;
        let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
        Some(format_uptime(secs as i64))
    }
}

fn format_uptime(total: i64) -> String {
    let days = total / 86400;
    let hours = (total % 86400) / 3600;
    let mins = (total % 3600) / 60;
    if days > 0 {
        format!(
            "{} day{}, {} hour{}, {} min{}",
            days,
            plural(days),
            hours,
            plural(hours),
            mins,
            plural(mins)
        )
    } else if hours > 0 {
        format!(
            "{} hour{}, {} min{}",
            hours,
            plural(hours),
            mins,
            plural(mins)
        )
    } else {
        format!("{} min{}", mins, plural(mins))
    }
}

fn plural(n: i64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ---------------------------------------------------------------------------
// Packages
// ---------------------------------------------------------------------------

pub fn gather_packages() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let cellars = ["/opt/homebrew/Cellar", "/usr/local/Cellar"];
        let mut total = 0;
        for cellar in cellars {
            total += count_subdirs(cellar);
        }
        if total > 0 {
            return Some(format!("{total} (brew)"));
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut val = String::new();

        // emerge (Gentoo)
        let n = count_emerge_pkgs();
        if n > 0 {
            val = format!("{n} (emerge)");
        }
        // pacman (Arch)
        if val.is_empty() {
            let n = count_subdirs("/var/lib/pacman/local");
            if n > 1 {
                // -1 for ALPM_DB_VERSION
                val = format!("{} (pacman)", n - 1);
            }
        }
        // dpkg (Debian/Ubuntu)
        if val.is_empty() {
            let n = count_file_lines("/var/lib/dpkg/status", Some("Package:"));
            if n > 0 {
                val = format!("{n} (dpkg)");
            }
        }
        // rpm (Fedora, RHEL, openSUSE, ...)
        if val.is_empty()
            && let Ok(out) = std::process::Command::new("sh")
                .arg("-c")
                .arg("rpm -qa 2>/dev/null")
                .output()
        {
            let n = String::from_utf8_lossy(&out.stdout).lines().count() as i32;
            if n > 0 {
                val = format!("{n} (rpm)");
            }
        }
        // xbps (Void)
        if val.is_empty()
            && let Ok(out) = std::process::Command::new("sh")
                .arg("-c")
                .arg("xbps-query -l 2>/dev/null | wc -l")
                .output()
        {
            let n: i32 = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            if n > 0 {
                val = format!("{n} (xbps)");
            }
        }
        // apk (Alpine)
        if val.is_empty() {
            let n = count_file_lines("/lib/apk/db/installed", Some("P:"));
            if n > 0 {
                val = format!("{n} (apk)");
            }
        }

        // Nix
        {
            let home = std::env::var("HOME").unwrap_or_default();
            let base = home.rsplit('/').next().unwrap_or(&home);
            let base_valid = !base.is_empty()
                && base.bytes().all(|c| {
                    (c as char).is_ascii_alphanumeric() || c == b'.' || c == b'_' || c == b'-'
                });
            if base_valid {
                let cmd = format!(
                    "sh -c 'sys=$(nix-store -q --references /run/current-system/sw 2>/dev/null | wc -l); resolved=$(readlink -f /etc/profiles/per-user/{base} 2>/dev/null); if [ -n \"$resolved\" ]; then hop1=$(nix-store -q --references \"$resolved\" 2>/dev/null); if [ -n \"$hop1\" ]; then usr=$(nix-store -q --references \"$hop1\" 2>/dev/null | wc -l); else usr=$(nix-store -q --references \"$resolved\" 2>/dev/null | wc -l); fi; else usr=0; fi; echo \"$sys $usr\"'"
                );
                if let Ok(out) = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let mut parts = text.split_whitespace();
                    let nix_system: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let nix_user: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let nix_val = if nix_system > 0 && nix_user > 0 {
                        format!("{nix_system} (system), {nix_user} (user)")
                    } else if nix_system > 0 {
                        format!("{nix_system} (system)")
                    } else if nix_user > 0 {
                        format!("{nix_user} (user)")
                    } else {
                        String::new()
                    };
                    if !nix_val.is_empty() {
                        if !val.is_empty() {
                            val.push_str(", ");
                        }
                        val.push_str(&nix_val);
                    }
                }
            }
        }

        // flatpak
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg("flatpak list --columns=ref 2>/dev/null")
            .output()
        {
            let n = String::from_utf8_lossy(&out.stdout).lines().count() as i32;
            if n > 0 {
                let flatpak_val = format!("{n} (flatpak)");
                if !val.is_empty() {
                    val.push_str(", ");
                }
                val.push_str(&flatpak_val);
            }
        }

        if val.is_empty() { None } else { Some(val) }
    }
}

#[cfg(target_os = "linux")]
fn count_emerge_pkgs() -> i32 {
    match std::fs::read_dir("/var/db/pkg") {
        Ok(entries) => {
            let mut count = 0;
            for cat in entries.filter_map(|e| e.ok()) {
                let name = match cat.file_name().to_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                if name.starts_with('.') {
                    continue;
                }
                let path = format!("/var/db/pkg/{name}");
                count += count_subdirs(&path);
            }
            count
        }
        Err(_) => 0,
    }
}

#[cfg(not(target_os = "linux"))]
fn count_emerge_pkgs() -> i32 {
    0
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

fn is_shell_name(s: &str) -> bool {
    matches!(
        s,
        "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "sh"
            | "nu"
            | "elvish"
            | "tcsh"
            | "csh"
            | "xonsh"
            | "ksh"
            | "ion"
    )
}

pub fn gather_shell() -> Option<(String, Option<String>)> {
    let shell = crate::util::parent_comm(is_shell_name)
        .or_else(|| std::env::var("SHELL").ok())?;
    let name = shell.rsplit('/').next().unwrap_or(&shell).to_string();
    let version = get_cmd_version(&shell);
    Some((name, version))
}

// ---------------------------------------------------------------------------
// Linux DRM display info
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod drm {
    const _IOC_READ: u32 = 0x80000000;
    const _IOC_WRITE: u32 = 0x40000000;

    const fn iowr(type_: u8, nr: u8, size: usize) -> u32 {
        (_IOC_READ | _IOC_WRITE) | ((type_ as u32) << 8) | (nr as u32) | ((size as u32) << 16)
    }

    pub const DRM_MIN_GETCONNECTOR: u32 = iowr(b'd', 0xA7, std::mem::size_of::<DrmConnector>());
    pub const DRM_MIN_GETENCODER: u32 = iowr(b'd', 0xA6, std::mem::size_of::<DrmEncoder>());
    pub const DRM_MIN_GETCRTC: u32 = iowr(b'd', 0xA1, std::mem::size_of::<DrmCrtc>());

    #[repr(C)]
    pub struct DrmModeInfo {
        pub clock: u32,
        pub hdisplay: u16,
        pub hsync_start: u16,
        pub hsync_end: u16,
        pub htotal: u16,
        pub hskew: u16,
        pub vdisplay: u16,
        pub vsync_start: u16,
        pub vsync_end: u16,
        pub vtotal: u16,
        pub vscan: u16,
        pub vrefresh: u32,
        pub flags: u32,
        pub type_: u32,
        pub name: [i8; 32],
    }

    #[repr(C)]
    pub struct DrmCrtc {
        pub set_connectors_ptr: u64,
        pub count_connectors: u32,
        pub crtc_id: u32,
        pub fb_id: u32,
        pub x: u32,
        pub y: u32,
        pub gamma_size: u32,
        pub mode_valid: u32,
        pub mode: DrmModeInfo,
    }

    #[repr(C)]
    pub struct DrmEncoder {
        pub encoder_id: u32,
        pub encoder_type: u32,
        pub crtc_id: u32,
        pub possible_crtcs: u32,
        pub possible_clones: u32,
    }

    #[repr(C)]
    pub struct DrmConnector {
        pub encoders_ptr: u64,
        pub modes_ptr: u64,
        pub props_ptr: u64,
        pub prop_values_ptr: u64,
        pub count_modes: u32,
        pub count_props: u32,
        pub count_encoders: u32,
        pub encoder_id: u32,
        pub connector_id: u32,
        pub connector_type: u32,
        pub connector_type_id: u32,
        pub connection: u32,
        pub mm_width: u32,
        pub mm_height: u32,
        pub subpixel: u32,
        pub pad: u32,
    }

    pub fn drm_active_mode(card: &str, conn_id: u32) -> Option<(i32, i32, f32, i32, i32)> {
        let dev = format!("/dev/dri/{card}");
        let fd = unsafe { libc::open(dev.as_ptr() as *const i8, libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let mut conn: DrmConnector = unsafe { std::mem::zeroed() };
        conn.connector_id = conn_id;
        let scratch: DrmModeInfo = unsafe { std::mem::zeroed() };
        conn.modes_ptr = &scratch as *const DrmModeInfo as u64;
        conn.count_modes = 1;
        let mut got: Option<(i32, i32, f32, i32, i32)> = None;
        if unsafe { libc::ioctl(fd, DRM_MIN_GETCONNECTOR.into(), &mut conn) } == 0 {
            let mut mm_w = 0i32;
            let mut mm_h = 0i32;
            if conn.mm_width != 0 && conn.mm_height != 0 {
                mm_w = conn.mm_width as i32;
                mm_h = conn.mm_height as i32;
            }
            if conn.encoder_id != 0 {
                let mut enc: DrmEncoder = unsafe { std::mem::zeroed() };
                enc.encoder_id = conn.encoder_id;
                if unsafe { libc::ioctl(fd, DRM_MIN_GETENCODER.into(), &mut enc) } == 0 && enc.crtc_id != 0
                {
                    let mut crtc: DrmCrtc = unsafe { std::mem::zeroed() };
                    crtc.crtc_id = enc.crtc_id;
                    if unsafe { libc::ioctl(fd, DRM_MIN_GETCRTC.into(), &mut crtc) } == 0
                        && crtc.mode_valid != 0
                        && crtc.mode.htotal != 0
                        && crtc.mode.vtotal != 0
                    {
                        let w = crtc.mode.hdisplay as i32;
                        let h = crtc.mode.vdisplay as i32;
                        let mut r = crtc.mode.clock as f32 * 1000.0
                            / ((crtc.mode.htotal as f32) * (crtc.mode.vtotal as f32));
                        if crtc.mode.flags & 0x10 != 0 {
                            r *= 2.0;
                        }
                        if crtc.mode.flags & 0x20 != 0 {
                            r /= 2.0;
                        }
                        if crtc.mode.vscan > 1 {
                            r /= crtc.mode.vscan as f32;
                        }
                        got = Some((w, h, r, mm_w, mm_h));
                    }
                }
            }
        }
        unsafe { libc::close(fd) };
        got
    }

    pub fn read_edid(conn_dir: &str) -> Option<Vec<u8>> {
        let path = format!("/sys/class/drm/{conn_dir}/edid");
        let data = std::fs::read(&path).ok()?;
        if data.len() < 128 {
            return None;
        }
        let hdr: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
        if data[..8] != hdr {
            return None;
        }
        Some(data)
    }

    pub fn edid_name(e: &[u8]) -> Option<String> {
        for i in (54..=108).step_by(18) {
            let d = &e[i..i + 18];
            if d[0] != 0 || d[1] != 0 || d[2] != 0 || d[3] != 0xFC {
                continue;
            }
            let mut n = String::new();
            for j in 5..18 {
                if d[j] == 0x0A {
                    break;
                }
                n.push(d[j] as char);
            }
            n = n.trim_end().to_string();
            if !n.is_empty() {
                return Some(n);
            }
        }
        let v = ((e[8] as u16) << 8) | e[9] as u16;
        let m = [
            b'A' + (((v >> 10) & 0x1F) - 1) as u8,
            b'A' + (((v >> 5) & 0x1F) - 1) as u8,
            b'A' + ((v & 0x1F) - 1) as u8,
        ];
        if m.iter().any(|&c| c < b'A' || c > b'Z') {
            return None;
        }
        Some(format!(
            "{}{:02X}{:02X}",
            std::str::from_utf8(&m).unwrap(),
            e[11],
            e[10]
        ))
    }

    pub fn edid_size_mm(e: &[u8]) -> Option<(i32, i32)> {
        let d = &e[54..];
        if d[0] != 0 || d[1] != 0 {
            let mw = (d[12] as i32) | ((d[14] as i32 & 0xF0) << 4);
            let mh = (d[13] as i32) | ((d[14] as i32 & 0x0F) << 8);
            if mw > 0 && mh > 0 {
                return Some((mw, mh));
            }
        }
        if e[21] != 0 && e[22] != 0 {
            return Some((e[21] as i32 * 10, e[22] as i32 * 10));
        }
        None
    }

    pub fn edid_refresh(e: &[u8]) -> f32 {
        let d = &e[54..];
        let clock = (d[0] as u32) | ((d[1] as u32) << 8);
        if clock == 0 {
            return 0.0;
        }
        let htotal = ((d[2] as u32) | ((d[4] as u32 & 0xF0) << 4))
            + ((d[3] as u32) | ((d[4] as u32 & 0x0F) << 8));
        let vtotal = ((d[5] as u32) | ((d[7] as u32 & 0xF0) << 4))
            + ((d[6] as u32) | ((d[7] as u32 & 0x0F) << 8));
        if htotal == 0 || vtotal == 0 {
            return 0.0;
        }
        clock as f32 * 10000.0 / (htotal as f32 * vtotal as f32)
    }

    pub fn format_hz(hz: f32) -> String {
        if hz <= 0.0 {
            return String::new();
        }
        if (hz - hz.round()).abs() < 0.05 {
            format!("{:.0} Hz", hz)
        } else {
            format!("{:.2} Hz", hz)
        }
    }

    pub fn connector_is_internal(name: &str) -> bool {
        name.starts_with("eDP") || name.starts_with("LVDS") || name.starts_with("DSI")
    }
}

#[cfg(not(target_os = "linux"))]
mod drm {
    pub fn drm_active_mode(_card: &str, _conn_id: u32) -> Option<(i32, i32, f32, i32, i32)> {
        None
    }
    pub fn read_edid(_conn_dir: &str) -> Option<Vec<u8>> {
        None
    }
    pub fn edid_name(_e: &[u8]) -> Option<String> {
        None
    }
    pub fn edid_size_mm(_e: &[u8]) -> Option<(i32, i32)> {
        None
    }
    pub fn edid_refresh(_e: &[u8]) -> f32 {
        0.0
    }
    pub fn format_hz(_hz: f32) -> String {
        String::new()
    }
    pub fn connector_is_internal(_name: &str) -> bool {
        false
    }
}

#[cfg(target_os = "macos")]
fn gather_display_macos() -> Option<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("system_profiler SPDisplaysDataType 2>/dev/null")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut name = String::new();
    let mut res = String::new();
    for line in text.lines() {
        if name.is_empty() {
            let prefix = if line.contains("Chipset Model:") {
                "Chipset Model:"
            } else if line.contains("Chip:") {
                "Chip:"
            } else {
                ""
            };
            if !prefix.is_empty() {
                let p = line.find(prefix).unwrap();
                let rest = &line[p + prefix.len()..];
                name = rest.trim_start().trim_end_matches(['\n', '\r']).to_string();
            }
        }
        if res.is_empty() {
            if let Some(p) = line.find("Resolution:") {
                let rest = &line[p + 11..];
                res = rest.trim_start().trim_end_matches(['\n', '\r']).to_string();
            }
        }
    }
    match (name.is_empty(), res.is_empty()) {
        (false, false) => Some(format!("{name} {res}")),
        (true, false) => Some(res),
        _ => None,
    }
}

pub fn gather_displays() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        let mut out = Vec::new();
        if let Some(v) = gather_display_macos() {
            out.push(("Display".to_string(), v));
        }
        return out;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut out = Vec::new();
        let dir = match std::fs::read_dir("/sys/class/drm") {
            Ok(d) => d,
            Err(_) => return out,
        };
        let mut emitted = 0;
        for ent in dir.filter_map(|e| e.ok()) {
            let name = match ent.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !name.starts_with("card") {
                continue;
            }
            let dash = match name[4..].find('-') {
                Some(d) => 4 + d,
                None => continue,
            };
            let status_path = format!("/sys/class/drm/{name}/status");
            let status = read_first_line(&status_path).unwrap_or_default();
            if status != "connected" {
                continue;
            }
            let mode_path = format!("/sys/class/drm/{name}/modes");
            let mode = read_first_line(&mode_path).unwrap_or_default();
            if mode.is_empty() {
                continue;
            }
            let conn = &name[dash + 1..];
            let card = &name[..dash];
            let conn_id_path = format!("/sys/class/drm/{name}/connector_id");
            let conn_id = read_first_line(&conn_id_path)
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let (w, h, hz, mut mm_w, mut mm_h) = if conn_id != 0 {
                drm::drm_active_mode(card, conn_id).unwrap_or((0, 0, 0.0, 0, 0))
            } else {
                (0, 0, 0.0, 0, 0)
            };
            let mut hz = hz;
            let mut have_edid = false;
            let mut edid = Vec::new();
            if let Some(data) = drm::read_edid(&name) {
                have_edid = true;
                edid = data;
                if hz <= 0.0 {
                    hz = drm::edid_refresh(&edid);
                }
                if mm_w == 0 || mm_h == 0 {
                    if let Some((mw, mh)) = drm::edid_size_mm(&edid) {
                        mm_w = mw;
                        mm_h = mh;
                    }
                }
            }
            let res = if w > 0 && h > 0 {
                format!("{w}x{h}")
            } else {
                mode.clone()
            };
            let mon_name = have_edid
                .then(|| drm::edid_name(&edid))
                .flatten()
                .unwrap_or_default();
            let label = format!(
                "Display ({})",
                if !mon_name.is_empty() {
                    mon_name
                } else {
                    conn.to_string()
                }
            );
            let mut inches = String::new();
            if mm_w > 0 && mm_h > 0 {
                let diag = ((mm_w.pow(2) + mm_h.pow(2)) as f32).sqrt() / 25.4;
                if diag > 0.0 {
                    inches = format!(" in {}\"", diag.round() as i32);
                }
            }
            let rate = drm::format_hz(hz);
            let sep = if rate.is_empty() {
                ""
            } else if !inches.is_empty() {
                ", "
            } else {
                " @ "
            };
            let kind = if drm::connector_is_internal(conn) {
                "Built-in"
            } else {
                "External"
            };
            out.push((label, format!("{res}{inches}{sep}{rate} [{kind}]")));
            emitted += 1;
        }
        if emitted == 0
            && let Ok(dir) = std::fs::read_dir("/sys/class/drm")
        {
            for ent in dir.filter_map(|e| e.ok()) {
                let name = match ent.file_name().to_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                if !name.starts_with("card") {
                    continue;
                }
                let mode_path = format!("/sys/class/drm/{name}/modes");
                let mode = read_first_line(&mode_path).unwrap_or_default();
                if !mode.is_empty() {
                    out.push(("Display".to_string(), mode));
                    break;
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// WM
// ---------------------------------------------------------------------------

fn scan_proc_comms(pred: impl Fn(&str) -> bool) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let dir = match std::fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return None,
        };
        for ent in dir.filter_map(|e| e.ok()) {
            let name = match ent.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !name
                .bytes()
                .next()
                .map_or(false, |b| b.is_ascii_digit() && b != b'0')
            {
                continue;
            }
            let path = format!("/proc/{name}/comm");
            if let Some(comm) = read_first_line(&path) {
                if pred(&comm) {
                    return Some(comm);
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pred;
        None
    }
}

pub fn gather_wm() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        return Some("Aqua".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let _session = std::env::var("XDG_SESSION_TYPE").ok();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();

        let known_wms = [
            "dwl", "sway", "river", "labwc", "weston", "i3", "bspwm", "openbox", "awesome", "dwm",
        ];

        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            return Some("Hyprland".to_string());
        }

        if let Some(wm) = scan_proc_comms(|comm| known_wms.contains(&comm)) {
            return Some(wm);
        }

        if let Some(desktop) = desktop.as_deref()
            && !desktop.is_empty()
        {
            let first = desktop.split(':').next().unwrap_or(desktop);
            let wm = match first {
                "KDE" => Some("KWin"),
                "GNOME" => Some("Mutter"),
                "XFCE" => Some("xfwm4"),
                "Cinnamon" => Some("Muffin"),
                "MATE" => Some("Marco"),
                "LXQt" => Some("Openbox"),
                "Budgie" => Some("Mutter"),
                "Deepin" => Some("KWin"),
                other => Some(other),
            };
            return wm.map(|w| w.to_string());
        }
        None
    }
}

pub fn wm_version_label(wm: &str, is_wayland: bool, version: Option<&str>) -> String {
    match version {
        Some(v) => format!("{wm} {v}{}", if is_wayland { " (Wayland)" } else { "" }),
        None => format!("{wm}{}", if is_wayland { " (Wayland)" } else { "" }),
    }
}

// ---------------------------------------------------------------------------
// Display manager
// ---------------------------------------------------------------------------

pub fn gather_displaymanager() -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    return None;

    #[cfg(target_os = "linux")]
    {
        let known_dms = [
            "sddm", "gdm", "gdm3", "lightdm", "lxdm", "greetd", "xdm", "slim", "lemurs", "ly",
        ];
        let labels: &[(&str, &str)] = &[
            ("sddm", "SDDM"),
            ("gdm", "GDM"),
            ("gdm3", "GDM"),
            ("lightdm", "LightDM"),
            ("lxdm", "LXDM"),
            ("greetd", "greetd"),
            ("xdm", "XDM"),
            ("slim", "SLiM"),
            ("lemurs", "lemurs"),
            ("ly", "ly"),
        ];
        let matched = scan_proc_comms(|comm| known_dms.iter().any(|&d| d == comm));
        matched.and_then(|id| {
            labels
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, l)| l.to_string())
        })
    }
}

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn sysctl_int_macos(name: &str) -> i32 {
    let mut val: i32 = 0;
    let mut len: libc::size_t = std::mem::size_of::<i32>();
    let cname = std::ffi::CString::new(name).unwrap();
    if unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut val as *mut _ as *mut _,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } == 0
    {
        val
    } else {
        0
    }
}

#[cfg(target_os = "macos")]
fn sysctl_long_macos(name: &str) -> i64 {
    let mut val: i64 = 0;
    let mut len: libc::size_t = std::mem::size_of::<i64>();
    let cname = std::ffi::CString::new(name).unwrap();
    if unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut val as *mut _ as *mut _,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } == 0
    {
        val
    } else {
        0
    }
}

pub fn gather_cpu() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0i8; 128];
        let mut len: libc::size_t = buf.len();
        if unsafe {
            libc::sysctlbyname(
                b"machdep.cpu.brand_string\0".as_ptr(),
                buf.as_mut_ptr() as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        let name = cstr_from_buf(&buf);
        if name.is_empty() {
            return None;
        }
        let cores = sysctl_int_macos("hw.ncpu");
        let freq = sysctl_long_macos("hw.cpufrequency_max");
        if freq > 0 {
            let ghz = freq as f32 / 1e9;
            Some(format!("{name} ({cores}) @ {ghz:.2} GHz"))
        } else if cores > 0 {
            Some(format!("{name} ({cores})"))
        } else {
            Some(name)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut name = String::new();
        let mut cores = 0i32;
        if let Ok(t) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in t.lines() {
                if name.is_empty()
                    && line.starts_with("model name")
                    && let Some((_k, val)) = line.split_once(':')
                {
                    let mut v = val.trim_start().trim_end_matches(['\n', '\r']).to_string();
                    if let Some(at) = v.find(" @ ") {
                        v.truncate(at);
                    }
                    name = v;
                }
                if line.starts_with("processor") {
                    cores += 1;
                }
            }
        }
        if name.is_empty()
            && let Some(model) = read_first_line("/proc/device-tree/model")
            && let Some(paren) = model.find('(')
        {
            let rest = &model[paren + 1..];
            let end = rest.find(',').or_else(|| rest.find(')'));
            if let Some(end) = end {
                name = format!("Apple {}", &rest[..end]);
            }
        }
        let mut max_ghz = 0.0f32;
        if let Ok(entries) = std::fs::read_dir("/sys/devices/system/cpu/cpufreq") {
            let mut max_khz = 0i64;
            for ent in entries.filter_map(|e| e.ok()) {
                let n = match ent.file_name().to_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                if !n.starts_with("policy") {
                    continue;
                }
                let path = format!("/sys/devices/system/cpu/cpufreq/{n}/cpuinfo_max_freq");
                if let Ok(text) = std::fs::read_to_string(&path)
                    && let Ok(khz) = text.trim().parse::<i64>()
                    && khz > max_khz
                {
                    max_khz = khz;
                }
            }
            if max_khz > 0 {
                max_ghz = max_khz as f32 / 1e6;
            }
        }
        if name.is_empty() {
            None
        } else if cores > 0 && max_ghz > 0.0 {
            Some(format!("{name} ({cores}) @ {max_ghz:.2} GHz"))
        } else if cores > 0 {
            Some(format!("{name} ({cores})"))
        } else {
            Some(name)
        }
    }
}

// ---------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------

fn str_ci_contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn amd_name_is_igpu(name: &str) -> bool {
    let codenames = [
        "Raphael",
        "Phoenix",
        "Phoenix2",
        "Hawk Point",
        "Renoir",
        "Cezanne",
        "Rembrandt",
        "Picasso",
        "Raven",
        "Raven2",
        "Van Gogh",
        "Mendocino",
        "Barcelo",
        "Strix Point",
        "Strix Halo",
        "Krackan Point",
    ];
    codenames.iter().any(|c| str_ci_contains(name, c))
}

fn gpu_lookup_lspci(pci_id: &str) -> Option<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("lspci -d {pci_id} 2>/dev/null"))
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?;
    let line = line.split(" (rev ").next().unwrap_or(line);

    let lb = line.rfind('[');
    let rb = line.rfind(']');
    let bracket_count = line.matches('[').count();
    let name = if bracket_count >= 2 {
        lb.and_then(|l| rb.filter(|&r| r > l).map(|r| line[l + 1..r].to_string()))
    } else {
        lb.and_then(|l| {
            rb.filter(|&r| r > l).and_then(|r| {
                let after = line[r + 1..].trim_start();
                if after.is_empty() {
                    None
                } else {
                    Some(after.to_string())
                }
            })
        })
    };
    let name = name.or_else(|| {
        let corp = line.find(" Corporation ").or_else(|| line.find(" Corp "));
        match corp {
            Some(p) => {
                let skip = if line[p..].starts_with(" Corporation ") {
                    13
                } else {
                    6
                };
                Some(line[p + skip..].trim_start().to_string())
            }
            None => {
                let c1 = line.find(':')?;
                let c2 = line[c1 + 1..].find(':')?;
                let start = c1 + 1 + c2;
                Some(line[start + 1..].trim_start().to_string())
            }
        }
    });
    name.filter(|n| !n.is_empty())
}

pub fn gather_gpus() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        let mut out = Vec::new();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("system_profiler SPDisplaysDataType 2>/dev/null")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let prefix = if line.contains("Chipset Model:") {
                "Chipset Model:"
            } else if line.contains("Chip:") {
                "Chip:"
            } else {
                ""
            };
            if !prefix.is_empty() {
                let p = line.find(prefix).unwrap();
                let rest = &line[p + prefix.len()..];
                let gpu = rest.trim_start().trim_end_matches(['\n', '\r']);
                if !gpu.is_empty() {
                    out.push(("GPU".to_string(), gpu.to_string()));
                    break;
                }
            }
        }
        out
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut out = Vec::new();
        let dir = match std::fs::read_dir("/sys/class/drm") {
            Ok(d) => d,
            Err(_) => return out,
        };
        for ent in dir.filter_map(|e| e.ok()) {
            let name = match ent.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            // Only cardN (all digits after "card")
            let rest = &name[4..];
            if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let uevent_path = format!("/sys/class/drm/{name}/device/uevent");
            let text = match std::fs::read_to_string(&uevent_path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let mut driver = String::new();
            let mut pci_id = String::new();
            let mut compat = String::new();
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("DRIVER=") {
                    driver = rest.trim_end_matches(['\n', '\r']).to_string();
                } else if let Some(rest) = line.strip_prefix("PCI_ID=") {
                    pci_id = rest.trim_end_matches(['\n', '\r']).to_string();
                } else if let Some(rest) = line.strip_prefix("OF_COMPATIBLE_0=") {
                    compat = rest.trim_end_matches(['\n', '\r']).to_string();
                }
            }
            if compat.contains("display-subsystem") {
                continue;
            }
            let mut name_out = String::new();
            let mut type_ = "";
            if compat.starts_with("apple,agx") {
                let cpu = read_first_line("/proc/device-tree/model").and_then(|model| {
                    model.find('(').and_then(|paren| {
                        let rest = &model[paren + 1..];
                        let end = rest.find(',').or_else(|| rest.find(')'));
                        end.map(|end| rest[..end].to_string())
                    })
                });
                name_out = cpu.map_or_else(|| "Apple GPU".to_string(), |c| format!("Apple {c}"));
                type_ = "Integrated";
            } else if !pci_id.is_empty() {
                if let Some(n) = gpu_lookup_lspci(&pci_id) {
                    name_out = n;
                } else if driver == "i915" || driver == "xe" {
                    name_out = "Intel Graphics".to_string();
                } else if driver == "amdgpu" || driver == "radeon" {
                    name_out = "AMD Graphics".to_string();
                } else if driver == "nvidia" || driver == "nouveau" {
                    name_out = "NVIDIA GPU".to_string();
                } else if !driver.is_empty() {
                    name_out = driver.clone();
                }
                if type_.is_empty() {
                    type_ = if driver == "i915" || driver == "xe" {
                        "Integrated"
                    } else if driver == "nvidia" || driver == "nouveau" {
                        "Discrete"
                    } else if driver == "amdgpu" {
                        if amd_name_is_igpu(&name_out) {
                            "Integrated"
                        } else {
                            "Discrete"
                        }
                    } else {
                        ""
                    };
                }
            } else if driver == "i915" || driver == "xe" {
                name_out = "Intel Graphics".to_string();
                type_ = "Integrated";
            } else if driver == "amdgpu" || driver == "radeon" {
                name_out = "AMD Graphics".to_string();
            } else if driver == "nvidia" || driver == "nouveau" {
                name_out = "NVIDIA GPU".to_string();
                type_ = "Discrete";
            } else if !driver.is_empty() {
                name_out = driver.clone();
            }
            if name_out.is_empty() {
                continue;
            }
            if type_.is_empty() {
                out.push(("GPU".to_string(), name_out));
            } else {
                out.push(("GPU".to_string(), format!("{name_out} [{type_}]")));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Memory / Swap
// ---------------------------------------------------------------------------

fn meminfo_value(text: &str, key: &str) -> Option<i64> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let n: i64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(n);
        }
    }
    None
}

pub fn gather_memory() -> Option<(String, i32)> {
    // (formatted value, pct)
    #[cfg(target_os = "macos")]
    {
        let total = sysctl_long_macos("hw.memsize");
        if total <= 0 {
            return None;
        }
        let mut free_pages: i64 = 0;
        let mut inactive_pages: i64 = 0;
        let mut speculative_pages: i64 = 0;
        let mut page_size: i64 = 4096;
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg("vm_stat 2>/dev/null")
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(n) = line.strip_prefix("Pages free:") {
                    free_pages = n.trim().trim_end_matches('.').parse().unwrap_or(0);
                } else if let Some(n) = line.strip_prefix("Pages inactive:") {
                    inactive_pages = n.trim().trim_end_matches('.').parse().unwrap_or(0);
                } else if let Some(n) = line.strip_prefix("Pages speculative:") {
                    speculative_pages = n.trim().trim_end_matches('.').parse().unwrap_or(0);
                } else if line.contains("page size of") {
                    if let Some(n) = line.split("page size of").nth(1) {
                        page_size = n.trim().trim_end_matches(')').parse().unwrap_or(4096);
                    }
                }
            }
        }
        let avail = (free_pages + inactive_pages + speculative_pages) * page_size;
        let used_gib = (total - avail) as f32 / 1073741824.0;
        let total_gib = total as f32 / 1073741824.0;
        let pct = ((total - avail) * 100 / total) as i32;
        let color = if pct >= 80 {
            "31"
        } else if pct >= 50 {
            "93"
        } else {
            "32"
        };
        Some((
            format!("{used_gib:.2} GiB / {total_gib:.2} GiB (\u{1b}[{color}m{pct}%\u{1b}[0m)"),
            pct,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let total = meminfo_value(&text, "MemTotal:")?;
        let avail = meminfo_value(&text, "MemAvailable:")?;
        if total <= 0 {
            return None;
        }
        let used_gib = (total - avail) as f32 / 1048576.0;
        let total_gib = total as f32 / 1048576.0;
        let pct = ((total - avail) * 100 / total) as i32;
        let color = if pct >= 80 {
            "31"
        } else if pct >= 50 {
            "93"
        } else {
            "32"
        };
        Some((
            format!("{used_gib:.2} GiB / {total_gib:.2} GiB (\u{1b}[{color}m{pct}%\u{1b}[0m)"),
            pct,
        ))
    }
}

pub fn gather_swap() -> Option<(String, i32)> {
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0i8; 256];
        let mut len: libc::size_t = buf.len();
        if unsafe {
            libc::sysctlbyname(
                b"vm.swapusage\0".as_ptr(),
                buf.as_mut_ptr() as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        let swapstr = cstr_from_buf(&buf);
        let mut total_mb = 0.0f32;
        let mut used_mb = 0.0f32;
        if let Some(start) = swapstr.find("total ") {
            let rest = &swapstr[start + "total ".len()..];
            let mut parts = rest.split_whitespace();
            total_mb = parts
                .next()
                .unwrap_or("0")
                .trim_end_matches('M')
                .parse()
                .unwrap_or(0.0);
        }
        if let Some(start) = swapstr.find("used ") {
            let rest = &swapstr[start + "used ".len()..];
            used_mb = rest
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .trim_end_matches('M')
                .parse()
                .unwrap_or(0.0);
        }
        if total_mb <= 0.0 {
            return None;
        }
        let pct = (used_mb * 100.0 / total_mb) as i32;
        let color = if pct >= 80 {
            "31"
        } else if pct >= 50 {
            "93"
        } else {
            "32"
        };
        Some((
            format!("{used_mb:.2} MiB / {total_mb:.2} MiB (\u{1b}[{color}m{pct}%\u{1b}[0m)"),
            pct,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let total = meminfo_value(&text, "SwapTotal:")?;
        let free_s = meminfo_value(&text, "SwapFree:").unwrap_or(0);
        if total <= 0 {
            return None;
        }
        let used = total - free_s;
        let pct = (used * 100 / total) as i32;
        let color = if pct >= 80 {
            "31"
        } else if pct >= 50 {
            "93"
        } else {
            "32"
        };
        let (used_v, total_v, unit) = if total >= 1048576 {
            (used as f32 / 1048576.0, total as f32 / 1048576.0, "GiB")
        } else {
            (used as f32 / 1024.0, total as f32 / 1024.0, "MiB")
        };
        Some((
            format!("{used_v:.2} {unit} / {total_v:.2} {unit} (\u{1b}[{color}m{pct}%\u{1b}[0m)"),
            pct,
        ))
    }
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

fn disk_fstype_linux(path: &str) -> String {
    if let Ok(t) = std::fs::read_to_string("/proc/mounts") {
        for line in t.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[1] == path {
                return parts[2].to_string();
            }
        }
    }
    String::new()
}

#[cfg(target_os = "macos")]
fn disk_fstype_macos(path: &str) -> String {
    unsafe {
        let mut mnts: *mut libc::statfs = std::ptr::null_mut();
        let count = libc::getmntinfo(&mut mnts, libc::MNT_NOWAIT);
        for i in 0..count {
            let mnt = &*mnts.add(i);
            let mnton = cstr_from_buf(&mnt.f_mntonname);
            if mnton == path {
                return cstr_from_buf(&mnt.f_fstypename);
            }
        }
    }
    String::new()
}

#[cfg(not(target_os = "macos"))]
fn disk_fstype_macos(_path: &str) -> String {
    String::new()
}

pub fn gather_disk(path: &str) -> Option<(String, String, i32)> {
    #[cfg(unix)]
    {
        gather_disk_unix(path)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(unix)]
fn gather_disk_unix(path: &str) -> Option<(String, String, i32)> {
    // (label, value, pct)
    let st = unsafe { statvfs(path) }?;
    if st.f_blocks == 0 {
        return None;
    }
    let total_gib = st.f_blocks as f32 * st.f_frsize as f32 / (1024.0 * 1024.0 * 1024.0);
    let free_gib = st.f_bfree as f32 * st.f_frsize as f32 / (1024.0 * 1024.0 * 1024.0);
    let used_gib = total_gib - free_gib;
    let pct = (used_gib * 100.0 / total_gib) as i32;
    let color = if pct >= 80 {
        "31"
    } else if pct >= 50 {
        "93"
    } else {
        "32"
    };
    let label = format!("Disk ({path})");
    let fstype = disk_fstype_macos(path);
    let fstype = if fstype.is_empty() {
        disk_fstype_linux(path)
    } else {
        fstype
    };
    let value = if !fstype.is_empty() {
        format!(
            "{used_gib:.2} GiB / {total_gib:.2} GiB (\u{1b}[{color}m{pct}%\u{1b}[0m) - {fstype}"
        )
    } else {
        format!("{used_gib:.2} GiB / {total_gib:.2} GiB (\u{1b}[{color}m{pct}%\u{1b}[0m)")
    };
    Some((label, value, pct))
}

#[cfg(unix)]
unsafe fn statvfs(path: &str) -> Option<libc::statvfs> {
    let cpath = std::ffi::CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(cpath.as_ptr(), &mut st) } == 0 {
        Some(st)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Battery
// ---------------------------------------------------------------------------

pub fn gather_battery() -> Option<(String, String)> {
    // (label, value)
    #[cfg(target_os = "macos")]
    {
        // IOKit binding is heavy; stubbed
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let ps_dir = match std::fs::read_dir("/sys/class/power_supply") {
            Ok(d) => d,
            Err(_) => return None,
        };
        let bat_name = ps_dir.filter_map(|e| e.ok()).find_map(|ent| {
            let name = ent.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let cap_path = format!("/sys/class/power_supply/{name}/capacity");
            if std::fs::File::open(&cap_path).is_ok() {
                Some(name)
            } else {
                None
            }
        });
        let bat_name = bat_name?;
        let mut energy_now = 0i64;
        let mut energy_full = 0i64;
        let en_path = format!("/sys/class/power_supply/{bat_name}/energy_now");
        if let Ok(text) = std::fs::read_to_string(&en_path) {
            energy_now = text.trim().parse().unwrap_or(0);
        }
        let ef_path = format!("/sys/class/power_supply/{bat_name}/energy_full");
        if let Ok(text) = std::fs::read_to_string(&ef_path) {
            energy_full = text.trim().parse().unwrap_or(0);
        }
        let mut capacity: i32 = if energy_full > 0 {
            (energy_now * 100 / energy_full) as i32
        } else {
            -1
        };
        if capacity < 0 {
            let mut charge_now = 0i64;
            let mut charge_full = 0i64;
            let cn_path = format!("/sys/class/power_supply/{bat_name}/charge_now");
            if let Ok(text) = std::fs::read_to_string(&cn_path) {
                charge_now = text.trim().parse().unwrap_or(0);
            }
            let cf_path = format!("/sys/class/power_supply/{bat_name}/charge_full");
            if let Ok(text) = std::fs::read_to_string(&cf_path) {
                charge_full = text.trim().parse().unwrap_or(0);
            }
            if charge_full > 0 {
                capacity = (charge_now * 100 / charge_full) as i32;
            }
        }
        if capacity < 0 {
            let cap_path = format!("/sys/class/power_supply/{bat_name}/capacity");
            if let Ok(text) = std::fs::read_to_string(&cap_path) {
                capacity = text.trim().parse().unwrap_or(-1);
            }
        }
        let status_path = format!("/sys/class/power_supply/{bat_name}/status");
        let status = read_first_line(&status_path).unwrap_or_default();
        let mut time_str = String::new();
        if energy_now > 0 {
            let pn_path = format!("/sys/class/power_supply/{bat_name}/power_now");
            let mut power_now = 0i64;
            if let Ok(text) = std::fs::read_to_string(&pn_path) {
                power_now = text.trim().parse().unwrap_or(0);
            }
            if power_now < 0 {
                power_now = -power_now;
            }
            if power_now > 0 {
                let mins_left = (energy_now as f32 / power_now as f32 * 60.0) as i32;
                let hours = mins_left / 60;
                let mins = mins_left % 60;
                if hours > 0 {
                    time_str = format!(
                        "{hours} hour{}, {mins} min{} remaining",
                        if hours == 1 { "" } else { "s" },
                        if mins == 1 { "" } else { "s" }
                    );
                } else {
                    time_str = format!("{mins} min{} remaining", if mins == 1 { "" } else { "s" });
                }
            }
        }
        let model_path = format!("/sys/class/power_supply/{bat_name}/model_name");
        let mut model = read_first_line(&model_path).unwrap_or_default();
        if model.is_empty() {
            let man_path = format!("/sys/class/power_supply/{bat_name}/manufacturer");
            model = read_first_line(&man_path).unwrap_or_default();
        }
        let label = if !model.is_empty() {
            format!("Battery ({model})")
        } else {
            "Battery".to_string()
        };
        if capacity >= 0 {
            let color = if capacity >= 50 {
                "32"
            } else if capacity >= 20 {
                "93"
            } else {
                "31"
            };
            let value = if !time_str.is_empty() && !status.is_empty() {
                format!("\u{1b}[{color}m{capacity}%\u{1b}[0m ({time_str}) [{status}]")
            } else if !status.is_empty() {
                format!("\u{1b}[{color}m{capacity}%\u{1b}[0m [{status}]")
            } else {
                format!("\u{1b}[{color}m{capacity}%\u{1b}[0m")
            };
            Some((label, value))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Power profile
// ---------------------------------------------------------------------------

pub fn gather_powerprofile() -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    return None;

    #[cfg(target_os = "linux")]
    {
        let mut profile =
            read_first_line("/sys/firmware/acpi/platform_profile").unwrap_or_default();
        if profile.is_empty() {
            if let Ok(out) = std::process::Command::new("sh")
                .arg("-c")
                .arg("powerprofilesctl get 2>/dev/null")
                .output()
            {
                profile = String::from_utf8_lossy(&out.stdout)
                    .trim_end_matches(['\n', '\r'])
                    .to_string();
            }
        }
        if profile.is_empty() {
            return None;
        }
        let mut chars: Vec<char> = profile.chars().collect();
        if let Some(c) = chars.first_mut() {
            if c.is_ascii_lowercase() {
                *c = c.to_ascii_uppercase();
            }
        }
        for c in &mut chars {
            if *c == '-' {
                *c = ' ';
            }
        }
        Some(chars.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// Terminal
// ---------------------------------------------------------------------------

fn is_shell_terminal(s: &str) -> bool {
    matches!(
        s,
        "bash" | "zsh" | "sh" | "dash" | "fish" | "nu" | "elvish" | "xonsh" | "tcsh" | "csh"
    )
}

fn walk_up_linux() -> Option<String> {
    let mut pid = std::process::id();
    for _ in 0..4 {
        let ppid = read_ppid_for(pid)?;
        pid = ppid;
        let comm_path = format!("/proc/{pid}/comm");
        let term = read_first_line(&comm_path)?;
        if !term.is_empty() && !is_shell_terminal(&term) {
            return Some(term);
        }
    }
    None
}

pub fn gather_terminal() -> Option<(String, Option<String>)> {
    let term = std::env::var("TERM_PROGRAM")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("KITTY_WINDOW_ID")
                .ok()
                .map(|_| "kitty".to_string())
        })
        .or_else(|| {
            std::env::var("ALACRITTY_LOG")
                .ok()
                .map(|_| "alacritty".to_string())
        })
        .or_else(|| {
            std::env::var("WEZTERM_PANE")
                .ok()
                .map(|_| "wezterm".to_string())
        })
        .or_else(|| {
            std::env::var("GHOSTTY_RESOURCES_DIR")
                .ok()
                .map(|_| "ghostty".to_string())
        })
        .or_else(|| {
            std::env::var("TERMINAL_EMULATOR")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(walk_up_linux);
    let term = term?;
    let version = get_cmd_version(&term);
    Some((term, version))
}

// ---------------------------------------------------------------------------
// IP / interfaces (getifaddrs)
// ---------------------------------------------------------------------------

fn iface_label(name: &str, _is_default: bool) -> String {
    let kind = if name.starts_with("tailscale") {
        Some("Tailscale")
    } else if name.starts_with("zt") {
        Some("ZeroTier")
    } else if name.starts_with("wg") {
        Some("WireGuard")
    } else if name.starts_with("ppp") {
        Some("PPP")
    } else if name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("veth")
        || name.starts_with("virbr")
    {
        Some("Virtual")
    } else if name.starts_with("utun") || name.starts_with("tun") || name.starts_with("tap") {
        Some("VPN")
    } else {
        None
    };
    match kind {
        Some(k) => format!("{k} ({name})"),
        None => format!("Local IP ({name})"),
    }
}

fn get_default_route_iface() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("route -n get default 2>/dev/null")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(p) = line.find("interface:") {
                let mut v = line[p + 10..]
                    .trim_start()
                    .trim_end_matches(['\n', '\r', ' '])
                    .to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string("/proc/net/route").ok()?;
        let mut lines = text.lines();
        lines.next(); // skip header
        let mut best_metric = u64::MAX;
        let mut found = None;
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let dest = u64::from_str_radix(parts[1], 16).unwrap_or(0);
            let metric: u64 = parts[2].parse().unwrap_or(u64::MAX);
            if dest == 0 && metric < best_metric {
                best_metric = metric;
                found = Some(parts[0].to_string());
            }
        }
        found
    }
}

pub fn gather_ips() -> Vec<(String, String)> {
    #[cfg(unix)]
    {
        let mut out = Vec::new();
        let default_iface = get_default_route_iface();
        unsafe {
            let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
            if libc::getifaddrs(&mut list) != 0 {
                return out;
            }
            let mut entries: Vec<(String, String, bool)> = Vec::new();
            let mut cur = list;
            while !cur.is_null() {
                let ifa = &*cur;
                let addr = ifa.ifa_addr;
                if addr.is_null() {
                    cur = ifa.ifa_next;
                    continue;
                }
                if (*addr).sa_family as i32 != libc::AF_INET {
                    cur = ifa.ifa_next;
                    continue;
                }
                let name = cstr_from_ptr(ifa.ifa_name);
                #[cfg(target_os = "macos")]
                if name == "lo0" {
                    cur = ifa.ifa_next;
                    continue;
                }
                #[cfg(not(target_os = "macos"))]
                if name == "lo" {
                    cur = ifa.ifa_next;
                    continue;
                }
                let sin = &*(addr as *const libc::sockaddr_in);
                let ip = u32::from_be(sin.sin_addr.s_addr);
                let addr_str = format!(
                    "{}.{}.{}.{}",
                    ip >> 24,
                    (ip >> 16) & 0xFF,
                    (ip >> 8) & 0xFF,
                    ip & 0xFF
                );
                let bits = ifa.ifa_netmask.as_ref().map_or(0, |mask| {
                    let mask_sin = &*(mask as *const libc::sockaddr as *const libc::sockaddr_in);
                    let m = u32::from_be(mask_sin.sin_addr.s_addr);
                    let mut bits = 0u32;
                    let mut mm = m;
                    while mm & 0x80000000 != 0 {
                        bits += 1;
                        mm <<= 1;
                    }
                    bits
                });
                let is_default = default_iface.as_deref() == Some(&name);
                let addr_full = if bits > 0 {
                    format!("{addr_str}/{bits}")
                } else {
                    addr_str
                };
                entries.push((name, addr_full, is_default));
                cur = ifa.ifa_next;
            }
            libc::freeifaddrs(list);
            for pass in [1, 0] {
                for (name, addr, is_default) in &entries {
                    if *is_default as i32 != pass {
                        continue;
                    }
                    let lbl = iface_label(name, *is_default);
                    out.push((lbl, addr.clone()));
                }
            }
        }
        out
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Locale
// ---------------------------------------------------------------------------

pub fn gather_locale() -> Option<String> {
    std::env::var("LANG").ok().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// GTK / Qt / KDE desktop settings
// ---------------------------------------------------------------------------

fn read_ini_key(path: &str, section: Option<&str>, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let keylen = key.len();
    let seclen = section.map(|s| s.len()).unwrap_or(0);
    let mut in_section = section.is_none();
    for raw in text.lines() {
        let line = raw.trim_start();
        let line = line.trim_end_matches(['\n', '\r', ' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(sec) = section {
                in_section = rest.starts_with(sec) && rest.as_bytes().get(seclen) == Some(&b']');
            }
            continue;
        }
        if !in_section || !line.starts_with(key) {
            continue;
        }
        let mut val = line[keylen..].trim_start();
        if !val.starts_with('=') {
            continue;
        }
        val = &val[1..];
        val = val.trim_start();
        let mut val = val.to_string();
        let vlen = val.len();
        if vlen >= 2 {
            let bytes = val.as_bytes();
            let first = bytes[0];
            let last = bytes[vlen - 1];
            if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                val.pop();
                val.remove(0);
            }
        }
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}

fn home() -> Option<String> {
    std::env::var("HOME").ok()
}

fn read_gtk3_setting(key: &str) -> Option<String> {
    let home = home()?;
    read_ini_key(&format!("{home}/.config/gtk-3.0/settings.ini"), None, key)
}

fn read_gtk2_setting(key: &str) -> Option<String> {
    let home = home()?;
    if let Some(v) = read_ini_key(&format!("{home}/.gtkrc-2.0"), None, key) {
        return Some(v);
    }
    read_ini_key(&format!("{home}/.config/gtk-2.0/gtkrc"), None, key)
}

fn read_gtk_setting(key: &str) -> Option<String> {
    read_gtk3_setting(key)
}

fn qtct_conf_path() -> Option<String> {
    let home = home()?;
    let platform = std::env::var("QT_QPA_PLATFORMTHEME").ok();
    if let Some(p) = platform.as_deref()
        && p.starts_with("qt")
    {
        let path = format!("{home}/.config/{p}/{p}.conf");
        if std::fs::metadata(&path).is_ok() {
            return Some(path);
        }
    }
    for variant in ["qt6ct/qt6ct.conf", "qt5ct/qt5ct.conf"] {
        let path = format!("{home}/.config/{variant}");
        if std::fs::metadata(&path).is_ok() {
            return Some(path);
        }
    }
    None
}

fn qt_theme() -> Option<String> {
    if let Some(path) = qtct_conf_path()
        && let Some(v) = read_ini_key(&path, Some("Appearance"), "style")
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Some(home) = home() {
        if let Some(v) = read_ini_key(
            &format!("{home}/.config/kdeglobals"),
            Some("KDE"),
            "widgetStyle",
        ) {
            if v.contains("ct-style") {
                return None;
            }
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn qt_icons() -> Option<String> {
    if let Some(path) = qtct_conf_path()
        && let Some(v) = read_ini_key(&path, Some("Appearance"), "icon_theme")
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Some(home) = home()
        && let Some(v) = read_ini_key(
            &format!("{home}/.config/kdeglobals"),
            Some("Icons"),
            "Theme",
        )
        && !v.is_empty()
    {
        return Some(v);
    }
    None
}

fn qt_font() -> Option<String> {
    let path = qtct_conf_path()?;
    let raw = read_ini_key(&path, Some("Fonts"), "general")?;
    let comma = raw.find(',')?;
    let (front, rest) = raw.split_at(comma);
    let size = rest
        .trim_start_matches(',')
        .split(',')
        .next()
        .unwrap_or(rest);
    Some(format!("{front} ({size}pt)"))
}

fn font_pt_format(s: &mut String) {
    let last = match s.rfind(' ') {
        Some(l) => l,
        None => return,
    };
    if last + 1 >= s.len() {
        return;
    }
    let tail = &s[last + 1..];
    if !tail.bytes().all(|b| b.is_ascii_digit()) {
        return;
    }
    *s = format!("{} ({}pt)", &s[..last], tail);
}

fn append_tagged(dst: &mut String, val: &str, tag: &str) {
    if val.is_empty() {
        return;
    }
    if !dst.is_empty() {
        dst.push_str(", ");
    }
    dst.push_str(&format!("{val} [{tag}]"));
}

fn append_gtk_pair(dst: &mut String, gtk2: &str, gtk3: &str) {
    if !gtk2.is_empty() && !gtk3.is_empty() && gtk2 == gtk3 {
        append_tagged(dst, gtk3, "GTK2/3");
        return;
    }
    append_tagged(dst, gtk2, "GTK2");
    append_tagged(dst, gtk3, "GTK3");
}

pub fn gather_theme() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("defaults read -g AppleInterfaceStyle 2>/dev/null")
            .output()
            .ok()?;
        let style = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_string();
        if !style.is_empty() {
            return Some(style);
        }
        Some("Light".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut out = String::new();
        if let Some(qt) = qt_theme() {
            append_tagged(&mut out, &qt, "Qt");
        }
        let gtk2 = read_gtk2_setting("gtk-theme-name").unwrap_or_default();
        let gtk3 = read_gtk3_setting("gtk-theme-name").unwrap_or_default();
        append_gtk_pair(&mut out, &gtk2, &gtk3);
        if out.is_empty() { None } else { Some(out) }
    }
}

pub fn gather_icons() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        Some("System".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut out = String::new();
        if let Some(qt) = qt_icons() {
            append_tagged(&mut out, &qt, "Qt");
        }
        let gtk2 = read_gtk2_setting("gtk-icon-theme-name").unwrap_or_default();
        let gtk3 = read_gtk3_setting("gtk-icon-theme-name").unwrap_or_default();
        append_gtk_pair(&mut out, &gtk2, &gtk3);
        if out.is_empty() { None } else { Some(out) }
    }
}

pub fn gather_font() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("defaults read NSGlobalDomain AppleFont 2>/dev/null")
            .output()
            .ok()?;
        let font = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_string();
        if font.is_empty() { None } else { Some(font) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut out = String::new();
        if let Some(qt) = qt_font() {
            append_tagged(&mut out, &qt, "Qt");
        }
        let mut gtk2 = read_gtk2_setting("gtk-font-name").unwrap_or_default();
        let mut gtk3 = read_gtk3_setting("gtk-font-name").unwrap_or_default();
        font_pt_format(&mut gtk2);
        font_pt_format(&mut gtk3);
        append_gtk_pair(&mut out, &gtk2, &gtk3);
        if out.is_empty() { None } else { Some(out) }
    }
}

pub fn gather_cursor() -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let cursor = read_gtk_setting("gtk-cursor-theme-name")?;
        if cursor.is_empty() {
            return None;
        }
        if let Some(size) = read_gtk_setting("gtk-cursor-theme-size") {
            if !size.is_empty() {
                return Some(format!("{cursor} ({size}px)"));
            }
        }
        Some(cursor)
    }
}

// ---------------------------------------------------------------------------
// Colors field
// ---------------------------------------------------------------------------

pub const COLOR_LINES: [&str; 2] = [
    "\u{1b}[40m   \u{1b}[41m   \u{1b}[42m   \u{1b}[43m   \u{1b}[44m   \u{1b}[45m   \u{1b}[46m   \u{1b}[47m   \u{1b}[0m",
    "\u{1b}[100m   \u{1b}[101m   \u{1b}[102m   \u{1b}[103m   \u{1b}[104m   \u{1b}[105m   \u{1b}[106m   \u{1b}[107m   \u{1b}[0m",
];
