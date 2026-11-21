//! Application state and frame rendering, extracted from `main.rs`.
//!
//! Holds the `App` struct (logo, shading, layout, buffers, animation state,
//! info lines, …) and the per-frame render pass.

use std::env;
use std::ffi::c_char;
use std::sync::{Arc, Mutex};

use crate::ansi::{emit_clipped, visible_width};
use crate::config::{self, Config, F_CUSTOM_BASE, MAX_CUSTOM, F_SEPARATOR};
use crate::distro;
use crate::info::{self, COLOR_LINES};
use crate::logo::{self, Logo};
use crate::points::{build_points, Points};
use crate::render::{self, Buffers, Layout};
use crate::shading::{Shading, select_shading};
use crate::terminal::get_term_size;

// ---------------------------------------------------------------------------
// application state
// ---------------------------------------------------------------------------

pub struct App {
    pub config: Config,
    pub distro: String,
    pub logo: Logo,
    pub shading: Shading,
    pub sub_rows: i32,
    pub sub_cols: i32,
    pub layout: Layout,
    /// Render cache of the info block. The fast-changing fields (uptime,
    /// memory, swap, battery) are refreshed by a background thread so that
    /// spawning PowerShell on Windows can never block — and freeze — the
    /// animation render loop.
    pub info_cache: Arc<Mutex<Vec<String>>>,
    pub buffers: Buffers,
    pub points: Option<Points>,
    pub fetch_lines: Vec<String>,
    pub field_line: [i32; config::F_COUNT],
    pub current_field: i32,
    pub is_refresh_pass: bool,
    pub box_width: i32,
    pub term_rows: i32,
    pub term_cols: i32,
    // animation
    pub a: f32,
    pub b: f32,
    pub k1: f32,
    pub fixed_y_center: f32,
    pub face_up: f32,
    pub face_dn: f32,
    // colors
    pub color_inner: String,
    pub color_outer: String,
    // runtime
    pub show_info: bool,
    pub use_color: bool,
    pub max_frames: i32,
    pub speed: f32,
    pub rotate_x: i32,
    pub rotate_y: i32,
    pub fetch_start: i32,
    // mouse
    pub mouse_dragging: bool,
    pub mouse_last_x: i32,
    pub mouse_last_y: i32,
    pub drag_vx: f32,
    pub drag_vy: f32,
}

// ---------------------------------------------------------------------------
// App construction + info gathering
// ---------------------------------------------------------------------------

impl App {
    pub fn new(
        show_info: bool,
        use_color: bool,
        max_frames: i32,
        speed: f32,
        rotate_x: i32,
        rotate_y: i32,
        config: Config,
        home: &str,
        logo_name: Option<&str>,
    ) -> Self {
        let (mut distro, distro_id_like) = distro::detect_distro();
        let mut logo = Logo::new();

        // Logo: CLI --logo takes exclusivity (matches fetch.c).
        if let Some(name) = logo_name {
            if !logo::load_logo_fastfetch(name, &mut logo) {
                logo::load_default_logo(&mut logo);
            }
            distro = name.to_string();
        } else {
            // logo: ~/.config/fetch/logo.txt first
            let _has_custom_logo = logo::load_logo_file(home, &mut logo);
            if !logo.distro_hint.is_empty() {
                distro = logo.distro_hint.clone();
            }
            let mut got_logo = _has_custom_logo;
            if !got_logo && !distro.is_empty() {
                got_logo = logo::load_logo_fastfetch(&distro, &mut logo);
                if !got_logo && !distro_id_like.is_empty() {
                    for tok in distro_id_like.split(' ') {
                        if tok.is_empty() {
                            continue;
                        }
                        if logo::load_logo_fastfetch(tok, &mut logo) {
                            distro = tok.to_string();
                            got_logo = true;
                            break;
                        }
                    }
                }
            }
            if !got_logo && logo.rows.is_empty() {
                logo::load_default_logo(&mut logo);
            }
        }

        logo::process_logo(&mut logo);

        let mut shading = Shading::new();
        let sub_rows = 1usize;
        let sub_cols = 1usize;
        let shading_chars = if config.config_shading.is_empty() {
            None
        } else {
            Some(config.config_shading.as_str())
        };
        if !select_shading(
            Some(&config.config_shading_mode),
            shading_chars,
            &mut (sub_rows, sub_cols),
            &mut shading,
        ) {
            select_shading(None, None, &mut (sub_rows, sub_cols), &mut shading);
        }

        let layout = Layout::new();
        let buffers = Buffers::new();

        let color_inner = "\u{1b}[1;37m".to_string();
        let color_outer = "\u{1b}[1;35m".to_string();

        let mut app = Self {
            config,
            distro,
            logo,
            shading,
            sub_rows: sub_rows as i32,
            sub_cols: sub_cols as i32,
            layout,
            info_cache: Arc::new(Mutex::new(Vec::new())),
            buffers,
            points: None,
            fetch_lines: Vec::new(),
            field_line: [-1; config::F_COUNT],
            current_field: -1,
            is_refresh_pass: false,
            box_width: 0,
            term_rows: 0,
            term_cols: 0,
            a: 0.0,
            b: 0.0,
            k1: 0.0,
            fixed_y_center: 0.0,
            face_up: 0.0,
            face_dn: 0.0,
            color_inner,
            color_outer,
            show_info,
            use_color,
            max_frames,
            speed,
            rotate_x,
            rotate_y,
            fetch_start: if show_info { 1 } else { 0 },
            mouse_dragging: false,
            mouse_last_x: 0,
            mouse_last_y: 0,
            drag_vx: 0.0,
            drag_vy: 0.0,
        };

        (app.term_rows, app.term_cols) = get_term_size();
        app
    }

    pub(crate) fn set_distro_colors(&mut self) {
        let d = self.distro.to_lowercase();
        let (outer, inner) = match d.as_str() {
            "gentoo" => ("\u{1b}[1;35m", "\u{1b}[1;37m"),
            "arch" => ("\u{1b}[1;36m", "\u{1b}[1;36m"),
            "ubuntu" | "debian" => ("\u{1b}[1;31m", "\u{1b}[1;37m"),
            "asahi" | "asahi2" | "fedora-asahi-remix" => ("\u{1b}[1;31m", "\u{1b}[1;37m"),
            s if s == "fedora" || s.starts_with("fedora-") => ("\u{1b}[1;34m", "\u{1b}[1;37m"),
            "nixos" => ("\u{1b}[1;34m", "\u{1b}[1;36m"),
            "void" => ("\u{1b}[1;32m", "\u{1b}[1;32m"),
            "alpine" => ("\u{1b}[1;34m", "\u{1b}[1;37m"),
            s if s == "opensuse-tumbleweed" || s == "opensuse-leap" || s == "opensuse" => {
                ("\u{1b}[1;32m", "\u{1b}[1;37m")
            }
            "macos" => ("\u{1b}[1;36m", "\u{1b}[1;37m"),
            _ => ("\u{1b}[1;35m", "\u{1b}[1;37m"),
        };
        self.color_outer = outer.to_string();
        self.color_inner = inner.to_string();
        if !self.config.config_logo_outer.is_empty() {
            self.color_outer = self.config.config_logo_outer.clone();
        }
        if !self.config.config_logo_inner.is_empty() {
            self.color_inner = self.config.config_logo_inner.clone();
        }
    }

    fn add_line(&mut self, line: String) {
        if self.fetch_lines.len() >= 32 {
            return;
        }
        self.fetch_lines.push(line);
    }

    /// Format a labeled info line using the configured label color.
    fn add_info(&mut self, label: &str, value: &str) {
        let line = format!(
            "\u{1b}[1;{}m{}\u{1b}[0m: {}",
            self.config.label_color, label, value
        );

        if self.is_refresh_pass
            && self.current_field >= 0
            && self.field_line[self.current_field as usize] >= 0
        {
            let idx = self.field_line[self.current_field as usize] as usize;
            let mut new_line = line.clone();
            if self.box_width > 0 {
                let w = visible_width(&line);
                let mut pad = self.box_width - w as i32;
                if pad < 0 {
                    pad = 0;
                }
                let pad_str = " ".repeat(pad.max(0) as usize);
                new_line = format!("\u{2502} {line}{pad_str} \u{2502}");
            }
            self.fetch_lines[idx] = new_line;
            return;
        }
        if self.fetch_lines.len() >= 32 {
            return;
        }
        if self.current_field >= 0 {
            self.field_line[self.current_field as usize] = self.fetch_lines.len() as i32;
        }
        self.add_line(line);
    }

    pub(crate) fn gather_all(&mut self) {
        if !self.show_info {
            return;
        }
        if self.config.config_show_title {
            self.gather_title();
        }
        let mut i = 0;
        while i < self.config.field_order.len() {
            let id = self.config.field_order[i];
            if id == F_SEPARATOR {
                self.add_line("".to_string());
            } else if id >= F_CUSTOM_BASE && id < F_CUSTOM_BASE + MAX_CUSTOM {
                let ci = id - F_CUSTOM_BASE;
                let cf = &self.config.custom_fields[ci];
                self.add_info(&cf.label.clone(), &cf.value.clone());
            } else if id == config::F_COLORS {
                self.add_line("".to_string());
                for cl in COLOR_LINES {
                    self.add_line(cl.to_string());
                }
            } else if id < config::F_COUNT {
                self.current_field = id as i32;
                match id {
                    config::F_OS => {
                        if let Some(v) = info::gather_os() {
                            self.add_info("OS", &v);
                        }
                    }
                    config::F_HOST => {
                        if let Some(v) = info::gather_host() {
                            self.add_info("Host", &v);
                        }
                    }
                    config::F_KERNEL => {
                        if let Some(v) = info::gather_kernel() {
                            self.add_info("Kernel", &v);
                        }
                    }
                    config::F_UPTIME => {
                        if let Some(v) = info::gather_uptime() {
                            self.add_info("Uptime", &v);
                        }
                    }
                    config::F_PACKAGES => {
                        if let Some(v) = info::gather_packages() {
                            self.add_info("Packages", &v);
                        }
                    }
                    config::F_SHELL => {
                        if let Some((name, version)) = info::gather_shell() {
                            match version {
                                Some(v) => self.add_info("Shell", &format!("{name} {v}")),
                                None => self.add_info("Shell", &name),
                            }
                        }
                    }
                    config::F_DISPLAY => {
                        for (label, val) in info::gather_displays() {
                            self.add_info(&label, &val);
                        }
                    }
                    config::F_WM => {
                        let is_wayland = env::var("WAYLAND_DISPLAY")
                            .map_or(false, |s| !s.is_empty())
                            || env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland");
                        if let Some(wm) = info::gather_wm() {
                            let version = info::get_cmd_version(&wm);
                            self.add_info(
                                "WM",
                                &info::wm_version_label(&wm, is_wayland, version.as_deref()),
                            );
                        }
                    }
                    config::F_DISPLAYMANAGER => {
                        if let Some(v) = info::gather_displaymanager() {
                            self.add_info("Display Manager", &v);
                        }
                    }
                    config::F_THEME => {
                        if let Some(v) = info::gather_theme() {
                            self.add_info("Theme", &v);
                        }
                    }
                    config::F_ICONS => {
                        if let Some(v) = info::gather_icons() {
                            self.add_info("Icons", &v);
                        }
                    }
                    config::F_FONT => {
                        if let Some(v) = info::gather_font() {
                            self.add_info("Font", &v);
                        }
                    }
                    config::F_CURSOR => {
                        if let Some(v) = info::gather_cursor() {
                            self.add_info("Cursor", &v);
                        }
                    }
                    config::F_TERMINAL => {
                        if let Some((name, version)) = info::gather_terminal() {
                            match version {
                                Some(v) => self.add_info("Terminal", &format!("{name} {v}")),
                                None => self.add_info("Terminal", &name),
                            }
                        }
                    }
                    config::F_CPU => {
                        if let Some(v) = info::gather_cpu() {
                            self.add_info("CPU", &v);
                        }
                    }
                    config::F_GPU => {
                        for (label, val) in info::gather_gpus() {
                            self.add_info(&label, &val);
                        }
                    }
                    config::F_MEMORY => {
                        if let Some((v, _pct)) = info::gather_memory() {
                            self.add_info("Memory", &v);
                        }
                    }
                    config::F_SWAP => {
                        if let Some((v, _pct)) = info::gather_swap() {
                            self.add_info("Swap", &v);
                        }
                    }
                    config::F_DISK => {
                        if let Some((label, val, _pct)) = info::gather_disk("/") {
                            self.add_info(&label, &val);
                        }
                        for disk in self.config.extra_disks.clone() {
                            if let Some((label, val, _pct)) = info::gather_disk(&disk) {
                                self.add_info(&label, &val);
                            }
                        }
                    }
                    config::F_IP => {
                        for (label, val) in info::gather_ips() {
                            self.add_info(&label, &val);
                        }
                    }
                    config::F_BATTERY => {
                        if let Some((label, val)) = info::gather_battery() {
                            self.add_info(&label, &val);
                        }
                    }
                    config::F_POWERPROFILE => {
                        if let Some(v) = info::gather_powerprofile() {
                            self.add_info("Power Profile", &v);
                        }
                    }
                    config::F_LOCALE => {
                        if let Some(v) = info::gather_locale() {
                            self.add_info("Locale", &v);
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        self.current_field = -1;
    }

    fn gather_title(&mut self) {
        let user = env::var("LOGNAME")
            .or_else(|_| env::var("USER"))
            .unwrap_or_default();
        let mut host = String::new();
        #[cfg(unix)]
        {
            let mut buf = [0 as c_char; 64];
            unsafe {
                if libc::gethostname(buf.as_mut_ptr(), buf.len()) == 0 {
                    host = crate::util::cstr_from_buf(&buf);
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            host = std::env::var("COMPUTERNAME").unwrap_or_default();
        }
        let line = format!(
            "\u{1b}[1;{}m{}\u{1b}[0m@\u{1b}[1;{}m{}\u{1b}[0m",
            self.config.label_color, user, self.config.label_color, host
        );
        self.add_line(line);

        let title_len = user.len() + 1 + host.len();
        let sep_char_len = self.config.config_separator.len().max(1);
        let mut sep = String::new();
        let mut pos = 0;
        while pos + sep_char_len <= title_len && sep.len() < 512 {
            sep.push_str(&self.config.config_separator);
            pos += sep_char_len;
        }
        self.add_line(sep);
    }

    /// Wrap the info lines in a box, fastfetch Caelestia style.
    pub(crate) fn box_wrap_lines(&mut self) {
        let start = if self.config.config_show_title { 2 } else { 0 };
        if start >= self.fetch_lines.len() {
            return;
        }

        let mut max_w = 0;
        for i in start..self.fetch_lines.len() {
            let w = visible_width(&self.fetch_lines[i]);
            if w > max_w {
                max_w = w;
            }
        }
        if max_w > (512usize - 16) / 3 {
            max_w = (512usize - 16) / 3;
        }
        self.box_width = max_w as i32;

        let content_count = (self.fetch_lines.len() - start).min(32usize - start - 2);
        let mut content: Vec<String> = Vec::with_capacity(content_count);
        for i in 0..content_count {
            let mut buf = [0u8; 512];
            let n = emit_clipped(&mut buf, &self.fetch_lines[start + i], max_w as i32);
            content.push(String::from_utf8_lossy(&buf[..n]).into_owned());
        }

        let mut border = String::new();
        border.push_str("\u{256F}"); // ╭
        for _ in 0..max_w + 2 {
            border.push_str("\u{2500}"); // ─
        }
        border.push_str("\u{256E}"); // ╮
        self.fetch_lines[start] = border;

        let mut out = start + 1;
        for i in 0..content_count {
            if out >= 32 {
                break;
            }
            let w = visible_width(&content[i]);
            let pad = max_w - w;
            let item = &content[i];
            let padded = format!("{item: <width$}", width = pad);
            self.fetch_lines[out] = format!("\u{2502} {padded} \u{2502}");
            out += 1;
        }

        let mut border = String::new();
        border.push_str("\u{2570}"); // ╰
        for _ in 0..max_w + 2 {
            border.push_str("\u{2500}");
        }
        border.push_str("\u{256F}"); // ╯
        if out < 32usize {
            self.fetch_lines[out] = border;
        }
        self.fetch_lines.truncate(out + 1);

        for i in 0..config::F_COUNT {
            if self.field_line[i] >= start as i32 {
                if self.field_line[i] < start as i32 + content_count as i32 {
                    self.field_line[i] += 1;
                } else {
                    self.field_line[i] = -1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// projection / layout helpers
// ---------------------------------------------------------------------------

impl App {
    fn get_alignment_padding(&self) -> (i32, i32) {
        // Recover from a poisoned lock (background thread panicked while
        // holding it) rather than crashing the render loop.
        let cache = self.info_cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut maxw = 0usize;
        for line in cache.iter() {
            let w = visible_width(line);
            if w > maxw {
                maxw = w;
            }
        }
        drop(cache);
        let vertical = match self.config.config_v_alignment {
            config::VAlign::Top => 0,
            config::VAlign::Center => self.term_rows / 2 - self.layout.render_height / 2,
            config::VAlign::Bottom => self.term_rows - self.layout.render_height,
        };
        let horizontal = match self.config.config_h_alignment {
            config::HAlign::Left => 0,
            config::HAlign::Center => {
                self.term_cols / 2 - (self.layout.anim_width + render::GAP + maxw as i32) / 2
            }
            config::HAlign::Right => {
                self.term_cols - (self.layout.anim_width + render::GAP + maxw as i32)
            }
        };
        (vertical, horizontal)
    }

    /// Clone the info line at `idx` from the render cache. The background
    /// refresh thread only holds the cache lock briefly, so this never
    /// blocks the animation loop for more than a few microseconds.
    fn info_line(&self, idx: usize) -> Option<String> {
        let cache = self.info_cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(idx).cloned()
    }

    pub(crate) fn build_points_and_projection(&mut self) {
        self.points = Some(build_points(
            &self.logo,
            self.config.size_scale,
            self.config.config_depth,
            self.config.depth_user_set,
            self.sub_rows,
        ));

        let points = self.points.as_ref().unwrap();
        let logo_height = self.layout.logo_height as f32;
        let k1 = 37.0 * logo_height / 36.0 * self.config.size_scale;

        let mut face_up = 0.0f32;
        let mut face_dn = 0.0f32;
        for i in 0..points.px.len() {
            let zc = points.pz[i] + render::K2_CONST;
            if zc < 0.1 {
                continue;
            }
            let ys = k1 * points.py[i] / zc;
            if ys > face_up {
                face_up = ys;
            }
            if -ys > face_dn {
                face_dn = -ys;
            }
        }
        self.k1 = k1;
        self.fixed_y_center = face_up + 1.0;
        self.face_up = face_up;
        self.face_dn = face_dn;
    }

    pub(crate) fn tighten_render_height(&mut self) {
        if self.config.config_height == 0 {
            let logo_bottom = (self.fixed_y_center + self.face_dn) as i32 + 2;
            let info_bottom = if self.show_info {
                self.fetch_start + self.fetch_lines.len() as i32 + 1
            } else {
                0
            };
            let needed = logo_bottom.max(info_bottom);
            if needed < self.layout.render_height {
                self.layout.render_height = needed;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// frame rendering
// ---------------------------------------------------------------------------

impl App {
    pub(crate) fn render_frame(&mut self) -> Vec<u8> {
        let points = self.points.as_ref().expect("points built");
        let smax = self.shading.chars.len() as i32 - 1;
        let half_aw = self.layout.anim_width as f32 * 0.5;
        let k1x2 = self.k1 * 2.0;
        let aw = self.layout.anim_width;

        self.buffers.clear(self.layout.render_height, self.sub_rows);

        let c_a = self.a.cos();
        let s_a = self.a.sin();
        let c_b = self.b.cos();
        let s_b = self.b.sin();

        let lx = self.config.light_x;
        let ly = self.config.light_y;
        let lz = self.config.light_z;
        let y_center = if !self.layout.layout_stacked && self.show_info {
            self.fixed_y_center
        } else {
            self.layout.render_height as f32 * 0.5
        };

        let hl = (lx * lx + ly * ly + (lz - 1.0) * (lz - 1.0)).sqrt();
        let hlx = lx / hl;
        let hly = ly / hl;
        let hlz = (lz - 1.0) / hl;

        // cached ANSI prefixes "\u{1b}[1;{c}m"
        let mut ansi_cache: [Option<String>; 128] = [(); 128].map(|_| None);

        for i in 0..points.px.len() {
            let px = points.px[i];
            let py = points.py[i];
            let pz = points.pz[i];
            let nx = points.nx[i];
            let ny = points.ny[i];
            let nz = points.nz[i];

            let y1 = py * c_a - pz * s_a;
            let z1 = py * s_a + pz * c_a;
            let x2 = px * c_b + z1 * s_b;
            let z2 = -px * s_b + z1 * c_b;
            let y2 = y1;

            let ny1 = ny * c_a - nz * s_a;
            let nz1 = ny * s_a + nz * c_a;
            let nx2 = nx * c_b + nz1 * s_b;
            let nz2 = -nx * s_b + nz1 * c_b;
            let ny2 = ny1;

            let zc = z2 + render::K2_CONST;
            if zc < 0.1 {
                continue;
            }
            let ooz = 1.0 / zc;
            let xs = ((half_aw + k1x2 * x2 * ooz) * self.sub_cols as f32) as i32;
            let ys = ((y_center - self.k1 * y2 * ooz) * self.sub_rows as f32) as i32;
            if xs < 0
                || xs >= aw * self.sub_cols
                || ys < 0
                || ys >= self.layout.render_height * self.sub_rows
            {
                continue;
            }
            let idx = (ys * render::SUB_W + xs) as usize;
            if ooz > self.buffers.zbuf[idx] {
                let diff = nx2 * lx + ny2 * ly + nz2 * lz;
                let diff = if diff < 0.0 { 0.0 } else { diff };
                let spec_dot = nx2 * hlx + ny2 * hly + nz2 * hlz;
                let spec_dot = if spec_dot < 0.0 { 0.0 } else { spec_dot };
                let mut spec = spec_dot * spec_dot;
                spec = spec * spec;
                spec = spec * spec;
                let mut l = 0.08 + 0.62 * diff + 0.30 * spec;
                if l > 1.0 {
                    l = 1.0;
                }
                self.buffers.zbuf[idx] = ooz;
                self.buffers.lumbuf[idx] = l;
                self.buffers.colorbuf[idx] = points.pcolor[i];
            }
        }

        let (top_padding, left_padding) = self.get_alignment_padding();

        let mut out = Vec::new();
        out.reserve(
            (self.layout.render_height + self.layout.stacked_info_rows + 2) as usize
                * (2048 + left_padding.max(0) as usize)
                + 64
                + top_padding.max(0) as usize,
        );
        out.push(0x1B);
        out.push(b'[');
        out.push(b'H'); // cursor to home

        if top_padding > 0 {
            for _ in 0..top_padding {
                out.push(b'\n');
            }
        }

        let reset_seq = b"\x1b[0m";
        let clr_seq = b"\x1b[K";

        for i in 0..self.layout.render_height {
            if left_padding > 0 {
                for _ in 0..left_padding {
                    out.push(b' ');
                }
            }

            if !self.use_color {
                for j in 0..aw {
                    let mut color = 0;
                    let sc = cell_glyph_opt(
                        i,
                        j,
                        smax,
                        &self.buffers,
                        self.sub_rows,
                        self.sub_cols,
                        &self.shading,
                        &mut color,
                    );
                    match sc {
                        Some(s) => out.extend_from_slice(s.as_bytes()),
                        None => out.push(b' '),
                    }
                }
            } else {
                let mut prev_color = -1i32;
                for j in 0..aw {
                    let mut color = 0;
                    let sc = cell_glyph_opt(
                        i,
                        j,
                        smax,
                        &self.buffers,
                        self.sub_rows,
                        self.sub_cols,
                        &self.shading,
                        &mut color,
                    );
                    match sc {
                        None => {
                            if prev_color != -1 {
                                out.extend_from_slice(reset_seq);
                                prev_color = -1;
                            }
                            out.push(b' ');
                        }
                        Some(s) => {
                            if color != prev_color {
                                let cs: &[u8] = if self.logo.has_ansi && color > 0 && color < 128 {
                                    let entry = &mut ansi_cache[color as usize];
                                    if entry.is_none() {
                                        *entry = Some(format!("\u{1b}[1;{}m", color));
                                    }
                                    entry.as_ref().unwrap().as_bytes()
                                } else if color == 1 {
                                    self.color_inner.as_bytes()
                                } else {
                                    self.color_outer.as_bytes()
                                };
                                out.extend_from_slice(cs);
                                prev_color = color;
                            }
                            out.extend_from_slice(s.as_bytes());
                        }
                    }
                }
                if prev_color != -1 {
                    out.extend_from_slice(reset_seq);
                }
            }

            let fi = i as i32 - self.fetch_start;
            if !self.layout.layout_stacked && fi >= 0 {
                if let Some(line) = self.info_line(fi as usize) {
                    for _ in 0..render::GAP {
                        out.push(b' ');
                    }
                    let mut buf = [0u8; 512];
                    let n = emit_clipped(&mut buf, &line, self.layout.info_clip_cols);
                    out.extend_from_slice(&buf[..n]);
                }
            }

            out.extend_from_slice(clr_seq);
            out.push(b'\n');
        }

        if self.layout.layout_stacked && self.layout.stacked_info_rows > 0 {
            if self.layout.render_height > 0 {
                out.extend_from_slice(clr_seq);
                out.push(b'\n');
            }
            for i in 0..self.layout.stacked_info_rows {
                if let Some(line) = self.info_line(i as usize) {
                    let mut buf = [0u8; 512];
                    let n = emit_clipped(&mut buf, &line, self.layout.info_clip_cols);
                    out.extend_from_slice(&buf[..n]);
                }
                out.extend_from_slice(clr_seq);
                out.push(b'\n');
            }
        }

        out
    }
}

/// Format one info line exactly like `App::add_info`, including optional box
/// padding, so the background refresh thread produces identical output.
pub(crate) fn format_info_line(label_color: &str, label: &str, value: &str, box_width: i32) -> String {
    let line = format!("\u{1b}[1;{}m{}\u{1b}[0m: {}", label_color, label, value);
    if box_width > 0 {
        let w = visible_width(&line);
        let mut pad = box_width - w as i32;
        if pad < 0 {
            pad = 0;
        }
        let pad_str = " ".repeat(pad.max(0) as usize);
        return format!("\u{2502} {line}{pad_str} \u{2502}");
    }
    line
}

fn cell_glyph_opt<'a>(
    row: i32,
    col: i32,
    smax: i32,
    buffers: &render::Buffers,
    sub_rows: i32,
    sub_cols: i32,
    shading: &'a Shading,
    color_out: &mut i32,
) -> Option<&'a str> {
    render::cell_glyph(
        row, col, smax, buffers, sub_rows, sub_cols, shading, color_out,
    )
}
