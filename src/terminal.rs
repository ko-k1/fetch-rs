//! Terminal raw mode, signal handling, window-resize notification, and
//! low-level stdin reading.
//!
//! Unix uses termios/ioctl; Windows uses the Win32 console API; other
//! platforms get no-op stubs so the crate compiles everywhere.

use std::sync::atomic::AtomicBool;

/// Set to `true` by the resize handler; consumed (and reset) by the
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
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::io::{self, Write};
    use std::sync::atomic::Ordering;
    use std::os::raw::c_void;

    const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5; // -11
    const STD_INPUT_HANDLE: u32 = 0xFFFFFFF6;  // -12
    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
    const ENABLE_WINDOW_INPUT: u32 = 0x0008;

    #[repr(C)]
    struct COORD {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    struct SMALL_RECT {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }

    #[repr(C)]
    struct CONSOLE_SCREEN_BUFFER_INFO {
        dw_size: COORD,
        dw_cursor_position: COORD,
        w_attributes: u16,
        sr_window: SMALL_RECT,
        dw_maximum_window_size: COORD,
    }

    static mut ORIG_INPUT_MODE: u32 = 0;
    static mut ORIG_OUTPUT_MODE: u32 = 0;
    static mut MODE_SAVED: bool = false;

    unsafe extern "system" {
        fn GetStdHandle(n_std_handle: u32) -> *mut c_void;
        fn GetConsoleScreenBufferInfo(
            h_console_output: *mut c_void,
            lp_console_screen_buffer_info: *mut CONSOLE_SCREEN_BUFFER_INFO,
        ) -> i32;
        fn GetConsoleMode(h_console_handle: *mut c_void, lp_mode: *mut u32) -> i32;
        fn SetConsoleMode(h_console_handle: *mut c_void, dw_mode: u32) -> i32;
        fn SetConsoleCtrlHandler(
            handler: Option<extern "system" fn(ctrl_type: u32) -> i32>,
            add: i32,
        ) -> i32;
    }

    pub fn get_term_size() -> (i32, i32) {
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if h.is_null() {
                return (0, 0);
            }
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(h, &mut info) != 0 {
                let rows = info.sr_window.bottom - info.sr_window.top + 1;
                let cols = info.sr_window.right - info.sr_window.left + 1;
                if rows > 0 && cols > 0 {
                    return (rows as i32, cols as i32);
                }
            }
        }
        (0, 0)
    }

    pub fn setup_raw_mode() {
        unsafe {
            // --- Input: disable echo/line/processed input, disable Quick Edit
            // (clicking to select text pauses output on Windows), enable window
            // input so resize events are delivered ---
            let hin = GetStdHandle(STD_INPUT_HANDLE);
            if !hin.is_null() {
                let mut mode: u32 = 0;
                if GetConsoleMode(hin, &mut mode) != 0 {
                    ORIG_INPUT_MODE = mode;
                    let raw = mode
                        & !(ENABLE_ECHO_INPUT
                            | ENABLE_LINE_INPUT
                            | ENABLE_PROCESSED_INPUT
                            | ENABLE_QUICK_EDIT_MODE)
                        | ENABLE_WINDOW_INPUT;
                    let _ = SetConsoleMode(hin, raw);
                }
            }

            // --- Output: enable virtual terminal (ANSI) processing ---
            let hout = GetStdHandle(STD_OUTPUT_HANDLE);
            if !hout.is_null() {
                let mut mode: u32 = 0;
                if GetConsoleMode(hout, &mut mode) != 0 {
                    ORIG_OUTPUT_MODE = mode;
                    let vt = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
                    let _ = SetConsoleMode(hout, vt);
                }
            }

            MODE_SAVED = true;

            // Register Ctrl-C / close handler so the terminal is restored and
            // the process exits (matches Unix `_exit(0)` behaviour).
            let _ = SetConsoleCtrlHandler(Some(signal_handler), 1);
        }
    }

    pub unsafe fn cleanup_terminal() {
        unsafe {
            if MODE_SAVED {
                let hin = GetStdHandle(STD_INPUT_HANDLE);
                if !hin.is_null() {
                    let _ = SetConsoleMode(hin, ORIG_INPUT_MODE);
                }
                let hout = GetStdHandle(STD_OUTPUT_HANDLE);
                if !hout.is_null() {
                    let _ = SetConsoleMode(hout, ORIG_OUTPUT_MODE);
                }
            }
        }
        let _ = io::stdout().write_all(b"\x1b[?1002l\x1b[?1006l\x1b[?25h");
        let _ = io::stdout().flush();
    }

    pub extern "system" fn signal_handler(_ctrl_type: u32) -> i32 {
        unsafe {
            cleanup_terminal();
        }
        // Terminate immediately, matching Unix `_exit(0)`. Returning without
        // exiting would leave the program running with a restored (non-ANSI)
        // terminal, which makes the screen appear frozen.
        std::process::exit(0);
    }

    pub extern "system" fn winch_handler(_ctrl_type: u32) -> i32 {
        TERM_RESIZED.store(true, Ordering::SeqCst);
        1
    }
}

// ---------------------------------------------------------------------------
// Non-Unix, non-Windows stubs
// ---------------------------------------------------------------------------

#[cfg(not(any(unix, target_os = "windows")))]
mod imp {

    pub fn get_term_size() -> (i32, i32) {
        (0, 0)
    }

    pub fn cleanup_terminal() {}

    pub extern "C" fn signal_handler(_sig: std::os::raw::c_int) {}

    pub extern "C" fn winch_handler(_sig: std::os::raw::c_int) {}

    pub fn setup_raw_mode() {}
}

pub use imp::{cleanup_terminal, get_term_size};
#[cfg(any(unix, target_os = "windows"))]
pub use imp::{setup_raw_mode, signal_handler, winch_handler};
