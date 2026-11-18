//! Distro detection, ported from fetch.c.
//! fastfetch --json first, then /etc/os-release, then macOS sw_vers.

use crate::util::strip_quotes;

pub fn detect_distro() -> (String, String) {
    // returns (id, id_like)
    #[cfg(target_os = "macos")]
    {
        if let Some((id, like)) = detect_distro_fastfetch() {
            return (id, like);
        }
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg("sw_vers -productName 2>/dev/null")
            .output()
        {
            let name = String::from_utf8_lossy(&out.stdout)
                .trim_end_matches(['\n', '\r'])
                .to_string();
            if !name.is_empty() {
                return (name.to_lowercase().replace(' ', ""), String::new());
            }
        }
        return ("macos".to_string(), String::new());
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some((id, like)) = detect_distro_fastfetch() {
            return (id, like);
        }
        detect_distro_os_release()
    }
}

fn detect_distro_fastfetch() -> Option<(String, String)> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("fastfetch -c none --json 2>/dev/null")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found_os = false;
    let mut id = String::new();
    let mut like = String::new();
    for line in text.lines() {
        if line.contains("\"OS\"") {
            found_os = true;
        }
        if found_os {
            if let Some(pos) = line.find("\"id\"")
                && let Some(val) = json_string_after_colon(&line[pos..])
            {
                id = val;
                break;
            }
            if let Some(pos) = line.find("\"idLike\"")
                && let Some(val) = json_string_after_colon(&line[pos..])
            {
                like = val;
            }
        }
    }
    if id.is_empty() {
        None
    } else {
        Some((id, like))
    }
}

fn json_string_after_colon(s: &str) -> Option<String> {
    let colon = s.find(':')?;
    let rest = &s[colon + 1..];
    let q1 = rest.find('"')?;
    let after = &rest[q1 + 1..];
    let q2 = after.find('"')?;
    let val = &after[..q2];
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

fn detect_distro_os_release() -> (String, String) {
    let mut id = String::new();
    let mut like = String::new();
    if let Ok(t) = std::fs::read_to_string("/etc/os-release") {
        for line in t.lines() {
            if id.is_empty() {
                if let Some(rest) = line.strip_prefix("ID=") {
                    id = strip_quotes(rest);
                }
            } else if let Some(rest) = line.strip_prefix("ID_LIKE=") {
                like = strip_quotes(rest);
            }
        }
    }
    (id, like)
}
