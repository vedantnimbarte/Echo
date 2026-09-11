//! The tray icon — the only persistent way back into Echo.
//!
//! The pill has no window chrome and sets `skipTaskbar`, and Settings starts
//! hidden after onboarding. So until now a user who dismissed the pill had no
//! route to Settings and no way to quit short of ending the process: the app was
//! running, listening on a global hotkey, and unreachable.
//!
//! The menu opens on a left click, which is the default on every platform and
//! the only affordance macOS offers anyway — so there is no per-OS branch here.
//!
//! It also carries the two settings worth changing without opening a window:
//! dictation language and microphone. Both are a tick beside the live value
//! rather than a bare list, so the menu answers "what is Echo using?" as well as
//! changing it — which means it is rebuilt, not built once (see [`refresh`]).
//!
//! **Never rebuild from a click event.** The shell posts the tray's button-down
//! and button-up messages back to back, so tao gets no turn between them: by the
//! time our click handler runs, `TrackPopupMenu` is already up and pumping the
//! queue, and the handler runs *inside* that modal loop. Swapping the menu there
//! destroys the `HMENU` the popup is tracking and it vanishes on the frame it
//! appeared. `Enter` — the cursor arriving on the icon — is the hook that works:
//! it lands a whole gesture before the press, outside any modal loop.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::core::asr::languages::LANGUAGES;
use crate::core::audio::AudioDevice;
use crate::state::AppState;
use crate::storage::repositories;

/// Menu ids carry their value after the prefix, rather than an index into a
/// device list that is enumerated afresh on every hover.
const LANG_PREFIX: &str = "tray_lang:";
const MIC_PREFIX: &str = "tray_mic:";

/// The empty `audio_device` setting means "whatever the OS calls default", so
/// it needs a row of its own — an empty-named device is not the same thing.
const SYSTEM_DEFAULT: &str = "";

/// Which setting a menu id means, and what to set it to. Pure so the two
/// prefixes can be tested without a desktop to hang a tray icon on.
fn route(id: &str) -> Option<(&'static str, &str)> {
    if let Some(code) = id.strip_prefix(LANG_PREFIX) {
        Some(("language", code))
    } else {
        id.strip_prefix(MIC_PREFIX)
            .map(|name| ("audio_device", name))
    }
}

/// Where the tick goes. Absent or empty resolves to auto downstream
/// (`whisper_cli::resolve_language`), so the menu has to agree.
fn active_language(raw: &str) -> &str {
    if raw.is_empty() {
        "auto"
    } else {
        raw
    }
}

fn setting<R: Runtime>(app: &AppHandle<R>, key: &str) -> String {
    let state = app.state::<AppState>();
    let conn = state.db.lock().unwrap();
    repositories::get_setting(&conn, key)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Everything the menu draws itself from.
struct MenuState {
    /// Already resolved through [`active_language`], so it is what gets ticked.
    language: String,
    device: String,
    devices: Vec<AudioDevice>,
}

impl MenuState {
    fn read<R: Runtime>(app: &AppHandle<R>) -> Self {
        // Both settings reads take the database lock and give it back before
        // the enumeration below, which is the slow part.
        let language = active_language(&setting(app, "language")).to_string();
        let device = setting(app, "audio_device");
        // An enumeration failure yields an empty list, leaving just the System
        // default row — still a usable menu.
        let devices = app
            .state::<AppState>()
            .audio
            .list_input_devices()
            .unwrap_or_default();
        Self {
            language,
            device,
            devices,
        }
    }

    /// Every string the menu renders, in one comparable value.
    ///
    /// This is what makes hovering cheap *and* safe: nothing changed means no
    /// `set_menu`, so the live menu object is left alone. The separator is a
    /// control character because a device name can hold anything a driver
    /// author liked, including the punctuation an obvious separator would use.
    fn fingerprint(&self) -> String {
        const SEP: char = '\u{1}';
        let mut s = self.language.clone();
        s.push(SEP);
        s.push_str(&self.device);
        for d in &self.devices {
            // The flag goes before the name, not after: appended, it reads the
            // same as the separator starting the next row, so a renamed device
            // could fingerprint as an unchanged list.
            s.push(SEP);
            s.push(if d.is_default { '*' } else { '-' });
            s.push_str(&d.name);
        }
        s
    }
}

/// Fingerprint of the menu currently attached to the tray.
static SHOWN: Mutex<String> = Mutex::new(String::new());

fn build_menu<R: Runtime>(app: &AppHandle<R>, state: &MenuState) -> tauri::Result<Menu<R>> {
    let MenuState {
        language,
        device,
        devices,
    } = state;

    let langs: Vec<CheckMenuItem<R>> = LANGUAGES
        .iter()
        .map(|l| {
            CheckMenuItem::with_id(
                app,
                format!("{LANG_PREFIX}{}", l.code),
                l.label,
                true,
                l.code == language.as_str(),
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let lang_menu = Submenu::with_items(
        app,
        "Language",
        true,
        &langs
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
            .collect::<Vec<_>>(),
    )?;

    let mut mics = vec![CheckMenuItem::with_id(
        app,
        format!("{MIC_PREFIX}{SYSTEM_DEFAULT}"),
        "System default",
        true,
        device.is_empty(),
        None::<&str>,
    )?];
    for d in devices {
        mics.push(CheckMenuItem::with_id(
            app,
            format!("{MIC_PREFIX}{}", d.name),
            if d.is_default {
                format!("{} (default)", d.name)
            } else {
                d.name.clone()
            },
            true,
            &d.name == device,
            None::<&str>,
        )?);
    }
    let mic_menu = Submenu::with_items(
        app,
        "Microphone",
        true,
        &mics
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
            .collect::<Vec<_>>(),
    )?;

    let open = MenuItem::with_id(app, "tray_settings", "Open Echo", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "tray_update", "Check for Updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit Echo", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &lang_menu,
            &mic_menu,
            &PredefinedMenuItem::separator(app)?,
            &update,
            &quit,
        ],
    )
}

/// Rebuild the menu in place, so the ticks and the device list match reality.
///
/// A no-op when nothing it renders has changed. That is not only an
/// optimisation: it means the ordinary hover-then-click never touches the menu
/// object at all, so a rebuild that somehow arrived late could not pull the
/// popup out from under the user.
///
/// Public because `set_setting` calls it: a language picked in the settings
/// window has to move the tick in the tray too.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id("echo") else {
        return; // No tray on this desktop; startup already warned.
    };
    let state = MenuState::read(app);
    let fingerprint = state.fingerprint();
    if *SHOWN.lock().unwrap() == fingerprint {
        return;
    }
    match build_menu(app, &state) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
            *SHOWN.lock().unwrap() = fingerprint;
        }
        Err(e) => tracing::warn!("Could not rebuild the tray menu: {e}"),
    }
}

/// Bring the settings window back, wherever it was left.
fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        // All three, in this order: a window can be hidden, or
        // visible-but-minimised, or visible behind something else, and only the
        // last of those is fixed by `set_focus` alone.
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Persist a choice made from the tray and tell everyone who is showing it.
fn choose<R: Runtime>(app: &AppHandle<R>, key: &str, value: &str) {
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        if let Err(e) = repositories::set_setting(&conn, key, value) {
            tracing::warn!("Could not save {key} from the tray: {e}");
            return;
        }
    }
    // The settings window is a separate webview holding its own cached copy of
    // this value, and nothing polls — so it is told, the way pill size is.
    let _ = app.emit("echo://setting-changed", key);
    refresh(app);
}

/// Install the tray icon.
///
/// Errors are returned rather than logged so the caller decides: startup treats
/// a missing tray as degraded, not fatal, because a desktop that refuses to host
/// one (a bare CI container, a session with no status area) is still a desktop
/// Echo can dictate into.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let state = MenuState::read(app);
    *SHOWN.lock().unwrap() = state.fingerprint();

    let mut tray = TrayIconBuilder::with_id("echo")
        .tooltip("Echo — voice keyboard")
        .menu(&build_menu(app, &state)?)
        .on_tray_icon_event(|tray, event| {
            // The cursor arriving on the icon, not the click — see the note at
            // the top of this file. One device query per hover, and the menu is
            // already correct by the time the button goes down.
            //
            // ponytail: a click with no hover first (automation, or a desktop
            // that reports no motion over the icon) opens the previous list.
            // Fix by driving the popup ourselves if Tauri ever exposes
            // `TrayIcon::show_menu`.
            if let TrayIconEvent::Enter { .. } = event {
                refresh(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_settings" => show_main(app),
            // The updater is a frontend plugin, so the window does the work.
            // Shown first, and unconditionally: every answer this can give —
            // "up to date", "here is 0.4.0", "that failed" — is a dialog the
            // user has to be looking at the app to see.
            "tray_update" => {
                show_main(app);
                let _ = app.emit("echo://check-for-updates", ());
            }
            // The same exit path as the Quit button in Settings, so the
            // `RunEvent::Exit` handler still reaps the whisper-server child
            // rather than orphaning a few hundred megabytes of resident model.
            "tray_quit" => app.exit(0),
            id => {
                if let Some((key, value)) = route(id) {
                    choose(app, key, value);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_to_the_setting_they_came_from() {
        for l in LANGUAGES {
            assert_eq!(
                route(&format!("{LANG_PREFIX}{}", l.code)),
                Some(("language", l.code))
            );
        }
        // The System default row carries an empty name on purpose: that is what
        // the setting stores for "whatever the OS picks".
        assert_eq!(
            route(&format!("{MIC_PREFIX}{SYSTEM_DEFAULT}")),
            Some(("audio_device", ""))
        );
        // Device names are arbitrary, including ones with a colon in them.
        assert_eq!(
            route(&format!("{MIC_PREFIX}Headset: Mic (2- USB)")),
            Some(("audio_device", "Headset: Mic (2- USB)"))
        );
        assert_eq!(route("tray_quit"), None);
        assert_eq!(route("tray_settings"), None);
        assert_eq!(route("tray_update"), None);
    }

    fn state(language: &str, device: &str, devices: &[(&str, bool)]) -> MenuState {
        MenuState {
            language: active_language(language).to_string(),
            device: device.to_string(),
            devices: devices
                .iter()
                .map(|(name, is_default)| AudioDevice {
                    name: name.to_string(),
                    is_default: *is_default,
                })
                .collect(),
        }
    }

    #[test]
    fn the_fingerprint_moves_with_everything_the_menu_draws() {
        let base = state("en", "", &[("Mic A", true)]);
        // A hover that changes nothing must compare equal, or every hover would
        // swap the menu object the popup is about to open.
        assert_eq!(base.fingerprint(), state("en", "", &[("Mic A", true)]).fingerprint());

        for changed in [
            state("de", "", &[("Mic A", true)]),                   // tick moved
            state("en", "Mic A", &[("Mic A", true)]),              // device chosen
            state("en", "", &[("Mic A", false)]),                  // OS default moved
            state("en", "", &[("Mic A", true), ("Mic B", false)]), // plugged in
            state("en", "", &[]),                                  // unplugged
        ] {
            assert_ne!(base.fingerprint(), changed.fingerprint());
        }
    }

    #[test]
    fn a_device_name_cannot_forge_another_row() {
        // Names come from drivers, so the separator has to be one they cannot
        // put in a name — otherwise a rename could read as a different list.
        assert_ne!(
            state("en", "", &[("Mic A", true)]).fingerprint(),
            state("en", "", &[("Mic A", false), ("", false)]).fingerprint(),
        );
    }

    #[test]
    fn an_unset_language_ticks_auto() {
        assert_eq!(active_language(""), "auto");
        assert_eq!(active_language("auto"), "auto");
        assert_eq!(active_language("hi"), "hi");
        // Every code the menu can tick must be one the settings list offers,
        // or a language set elsewhere would show no tick at all.
        assert!(LANGUAGES.iter().any(|l| l.code == "auto"));
    }
}
