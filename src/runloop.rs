//! Main render loop and input interpretation, extracted from `main.rs`.

use std::io::Write;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crate::app::App;
use crate::render::apply_layout;
use crate::terminal::{get_term_size, TERM_RESIZED};

// ---------------------------------------------------------------------------
// terminal raw input (unix)
// ---------------------------------------------------------------------------

/// Returns true if the main loop should break (key pressed).
#[cfg(unix)]
fn read_input(app: &mut App) -> bool {
    use libc::{FIONREAD, POLLIN, STDIN_FILENO, ioctl, poll, read};

    static mut IBUF: [u8; 128] = [0; 128];
    static mut IBUF_LEN: i32 = 0;

    let mut pfd = libc::pollfd {
        fd: STDIN_FILENO,
        events: POLLIN,
        revents: 0,
    };
    let mut should_break = false;

    while unsafe { poll(&mut pfd, 1, 0) } > 0 {
        if unsafe { IBUF_LEN } == 0 {
            let mut avail: i32 = 0;
            unsafe {
                ioctl(STDIN_FILENO, FIONREAD, &mut avail);
            }
            if avail <= 0 {
                break;
            }
            if avail == 1 {
                // Almost certainly a keypress: leave it for the shell.
                should_break = true;
                break;
            }
            let n = unsafe { read(STDIN_FILENO, std::ptr::addr_of_mut!(IBUF).cast::<libc::c_void>(), 128) };
            if n <= 0 {
                should_break = true;
                break;
            }
            unsafe {
                IBUF_LEN = n as i32;
            }
            if unsafe { IBUF[0] } != 0x1B {
                unsafe {
                    IBUF_LEN = 0;
                }
                should_break = true;
                break;
            }
        } else {
            let n = unsafe {
                read(
                    STDIN_FILENO,
                    std::ptr::addr_of_mut!(IBUF).cast::<libc::c_void>().add(IBUF_LEN as usize),
                    128 - IBUF_LEN as usize,
                )
            };
            if n > 0 {
                unsafe {
                    IBUF_LEN += n as i32;
                }
            }
        }

        let mut i = 0;
        while i < unsafe { IBUF_LEN } {
            if unsafe { IBUF[i as usize] } == 0x1B {
                if i + 2 >= unsafe { IBUF_LEN } {
                    break;
                }
                if unsafe { IBUF[i as usize + 1] } == b'['
                    && unsafe { IBUF[i as usize + 2] } == b'<'
                {
                    let mut j = i + 3;
                    let mut btn = 0i32;
                    let mut mx = 0i32;
                    let mut my = 0i32;
                    while j < unsafe { IBUF_LEN } && unsafe { IBUF[j as usize] }.is_ascii_digit() {
                        btn = btn * 10 + (unsafe { IBUF[j as usize] } - b'0') as i32;
                        j += 1;
                    }
                    if j >= unsafe { IBUF_LEN } {
                        break;
                    }
                    if unsafe { IBUF[j as usize] } == b';' {
                        j += 1;
                    }
                    while j < unsafe { IBUF_LEN } && unsafe { IBUF[j as usize] }.is_ascii_digit() {
                        mx = mx * 10 + (unsafe { IBUF[j as usize] } - b'0') as i32;
                        j += 1;
                    }
                    if j >= unsafe { IBUF_LEN } {
                        break;
                    }
                    if unsafe { IBUF[j as usize] } == b';' {
                        j += 1;
                    }
                    while j < unsafe { IBUF_LEN } && unsafe { IBUF[j as usize] }.is_ascii_digit() {
                        my = my * 10 + (unsafe { IBUF[j as usize] } - b'0') as i32;
                        j += 1;
                    }
                    if j >= unsafe { IBUF_LEN } {
                        break;
                    }
                    let trail = unsafe { IBUF[j as usize] };
                    j += 1;
                    if trail != b'M' && trail != b'm' {
                        i = j;
                        continue;
                    }
                    if btn == 0 && trail == b'M' {
                        app.mouse_dragging = true;
                        app.mouse_last_x = mx;
                        app.mouse_last_y = my;
                        app.drag_vx = 0.0;
                        app.drag_vy = 0.0;
                    } else if btn == 32 && trail == b'M' && app.mouse_dragging {
                        let dx = mx - app.mouse_last_x;
                        let dy = my - app.mouse_last_y;
                        app.drag_vy = -dx as f32 * 0.03;
                        app.drag_vx = -dy as f32 * 0.03;
                        app.a += app.drag_vx;
                        app.b += app.drag_vy;
                        app.mouse_last_x = mx;
                        app.mouse_last_y = my;
                    } else if btn == 0 && trail == b'm' {
                        app.mouse_dragging = false;
                    }
                    i = j;
                } else {
                    i += 1;
                    while i < unsafe { IBUF_LEN } && unsafe { IBUF[i as usize] } < 0x40 {
                        i += 1;
                    }
                    if i < unsafe { IBUF_LEN } {
                        i += 1;
                    }
                }
            } else {
                should_break = true;
                break;
            }
        }

        if i > 0 && i < unsafe { IBUF_LEN } {
            for k in i..unsafe { IBUF_LEN } {
                unsafe {
                    IBUF[(k - i) as usize] = IBUF[k as usize];
                }
            }
            unsafe {
                IBUF_LEN -= i;
            }
        } else if i >= unsafe { IBUF_LEN } {
            unsafe {
                IBUF_LEN = 0;
            }
        }
        if should_break {
            break;
        }
    }
    should_break
}

#[cfg(not(unix))]
fn read_input(_app: &mut App) -> bool {
    false
}

// ---------------------------------------------------------------------------
// render loop
// ---------------------------------------------------------------------------

pub fn run(app: &mut App, out: &mut impl Write) {
    let _ = out.write_all(b"\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[2J");
    let _ = out.flush();

    let mut frame = 0;
    while app.max_frames == 0 || frame < app.max_frames {
        if read_input(app) {
            break;
        }
        if TERM_RESIZED.swap(false, Ordering::SeqCst) {
            let (tr, tc) = get_term_size();
            app.term_rows = tr;
            app.term_cols = tc;
            let old = (
                app.layout.render_height,
                app.layout.anim_width,
                app.layout.layout_stacked,
                app.layout.info_clip_cols,
            );
            apply_layout(
                &mut app.layout,
                app.show_info,
                app.fetch_lines.len() as i32,
                app.config.size_scale,
                app.config.config_height,
                &app.fetch_lines,
            );
            let new = (
                app.layout.render_height,
                app.layout.anim_width,
                app.layout.layout_stacked,
                app.layout.info_clip_cols,
            );
            if old != new {
                app.build_points_and_projection();
                let _ = out.write_all(b"\x1b[2J");
            }
        }
        if app.show_info && frame > 0 && frame % 20 == 0 {
            app.refresh_pass();
        }
        if !app.mouse_dragging {
            if app.drag_vx != 0.0 || app.drag_vy != 0.0 {
                app.a += app.drag_vx;
                app.b += app.drag_vy;
            } else {
                if app.rotate_x != 0 {
                    app.a += 0.04 * app.speed;
                }
                if app.rotate_y != 0 {
                    app.b += 0.06 * app.speed;
                }
            }
        }

        let frame_bytes = app.render_frame();
        let _ = out.write_all(&frame_bytes);
        let _ = out.flush();
        thread::sleep(Duration::from_millis(50));
        frame += 1;
    }
}
