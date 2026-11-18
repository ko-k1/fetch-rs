//! Terminal raw mode, signal handling, window-resize notification, and
//! low-level stdin reading.
//!
//! Unix-only behavior; every entry point has a no-op stub on other platforms
//! so the rest of the crate compiles everywhere.

use std::sync::atomic::AtomicBool;

/// Set to `true` by the `SIGWINCH` handler; consumed (and reset) by the
/// render loop so a terminal resize triggers a relayout.
pub static TERM_RESIZED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Unix implementation
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod imp {
    use super::*;
    use std::io::{self, Write};
    use std::sync::atomic::Ordering;
    use libc;

    /// Saved original terminal attributes, restored on exit.
    static mut ORIG_TERMIOS: Option<libc::termios> = None;
    static mut TERMIOS_SAVED: bool = false;

    pub fn get_term_size() -> (i32, i32) {
        use libc::{STDOUT_FILENO, TIOCGWINSZ, ioctl};
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) } == 0 {
            return (ws.ws_row as i32, ws.ws_col as i32);
        }
        (0, 0)
    }

    pub unsafe fn cleanup_terminal() {
        let saved = unsafe { std::ptr::addr_of!(ORIG_TERMIOS).read() };
        let was_saved = unsafe { TERMIOS_SAVED };
        if was_saved {
            if let Some(orig) = saved {
                unsafe {
                    let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &orig);
                }
            }
        }
        let _ = io::stdout().write_all(b"\x1b[?1002l\x1b[?1006l\x1b[?25h");
        let _ = io::stdout().flush();
    }

    pub extern "C" fn signal_handler(_sig: libc::c_int) {
        unsafe {
            cleanup_terminal();
            libc::_exit(0);
        }
    }

    pub extern "C" fn winch_handler(_sig: libc::c_int) {
        TERM_RESIZED.store(true, Ordering::SeqCst);
    }

    pub fn setup_raw_mode() {
        unsafe {
            let mut orig: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut orig) == 0 {
                ORIG_TERMIOS = Some(orig);
                TERMIOS_SAVED = true;
                let mut raw = orig;
                raw.c_lflag &= !(libc::ICANON | libc::ECHO);
                raw.c_cc[libc::VMIN] = 0;
                raw.c_cc[libc::VTIME] = 0;
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Non-Unix stubs
// ---------------------------------------------------------------------------

#[cfg(not(unix))]
mod imp {

    pub fn get_term_size() -> (i32, i32) {
        (0, 0)
    }    pub fn cleanup_terminal() {}

    pub extern "C" fn signal_handler(_sig: std::os::raw::c_int) {}

    pub extern "C" fn winch_handler(_sig: std::os::raw::c_int) {}

    pub fn setup_raw_mode() {}
}

pub use imp::{cleanup_terminal, get_term_size};
#[cfg(unix)]
pub use imp::{setup_raw_mode, signal_handler, winch_handler};
