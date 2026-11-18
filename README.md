# fetch-rs

**fetch-rs** is a Rust port of [`fetch`](https://github.com/areofyl/fetch): a
rotating 3D logo rendered in your terminal alongside your system info
(OS, host, kernel, CPU, GPU, memory, disk, IP, battery, and more).

Version `0.1.0` "Overclocked ASCII" · Edition 2024 · ISC License

## Features

- Rotating 3D ASCII/Unicode logo with per-cell shading and ANSI colors
- Logo spinning on X and/or Y axes, with adjustable speed, size, and depth
- Shading modes: `ascii` (default), `blocks` (2×2), `sextants` (2×3), or a
  custom UTF-8 shading ramp
- System info block: OS, host, kernel, uptime, packages, shell, display(s),
  WM, display manager, theme, icons, font, cursor, terminal, CPU, GPU,
  memory, swap, disk, IP, battery, power profile, locale, and ANSI colors
- Fastfetch logo support (`--logo <name>`) — any logo fastfetch ships works
- Custom logo via `~/.config/fetch/logo.txt` (prefix `# distro: <name>` to set
  the color scheme)
- Optional border box around the info block (fastfetch Caelestia style)
- Config file at `~/.config/fetch/config` with field ordering, custom fields,
  extra disks, alignment, label colors, light direction, and more
- Live refresh of fast-changing fields (uptime, memory, swap, battery) during
  the animation; press any key to stop, resize to relayout

## Installation

```sh
git clone <this-repo>
cd fetch-rs
cargo build --release
./target/release/fetch
```

Requires a Rust toolchain (edition 2024). On Linux, `libc` is used for
terminal size, signals, and system info; macOS is supported via `sysctl`.

## Usage

```
fetch [options]
```

| Option | Meaning |
|--------|---------|
| `-l, --logo <name>` | Use a fastfetch logo by name (e.g. `arch`, `nixos`, `ubuntu`, `gentoo`) |
| `--rotate-x` | Lock rotation to the X axis only |
| `--rotate-y` | Lock rotation to the Y axis only |
| `-s, --speed <float>` | Rotation speed multiplier (default `1.0`) |
| `--size <float>` | Logo scale (e.g. `2.0` for double size, range `0.5–5.0`) |
| `--depth <float>` | 3D depth scale (default `1.0`, range `0.1–10.0`) |
| `--height <n>` | Override render height in rows |
| `--no-info` | Just the logo, no system info block |
| `--no-color` | Disable logo coloring |
| `--frames <n>` | Stop after `n` frames (default: run until a key is pressed) |
| `--shading-mode <mode>` | `ascii` (default), `blocks`, or `sextants` |
| `--shading-chars <str>` | Custom shading ramp (supports UTF-8) |
| `--box` | Draw a border box around the info block |
| `-V, --version` | Show version |
| `-h, --help` | Show this help |

Config is read from `~/.config/fetch/config`; CLI flags override config values.

## Configuration

Create `~/.config/fetch/config`. List the fields to show, one per line, in the
order you want them. Comment out or remove fields to hide them.

```
# Fields (in order)
os
host
kernel
uptime
packages
shell
display
wm
theme
icons
font
cursor
terminal
cpu
gpu
memory
swap
disk
ip
battery
powerprofile
locale
colors

# Blank-line separator
---

# Custom static field
custom_Pronouns=he/him

# Extra disks
disk=/home
disk=/data

# Settings
label_color=cyan
separator=-
show_title=1
box=1
shading_mode=ascii
shading=.,-~:;=!*#$@
light=top-left
spin=xy
speed=1.0
size=1.0
depth=1.0
height=36
v_alignment=center
h_alignment=left
logo_outer=magenta
logo_inner=white
```

Available fields: `os`, `host`, `kernel`, `uptime`, `packages`, `shell`,
`display`, `wm`, `displaymanager`, `theme`, `icons`, `font`, `cursor`,
`terminal`, `cpu`, `gpu`, `memory`, `swap`, `disk`, `ip`, `battery`,
`powerprofile`, `locale`, `colors`.

### Custom logo

Place your ASCII/Unicode art at `~/.config/fetch/logo.txt`. If the first line
is `# distro: <name>`, that name sets the logo color scheme (e.g.
`# distro: arch`). Without a hint, the detected distro is used, falling back to
the built-in Gentoo logo when no logo can be loaded.

## Examples

```sh
# Default: rotating logo + full system info
fetch

# A specific distro logo, twice as big, no info block
fetch --logo nixos --size 2.0 --no-info

# Spin only on Y, slower, with a border box
fetch --rotate-y --speed 0.5 --box

# Run for 120 frames then exit
fetch --frames 120

# Custom shading ramp
fetch --shading-chars '█▓▒░ '
```

## Platform support

| Platform | Status | Info sources |
|----------|--------|--------------|
| **Linux** (primary) | Full | `/proc`, `/sys`, DRM, `libc` (uname/signals/raw mode) |
| **macOS** | Full | `sysctl` (model, CPU brand, memory, uptime), Apple Silicon GPU |
| **Windows** | Full | PowerShell `Get-CimInstance` (OS, host, CPU, GPU, memory, swap, disk, battery), `ver` (kernel), `winget` (packages), `ipconfig` (IP), `COMSPEC` (shell) |
| Other | Compiles | Best-effort / stubbed info |

Windows and macOS are first-class targets alongside Linux; the crate compiles and runs on all three.

## License

ISC License — see `LICENSE`.

---

Original `fetch` (C): https://github.com/areofyl/fetch
