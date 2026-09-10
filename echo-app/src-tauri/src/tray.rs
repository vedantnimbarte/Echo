//! The tray icon — the only persistent way back into Echo.
//!
//! The pill has no window chrome and sets `skipTaskbar`, and Settings starts
//! hidden after onboarding. So until now a user who dismissed the pill had no
//! route to Settings and no way to quit short of ending the process: the app was
//! running, listening on a global hotkey, and unreachable.
//!
//! The menu opens on a left click, which is the default on every platform and
//! the only affordance macOS offers anyway — so there is no per-OS branch here.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

/// Install the tray icon.
///
/// Errors are returned rather than logged so the caller decides: startup treats
/// a missing tray as degraded, not fatal, because a desktop that refuses to host
/// one (a bare CI container, a session with no status area) is still a desktop
/// Echo can dictate into.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "tray_settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit Echo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;

    let mut tray = TrayIconBuilder::with_id("echo")
        .tooltip("Echo — voice keyboard")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_settings" => {
                if let Some(win) = app.get_webview_window("main") {
                    // All three, in this order: a window can be hidden, or
                    // visible-but-minimised, or visible behind something else,
                    // and only the last of those is fixed by `set_focus` alone.
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            // The same exit path as the Quit button in Settings, so the
            // `RunEvent::Exit` handler still reaps the whisper-server child
            // rather than orphaning a few hundred megabytes of resident model.
            "tray_quit" => app.exit(0),
            _ => {}
        });

    // The bundled app icon, reused rather than a second asset to keep in step
    // with it. Absent only if `bundle.icon` is ever emptied, in which case a
    // tray with no icon is worse than none.
    match app.default_window_icon().cloned() {
        Some(icon) => tray = tray.icon(icon),
        None => {
            return Err(tauri::Error::Anyhow(anyhow::anyhow!(
                "no default window icon to use for the tray"
            )))
        }
    }

    tray.build(app)?;
    Ok(())
}
