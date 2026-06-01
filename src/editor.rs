use std::path::Path;
use std::process::Command;

/// Open `path` in the user's preferred editor (`$VISUAL` → `$EDITOR` → `vi`).
/// Redirects the editor's stdin/stdout to the real terminal so the editor
/// works correctly even when the caller is inside a pipeline.
/// Terminal settings are saved before and restored after the editor runs.
pub fn open(path: &Path) -> std::io::Result<std::process::ExitStatus> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;

        let tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")?;
        let fd = tty.as_raw_fd();

        // Save terminal settings before the editor runs.
        let saved = unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut t) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            t
        };

        let status = Command::new(&editor)
            .arg(path)
            .stdin(tty.try_clone()?)
            .stdout(tty)
            .status()?;

        // Restore terminal settings regardless of how the editor exited.
        unsafe {
            libc::tcsetattr(fd, libc::TCSANOW, &saved);
        }

        Ok(status)
    }

    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        let conin = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONIN$")?;
        let conout = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONOUT$")?;

        // Save console modes before the editor runs.
        let (saved_in, saved_out) = unsafe {
            let mut mode_in: u32 = 0;
            let mut mode_out: u32 = 0;
            winapi_get_console_mode(conin.as_raw_handle(), &mut mode_in);
            winapi_get_console_mode(conout.as_raw_handle(), &mut mode_out);
            (mode_in, mode_out)
        };

        let status = Command::new(&editor)
            .arg(path)
            .stdin(conin.try_clone()?)
            .stdout(conout.try_clone()?)
            .status()?;

        // Restore console modes.
        unsafe {
            winapi_set_console_mode(conin.as_raw_handle(), saved_in);
            winapi_set_console_mode(conout.as_raw_handle(), saved_out);
        }

        Ok(status)
    }

    #[cfg(not(any(unix, windows)))]
    {
        Command::new(&editor).arg(path).status()
    }
}

#[cfg(windows)]
unsafe fn winapi_get_console_mode(
    handle: *mut std::ffi::c_void,
    mode: &mut u32,
) {
    // SAFETY: linking against kernel32 which is always present on Windows.
    unsafe extern "system" {
        fn GetConsoleMode(
            hConsoleHandle: *mut std::ffi::c_void,
            lpMode: *mut u32,
        ) -> i32;
    }
    unsafe {
        GetConsoleMode(handle, mode);
    }
}

#[cfg(windows)]
unsafe fn winapi_set_console_mode(handle: *mut std::ffi::c_void, mode: u32) {
    unsafe extern "system" {
        fn SetConsoleMode(
            hConsoleHandle: *mut std::ffi::c_void,
            dwMode: u32,
        ) -> i32;
    }
    unsafe {
        SetConsoleMode(handle, mode);
    }
}
