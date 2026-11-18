//! Config file parsing, ported from fetch.c.

pub const F_COUNT: usize = 24;

pub const F_OS: usize = 0;
pub const F_HOST: usize = 1;
pub const F_KERNEL: usize = 2;
pub const F_UPTIME: usize = 3;
pub const F_PACKAGES: usize = 4;
pub const F_SHELL: usize = 5;
pub const F_DISPLAY: usize = 6;
pub const F_WM: usize = 7;
pub const F_DISPLAYMANAGER: usize = 8;
pub const F_THEME: usize = 9;
pub const F_ICONS: usize = 10;
pub const F_FONT: usize = 11;
pub const F_CURSOR: usize = 12;
pub const F_TERMINAL: usize = 13;
pub const F_CPU: usize = 14;
pub const F_GPU: usize = 15;
pub const F_MEMORY: usize = 16;
pub const F_SWAP: usize = 17;
pub const F_DISK: usize = 18;
pub const F_IP: usize = 19;
pub const F_BATTERY: usize = 20;
pub const F_POWERPROFILE: usize = 21;
pub const F_LOCALE: usize = 22;
pub const F_COLORS: usize = 23;

pub const F_SEPARATOR: usize = F_COUNT;
pub const F_CUSTOM_BASE: usize = F_COUNT + 1;
pub const MAX_CUSTOM: usize = 16;
pub const MAX_FIELDS: usize = F_COUNT + MAX_CUSTOM + 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone)]
pub struct CustomField {
    pub label: String,
    pub value: String,
}

pub struct Config {
    pub field_enabled: [bool; F_COUNT],
    pub field_order: Vec<usize>,
    pub label_color: String,
    pub config_height: i32,
    pub size_scale: f32,
    pub config_speed: f32,
    pub config_speed_set: bool,
    pub config_spin_x: i32,
    pub config_spin_y: i32,
    pub config_box: bool,
    pub config_show_title: bool,
    pub config_shading: String,
    pub config_shading_mode: String,
    pub config_separator: String,
    pub config_depth: f32,
    pub depth_user_set: bool,
    pub config_logo_outer: String,
    pub config_logo_inner: String,
    pub extra_disks: Vec<String>,
    pub config_v_alignment: VAlign,
    pub config_h_alignment: HAlign,
    pub light_x: f32,
    pub light_y: f32,
    pub light_z: f32,
    pub custom_fields: Vec<CustomField>,
}

impl Config {
    pub fn defaults() -> Self {
        let mut cfg = Self {
            field_enabled: [false; F_COUNT],
            field_order: Vec::new(),
            label_color: "35".to_string(),
            config_height: 0,
            size_scale: 1.0,
            config_speed: 0.0,
            config_speed_set: false,
            config_spin_x: -1,
            config_spin_y: -1,
            config_box: false,
            config_show_title: true,
            config_shading: String::new(),
            config_shading_mode: String::new(),
            config_separator: "-".to_string(),
            config_depth: 1.0,
            depth_user_set: false,
            config_logo_outer: String::new(),
            config_logo_inner: String::new(),
            extra_disks: Vec::new(),
            config_v_alignment: VAlign::Top,
            config_h_alignment: HAlign::Left,
            light_x: 0.4082,
            light_y: 0.8165,
            light_z: -0.4082,
            custom_fields: Vec::new(),
        };
        cfg.apply_defaults();
        cfg
    }

    fn apply_defaults(&mut self) {
        let defaults: [usize; 22] = [
            F_OS, F_HOST, F_KERNEL, F_UPTIME, F_PACKAGES, F_SHELL, F_DISPLAY, F_WM, F_THEME,
            F_ICONS, F_FONT, F_CURSOR, F_TERMINAL, F_CPU, F_GPU, F_MEMORY, F_SWAP, F_DISK, F_IP,
            F_BATTERY, F_LOCALE, F_COLORS,
        ];
        self.field_order = defaults.to_vec();
        for &id in &defaults {
            self.field_enabled[id] = true;
        }
    }
}

const FIELD_MAP: [(&str, usize); 24] = [
    ("os", F_OS),
    ("host", F_HOST),
    ("kernel", F_KERNEL),
    ("uptime", F_UPTIME),
    ("packages", F_PACKAGES),
    ("shell", F_SHELL),
    ("display", F_DISPLAY),
    ("wm", F_WM),
    ("displaymanager", F_DISPLAYMANAGER),
    ("theme", F_THEME),
    ("icons", F_ICONS),
    ("font", F_FONT),
    ("cursor", F_CURSOR),
    ("terminal", F_TERMINAL),
    ("cpu", F_CPU),
    ("gpu", F_GPU),
    ("memory", F_MEMORY),
    ("swap", F_SWAP),
    ("disk", F_DISK),
    ("ip", F_IP),
    ("battery", F_BATTERY),
    ("powerprofile", F_POWERPROFILE),
    ("locale", F_LOCALE),
    ("colors", F_COLORS),
];

pub fn parse_number(option: &str, text: &str, min: f32, max: f32, out: &mut f32) -> bool {
    let Ok(value) = text.parse::<f32>() else {
        eprintln!("fetch: {option} requires a finite number from {min} to {max} (got '{text}')");
        return false;
    };
    if !value.is_finite() || value < min || value > max {
        eprintln!("fetch: {option} requires a finite number from {min} to {max} (got '{text}')");
        return false;
    }
    *out = value;
    true
}

pub fn parse_integer(option: &str, text: &str, min: i32, max: i32, out: &mut i32) -> bool {
    let Ok(value) = text.parse::<i64>() else {
        eprintln!("fetch: {option} requires an integer from {min} to {max} (got '{text}')");
        return false;
    };
    if value < min as i64 || value > max as i64 {
        eprintln!("fetch: {option} requires an integer from {min} to {max} (got '{text}')");
        return false;
    }
    *out = value as i32;
    true
}

fn strip_inline_hint(val: &mut String) {
    if let Some(pos) = val.find(" (") {
        val.truncate(pos);
    }
    val.truncate(val.trim_end_matches([' ', '\t']).len());
}

fn ansi_for_color_name(val: &str) -> String {
    match val {
        "red" => "\u{1b}[1;31m".to_string(),
        "green" => "\u{1b}[1;32m".to_string(),
        "yellow" => "\u{1b}[1;33m".to_string(),
        "blue" => "\u{1b}[1;34m".to_string(),
        "magenta" => "\u{1b}[1;35m".to_string(),
        "cyan" => "\u{1b}[1;36m".to_string(),
        "white" => "\u{1b}[1;37m".to_string(),
        other => format!("\u{1b}[1;{}m", other),
    }
}

pub fn load_config(home: &str) -> Config {
    let mut cfg = Config::defaults();

    let path = std::path::Path::new(home).join(".config/fetch/config");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return cfg;
    };

    // Config file exists — reset defaults, use file order
    cfg.field_enabled.fill(false);
    cfg.field_order.clear();

    for raw in text.lines() {
        let mut line = raw.to_string();
        while line.ends_with('\n') || line.ends_with('\r') || line.ends_with(' ') {
            line.pop();
        }
        let line = line.trim_start().to_string();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("label_color=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.label_color = match val.as_str() {
                "red" => "31".to_string(),
                "green" => "32".to_string(),
                "yellow" => "33".to_string(),
                "blue" => "34".to_string(),
                "magenta" => "35".to_string(),
                "cyan" => "36".to_string(),
                "white" => "37".to_string(),
                other => other.to_string(),
            };
            continue;
        }
        if let Some(rest) = line.strip_prefix("height=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            let mut h = 0;
            if parse_integer("height", &val, 1, 200, &mut h) {
                cfg.config_height = h;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("size=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            let mut s = 1.0f32;
            if parse_number("size", &val, 0.5, 5.0, &mut s) {
                cfg.size_scale = s;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("speed=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            if parse_number("speed", &val, -1000.0, 1000.0, &mut cfg.config_speed) {
                cfg.config_speed_set = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("spin=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_spin_x = if val.contains('x') || val.contains('X') {
                1
            } else {
                0
            };
            cfg.config_spin_y = if val.contains('y') || val.contains('Y') {
                1
            } else {
                0
            };
            continue;
        }
        if let Some(rest) = line.strip_prefix("show_title=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_show_title = matches!(
                val.as_str(),
                "1" | "y" | "Y" | "yes" | "YES" | "Yes" | "true" | "TRUE" | "True"
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("box=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_box = matches!(
                val.as_str(),
                "1" | "y" | "Y" | "yes" | "YES" | "Yes" | "true" | "TRUE" | "True"
            ) as i32
                != 0;
            continue;
        }
        if let Some(rest) = line.strip_prefix("shading=") {
            // no strip_inline_hint: allow freeform shading strings
            cfg.config_shading = rest.to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("shading_mode=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_shading_mode = val;
            continue;
        }
        if let Some(rest) = line.strip_prefix("separator=") {
            // no strip_inline_hint: allow freeform separator strings
            cfg.config_separator = rest.to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("depth=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            let mut d = 1.0f32;
            if parse_number("depth", &val, 0.1, 10.0, &mut d) {
                cfg.config_depth = d;
                cfg.depth_user_set = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("logo_outer=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_logo_outer = ansi_for_color_name(&val);
            continue;
        }
        if let Some(rest) = line.strip_prefix("logo_inner=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_logo_inner = ansi_for_color_name(&val);
            continue;
        }
        if let Some(rest) = line.strip_prefix("light=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            match val.as_str() {
                "top-left" => {
                    cfg.light_x = 0.41;
                    cfg.light_y = 0.82;
                    cfg.light_z = -0.41;
                }
                "top-right" => {
                    cfg.light_x = -0.41;
                    cfg.light_y = 0.82;
                    cfg.light_z = -0.41;
                }
                "top" => {
                    cfg.light_x = 0.0;
                    cfg.light_y = 0.89;
                    cfg.light_z = -0.45;
                }
                "left" => {
                    cfg.light_x = 0.82;
                    cfg.light_y = 0.41;
                    cfg.light_z = -0.41;
                }
                "right" => {
                    cfg.light_x = -0.82;
                    cfg.light_y = 0.41;
                    cfg.light_z = -0.41;
                }
                "front" => {
                    cfg.light_x = 0.0;
                    cfg.light_y = 0.0;
                    cfg.light_z = -1.0;
                }
                "bottom-left" => {
                    cfg.light_x = 0.41;
                    cfg.light_y = -0.82;
                    cfg.light_z = -0.41;
                }
                "bottom-right" => {
                    cfg.light_x = -0.41;
                    cfg.light_y = -0.82;
                    cfg.light_z = -0.41;
                }
                _ => {}
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("disk=") {
            // no strip_inline_hint: allow spaces in path
            let path_val = rest;
            if !path_val.is_empty() && cfg.extra_disks.len() < 8 {
                cfg.extra_disks.push(path_val.to_string());
            }
            if !cfg.field_enabled[F_DISK] && cfg.field_order.len() < MAX_FIELDS {
                cfg.field_enabled[F_DISK] = true;
                cfg.field_order.push(F_DISK);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("v_alignment=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_v_alignment = match val.as_str() {
                "top" => VAlign::Top,
                "center" => VAlign::Center,
                "bottom" => VAlign::Bottom,
                _ => cfg.config_v_alignment,
            };
            continue;
        }
        if let Some(rest) = line.strip_prefix("h_alignment=") {
            let mut val = rest.to_string();
            strip_inline_hint(&mut val);
            cfg.config_h_alignment = match val.as_str() {
                "left" => HAlign::Left,
                "center" => HAlign::Center,
                "right" => HAlign::Right,
                _ => cfg.config_h_alignment,
            };
            continue;
        }
        if line == "---" {
            if cfg.field_order.len() < MAX_FIELDS {
                cfg.field_order.push(F_SEPARATOR);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("custom_")
            && let Some(eq) = rest.find('=')
            && eq > 0
            && cfg.custom_fields.len() < MAX_CUSTOM
            && cfg.field_order.len() < MAX_FIELDS
        {
            let label = rest[..eq].to_string();
            let value = rest[eq + 1..].to_string();
            cfg.custom_fields.push(CustomField { label, value });
            cfg.field_order
                .push(F_CUSTOM_BASE + cfg.custom_fields.len() - 1);
        }
        for (name, id) in FIELD_MAP {
            if line.eq_ignore_ascii_case(name) {
                if !cfg.field_enabled[id] && cfg.field_order.len() < MAX_FIELDS {
                    cfg.field_enabled[id] = true;
                    cfg.field_order.push(id);
                }
                break;
            }
        }
    }

    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integer_range() {
        let mut v = 0;
        assert!(parse_integer("height", "36", 1, 200, &mut v));
        assert_eq!(v, 36);
        assert!(!parse_integer("height", "0", 1, 200, &mut v));
    }

    #[test]
    fn strip_hint() {
        let mut v = "white (red, green)".to_string();
        strip_inline_hint(&mut v);
        assert_eq!(v, "white");
    }
}
