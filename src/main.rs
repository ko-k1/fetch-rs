//! `fetch-rs`: a Rust port of the `fetch` system-info tool.
//! Rotating 3D logo + OS/host/kernel/CPU/GPU/memory/disk/IP/battery info.

mod ansi;
mod app;
mod config;
mod distro;
mod info;
mod logo;
mod points;
mod render;
mod runloop;
mod shading;
mod terminal;
mod util;

use std::{env, io::{self, Write}, process};

use render::{apply_layout, MAX_HEIGHT};
use shading::select_shading;
use terminal::cleanup_terminal;
#[cfg(unix)]
use terminal::{signal_handler, setup_raw_mode, winch_handler};

use app::App;
use runloop::run;

const FETCH_VERSION: &str = env!("CARGO_PKG_VERSION");
const FETCH_CODENAME: &str = "Overclocked ASCII";

fn print_help() {
    println!(
        "Usage: fetch [options]\n\n\
         Options:\
          -l, --logo <name>         Use a logo from fastfetch by name\n\
                             Any logo fastfetch supports works, e.g.:\
                               gentoo, arch, nixos, debian, ubuntu,\
                               fedora, void, alpine, opensuse, manjaro,\
                               proxmox, pop, linuxmint, endeavouros...\
                             Run 'fastfetch --list-logos' to see all\n\
         --rotate-x                Lock rotation to X axis only\n\
         --rotate-y                Lock rotation to Y axis only\n\
         -s, --speed <float>       Speed multiplier (default 1.0)\n\
         --size <float>            Scale the logo (e.g. 2.0 for double size)\n\
         --depth <float>           Scale the 3D depth (default 1.0)\n\
         --height <n>              Override render height in rows\n\
         --no-info                 Just the logo, no system info\n\
         --no-color                Disable logo coloring\n\
         --frames <n>              Stop after n frames (default: run until keypress)\n\
          --shading-mode <mode>     ascii (.,-~:;=!*#$@, the default), or opt into\
                             sub-cell blocks: sextants (2x3) or blocks (2x2)\n\
          --shading-chars <str>     Custom shading ramp, supports UTF-8\n\
          --box                     Draw a border box around the info block\n\
         -V, --version             Show version\n\
         -h, --help                Show this help\n\n\
         Config: ~/.config/fetch/config\n\
          List field names to show (in order), one per line.\n\
          Comment out or remove fields to hide them.\n\
          Available fields:\n\
            os, host, kernel, uptime, packages, shell, display, wm,\n\
            displaymanager, theme, icons, font, cursor, terminal, cpu,\n\
            gpu, memory, swap, disk, ip, battery, powerprofile, locale,\n\
            colors\n\n\
          Separators and custom fields:\n\
            ---                      Blank line separator\n\
            custom_Label=value       Static field (e.g. custom_Pronouns=he/him)\n\n\
          Extra disks:\n\
            disk=/home               Show additional mount point\n\
            disk=/data               (repeat for multiple mounts)\n\n\
          Settings:\n\
            label_color=<color>      Label color (red, green, yellow, blue,\
                                     magenta, cyan, white, or ANSI number)\n\
            separator=<char>         Title separator character\n\
            show_title=<0/1>         Show user@host and underline (default 1)\n\
            shading_mode=<mode>      ascii (default), blocks or sextants\n\
            shading=<str>            Shading ramp characters\n\
            light=<dir>              Light direction (top-left, top-right, top,\
                                     left, right, front, bottom-left, bottom-right)\n\
            spin=<axes>              Rotation axes (x, y, or xy)\n\
            speed=<float>            Rotation speed\n\
            size=<float>             Logo scale\n\
            height=<n>               Render height in rows\n\n\
            box=<0/1>                Draw a border box around the info block\n\n\
         Logo: ~/.config/fetch/logo.txt\n\
          Custom ASCII/Unicode logo. Add '# distro: <name>' as the\n\
          first line to set the color scheme.\n"    );
}

fn main() {
    let mut show_info = true;
    let mut use_color = true;
    let mut max_frames: i32 = 0;
    let mut speed: f32 = 1.0;
    let mut rotate_x: i32 = 1;
    let mut rotate_y: i32 = 1;
    let mut shading_cli: Option<String> = None;
    let mut shading_mode_cli: Option<String> = None;
    let mut box_flag = false;
    let mut logo_name: Option<String> = None;
    let mut speed_set = false;
    let mut height_set = false;
    let mut size_set = false;
    let mut depth_set = false;
    let mut rotation_set = false;
    let mut cli_height = 0;
    let mut cli_size = 1.0f32;
    let mut cli_depth = 1.0f32;

    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--version" | "-V" => {
                println!(
                    "fetch {FETCH_VERSION} \"{FETCH_CODENAME}\" ({}, {})",
                    env::consts::ARCH,
                    env::consts::OS
                );
                return;
            }
            "--logo" | "-l" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                logo_name = Some(args[i + 1].clone());
                i += 1;
            }
            "--rotate-x" => {
                rotation_set = true;
                rotate_x = 1;
                rotate_y = 0;
            }
            "--rotate-y" => {
                rotation_set = true;
                rotate_x = 0;
                rotate_y = 1;
            }
            "--speed" | "-s" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    return;
                }
                if !config::parse_number(a, &args[i + 1], -1000.0, 1000.0, &mut speed) {
                    process::exit(1);
                }
                speed_set = true;
                i += 1;
            }
            "--no-info" => show_info = false,
            "--no-color" => use_color = false,
            "--frames" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                if !config::parse_integer(a, &args[i + 1], 0, i32::MAX, &mut max_frames) {
                    process::exit(1);
                }
                i += 1;
            }
            "--shading-chars" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                shading_cli = Some(args[i + 1].clone());
                i += 1;
            }
            "--shading-mode" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                shading_mode_cli = Some(args[i + 1].clone());
                i += 1;
            }
            "--height" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                if !config::parse_integer(a, &args[i + 1], 1, MAX_HEIGHT, &mut cli_height) {
                    process::exit(1);
                }
                height_set = true;
                i += 1;
            }
            "--size" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                if !config::parse_number(a, &args[i + 1], 0.5, 5.0, &mut cli_size) {
                    process::exit(1);
                }
                size_set = true;
                i += 1;
            }
            "--depth" => {
                if i + 1 >= args.len() {
                    eprintln!("fetch: option '{a}' requires an argument");
                    process::exit(1);
                }
                if !config::parse_number(a, &args[i + 1], 0.1, 10.0, &mut cli_depth) {
                    process::exit(1);
                }
                depth_set = true;
                i += 1;
            }
            "--box" => box_flag = true,
            _ => {
                eprintln!("unknown option: {a}\nTry 'fetch --help'");
                process::exit(1);
            }
        }
        i += 1;
    }

    let home = env::var("HOME").unwrap_or_default();
    let mut config = config::load_config(&home);

    // CLI overrides (C applies them after load_config)
    if height_set {
        config.config_height = cli_height;
    }
    if size_set {
        config.size_scale = cli_size;
    }
    if depth_set {
        config.config_depth = cli_depth;
        config.depth_user_set = true;
    }
    if !speed_set && config.config_speed_set {
        speed = config.config_speed;
    }
    if config.config_spin_x >= 0 && !rotation_set {
        rotate_x = config.config_spin_x;
        rotate_y = config.config_spin_y;
    }
    if box_flag {
        config.config_box = true;
    }

    let mut app = App::new(
        show_info,
        use_color,
        max_frames,
        speed,
        rotate_x,
        rotate_y,
        config,
        &home,
        logo_name.as_deref(),
    );
    app.set_distro_colors();

    // Shading: CLI flags > config > ascii default
    if let Some(mode) = shading_mode_cli {
        app.config.config_shading_mode = mode;
    }
    if let Some(chars) = shading_cli {
        app.config.config_shading = chars;
    }
    let sub_rows = app.sub_rows as usize;
    let sub_cols = app.sub_cols as usize;
    let shading_chars = if app.config.config_shading.is_empty() {
        None
    } else {
        Some(app.config.config_shading.as_str())
    };
    if !select_shading(
        Some(&app.config.config_shading_mode),
        shading_chars,
        &mut (sub_rows, sub_cols),
        &mut app.shading,
    ) {
        select_shading(None, None, &mut (sub_rows, sub_cols), &mut app.shading);
    }
    app.sub_rows = sub_rows as i32;
    app.sub_cols = sub_cols as i32;

    app.gather_all();
    if app.config.config_box {
        app.box_wrap_lines();
    }
    apply_layout(
        &mut app.layout,
        app.show_info,
        app.fetch_lines.len() as i32,
        app.config.size_scale,
        app.config.config_height,
        &app.fetch_lines,
    );
    app.build_points_and_projection();
    app.tighten_render_height();

    #[cfg(unix)]
    {
        setup_raw_mode();
        unsafe {
            libc::signal(libc::SIGINT, signal_handler as *const () as usize);
            libc::signal(libc::SIGTERM, signal_handler as *const () as usize);
            libc::signal(libc::SIGWINCH, winch_handler as *const () as usize);
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    run(&mut app, &mut out);

    unsafe {
        cleanup_terminal();
    }
    let _ = out.write_all(b"\x1b[?25h");
    let _ = out.flush();
}
