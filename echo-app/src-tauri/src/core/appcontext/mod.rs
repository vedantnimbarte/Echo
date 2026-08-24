//! Which application currently has focus.
//!
//! Used to pick a per-app profile at injection time — deliberately *at
//! injection*, not at recording start, because the target app is whatever is
//! focused when the text lands.
//!
//! Returns a lowercased, stable-ish identifier per platform:
//! - Windows — the executable file name (`code.exe`)
//! - macOS   — the bundle identifier (`com.microsoft.vscode`)
//! - Linux   — the X11 window class (`code`)
//!
//! Every path returns `None` rather than an error when it can't tell. A missing
//! identifier just means "use the global settings", which is the same behaviour
//! as before per-app profiles existed.

/// The focused application's identifier, lowercased, or `None` if unavailable.
pub fn foreground_app() -> Option<String> {
    let raw = platform_foreground()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_lowercase())
}

#[cfg(target_os = "windows")]
fn platform_foreground() -> Option<String> {
    use std::os::windows::ffi::OsStringExt;

    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        // LIMITED_INFORMATION is enough for the image name and, unlike the full
        // query right, is granted for processes at other integrity levels.
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;

        let full = std::ffi::OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .to_string();

        // Keep just the file name: the full path varies by install location.
        std::path::Path::new(&full)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }
}

#[cfg(target_os = "macos")]
fn platform_foreground() -> Option<String> {
    // Shelling out to osascript rather than linking Cocoa: this runs once per
    // utterance, so the process cost is irrelevant next to transcription, and
    // it keeps AppKit out of the dependency tree.
    //
    // Note this needs Automation permission for System Events. Without it the
    // command fails and we fall back to global settings, which is the correct
    // degradation — no prompt is forced on users who never open this feature.
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get bundle identifier of \
             first application process whose frontmost is true",
        ])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "linux")]
fn platform_foreground() -> Option<String> {
    // X11 only. Wayland deliberately does not expose the focused window to
    // other clients, and there is no portal for it, so this returns None there
    // and per-app profiles fall back to the global settings.
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return None;
    }

    let out = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowclassname"])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the platform reports, the contract holds: either `None`, or a
    /// non-empty lowercased identifier with no surrounding whitespace. Callers
    /// compare it against stored `app_match` values, so a stray newline from a
    /// shelled-out command would silently break every profile.
    #[test]
    fn foreground_app_is_normalised_or_absent() {
        if let Some(app) = foreground_app() {
            assert!(!app.is_empty());
            assert_eq!(app, app.to_lowercase());
            assert_eq!(app, app.trim());
        }
    }
}
