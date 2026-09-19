/*!
 * SOURCE OF TRUTH KEYWORDS: get_setting, set_setting, list_settings,
 *   settings_snapshot, SettingsSnapshot, StoredSetting, validate_against_kind
 * WHAT:  Reading and writing settings, and handing the frontend the registry
 *        table it renders its controls from.
 * WHY:   `get_setting` now falls back to the REGISTRY's default rather than
 *        returning None and letting each caller invent one. That is the whole
 *        point of the table: before it, `whisper_model` was defaulted in three
 *        files and `asr_provider`'s `"local"` in four, and nothing made the
 *        copies agree.
 *
 *        Writes are validated against the setting's declared kind, so a value
 *        that cannot be parsed is rejected at the boundary instead of surfacing
 *        three layers down as an `unwrap_or` quietly substituting a default —
 *        which looks, from the user's side, exactly like the setting not
 *        working.
 *
 *        An UNKNOWN key is rejected outright. Echo stores settings in a plain
 *        key/value table, so before this a typo in a call site wrote a row that
 *        nothing would ever read and nothing would ever complain about.
 * WHERE: Called by the settings view through the generated bindings; the table
 *        it serves comes from registry/.
 */

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::{EchoError, Result};
use crate::ipc::factory::{execute, CommandSpec, Validate};
use crate::registry::{self, Capability, CapabilityKey, NavDef, SettingKind};
use crate::state::AppState;
use crate::storage::repositories;

/**
 * SOURCE OF TRUTH KEYWORDS: SettingsSnapshot
 * WHAT:  Everything the settings window needs to render itself: the capability
 *        table, and the value of every setting.
 * WHY:   One round trip rather than one per control. The window opens with
 *        forty-odd settings on it, and forty sequential IPC calls is a visible
 *        stagger as each control pops in with its real value.
 * WHERE: Produced by `settings_snapshot`; consumed by the settings view.
 */
#[derive(Debug, Serialize, Type)]
pub struct SettingsSnapshot {
    /// The registry table, verbatim. The frontend renders controls from this
    /// and holds no second copy of what the app has.
    pub capabilities: Vec<Capability>,
    /// Current value per key, already defaulted. Every key in the registry is
    /// present, so the frontend never has to decide what missing means.
    pub values: Vec<StoredSetting>,
    /// Nav entries in rail order, so the shell does not re-sort them.
    pub nav: Vec<NavEntry>,
}

#[derive(Debug, Serialize, Type)]
pub struct StoredSetting {
    pub key: String,
    pub value: String,
    /// False when the stored row is absent and this is the registry's default.
    /// The UI uses it to offer "reset" only where there is something to reset.
    pub is_set: bool,
}

#[derive(Debug, Serialize, Type)]
pub struct NavEntry {
    pub capability: CapabilityKey,
    pub nav: NavDef,
}

/// A setting write, validated against its own declared kind before it lands.
pub struct SettingWrite {
    pub key: String,
    pub value: String,
}

impl Validate for SettingWrite {
    fn validate(&self) -> std::result::Result<(), String> {
        let Some(def) = registry::setting_def(&self.key) else {
            // See the module WHY: an unknown key used to write a row nothing
            // would ever read.
            return Err(format!(
                "{} is not a setting this version of Echo has.",
                self.key
            ));
        };
        validate_against_kind(&def.kind, &self.value)
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: validate_against_kind
 * WHAT:  Whether a stored string is a legal value for a declared kind.
 * WHY:   Settings are stored as strings — that is the existing schema and
 *        changing it would reset every user's preferences — so the type check
 *        has to happen here rather than being carried by the column. Splitting
 *        it out of the Validate impl is what makes it table-testable.
 * WHERE: Called by SettingWrite::validate; tested below.
 */
pub fn validate_against_kind(kind: &SettingKind, value: &str) -> std::result::Result<(), String> {
    match kind {
        SettingKind::Toggle => {
            if value == "true" || value == "false" {
                Ok(())
            } else {
                Err(format!("{value:?} is not true or false."))
            }
        }
        SettingKind::Number { min, max, .. } => {
            let n: f64 = value
                .parse()
                .map_err(|_| format!("{value:?} is not a number."))?;
            if n < *min || n > *max {
                return Err(format!("{n} is outside {min} to {max}."));
            }
            Ok(())
        }
        SettingKind::Text { max_len, .. } => {
            if let Some(max) = max_len {
                if value.chars().count() > *max as usize {
                    return Err(format!("That is longer than {max} characters."));
                }
            }
            Ok(())
        }
        SettingKind::Choice { options } => {
            if options.iter().any(|o| o.value == value) {
                Ok(())
            } else {
                Err(format!("{value:?} is not one of the available options."))
            }
        }
        // Resolved at runtime, so the registry cannot know the legal set. An
        // empty value is always legal and means "let Echo choose".
        SettingKind::DynamicChoice { .. } => Ok(()),
        SettingKind::Hotkey => {
            if value.trim().is_empty() {
                Err("A hotkey cannot be empty.".into())
            } else {
                Ok(())
            }
        }
    }
}

/// Reads a setting, falling back to the registry's default.
///
/// Returns `None` only for a key the registry does not have — which is a caller
/// bug, not a missing value.
#[tauri::command]
#[specta::specta]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>> {
    let conn = state.db.lock().map_err(|_| poisoned())?;
    let stored = repositories::get_setting(&conn, &key)?;
    Ok(stored.or_else(|| registry::default_for(&key)))
}

/// The whole settings surface in one call. See SettingsSnapshot.
#[tauri::command]
#[specta::specta]
pub fn settings_snapshot(state: State<'_, AppState>) -> Result<SettingsSnapshot> {
    let conn = state.db.lock().map_err(|_| poisoned())?;

    let mut values = Vec::new();
    for def in registry::all_settings() {
        let stored = repositories::get_setting(&conn, &def.key)?;
        values.push(StoredSetting {
            key: def.key.clone(),
            is_set: stored.is_some(),
            value: stored.unwrap_or_else(|| def.default.to_stored()),
        });
    }

    Ok(SettingsSnapshot {
        capabilities: registry::capabilities().to_vec(),
        values,
        nav: registry::nav_items()
            .into_iter()
            .map(|(capability, nav)| NavEntry {
                capability: *capability,
                nav: nav.clone(),
            })
            .collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn set_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<()> {
    let spec = CommandSpec::new("set_setting", CapabilityKey::Settings).reports();
    let state = state.inner();

    execute(
        &state.exclusive,
        spec,
        SettingWrite { key, value },
        |write| async move {
            // Scoped: `tray::refresh` reads the same settings back, and the
            // lock is not reentrant.
            {
                let conn = state.db.lock().map_err(|_| poisoned())?;
                repositories::set_setting(&conn, &write.key, &write.value)?;
            }
            // The tray shows a tick beside the current language and microphone,
            // so a change made in this window has to reach it.
            if write.key == "language" || write.key == "audio_device" {
                crate::tray::refresh(&app);
            }
            Ok(())
        },
    )
    .await
}

fn poisoned() -> EchoError {
    EchoError::Storage(rusqlite::Error::InvalidQuery)
}

/// Language codes that spoken punctuation actually has rules for.
///
/// Surfaced in settings so the list is a fact the user can read, rather than
/// something they discover by dictating "coma" and being ignored.
#[tauri::command]
#[specta::specta]
pub fn spoken_punctuation_languages() -> Vec<&'static str> {
    crate::core::format::punctuation::supported_languages()
}

/// Language codes that number conversion has a parser for. Same reason as
/// [`spoken_punctuation_languages`].
#[tauri::command]
#[specta::specta]
pub fn number_languages() -> Vec<&'static str> {
    crate::core::format::numbers::supported_languages()
}

/// Language codes that filler and stutter cleanup has rules for. Same reason
/// as [`spoken_punctuation_languages`].
#[tauri::command]
#[specta::specta]
pub fn cleanup_languages() -> Vec<&'static str> {
    crate::core::format::cleanup::supported_languages()
}

/// The dictation languages the settings picker and the tray submenu both
/// render — one list, so the two cannot drift.
#[tauri::command]
#[specta::specta]
pub fn dictation_languages() -> &'static [crate::core::asr::languages::Language] {
    crate::core::asr::languages::LANGUAGES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::SettingChoice;

    #[test]
    fn a_toggle_takes_only_true_or_false() {
        assert!(validate_against_kind(&SettingKind::Toggle, "true").is_ok());
        assert!(validate_against_kind(&SettingKind::Toggle, "false").is_ok());
        // "1" and "yes" look reasonable and are exactly what a caller invents
        // when nothing rejects them; the read sites all compare against
        // "false", so "1" would silently read as ON.
        assert!(validate_against_kind(&SettingKind::Toggle, "1").is_err());
        assert!(validate_against_kind(&SettingKind::Toggle, "").is_err());
    }

    #[test]
    fn a_number_outside_its_range_is_refused() {
        let kind = SettingKind::Number {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: None,
        };
        assert!(validate_against_kind(&kind, "50").is_ok());
        assert!(validate_against_kind(&kind, "0").is_ok());
        assert!(validate_against_kind(&kind, "101").is_err());
        assert!(validate_against_kind(&kind, "-1").is_err());
        assert!(validate_against_kind(&kind, "lots").is_err());
    }

    #[test]
    fn a_choice_takes_only_its_own_options() {
        let kind = SettingKind::Choice {
            options: vec![
                SettingChoice::new("auto", "Auto"),
                SettingChoice::new("paste", "Paste"),
            ],
        };
        assert!(validate_against_kind(&kind, "auto").is_ok());
        assert!(validate_against_kind(&kind, "type").is_err());
    }

    /// The runtime-resolved kinds cannot be checked here, and pretending
    /// otherwise would reject a model the user has legitimately just installed.
    #[test]
    fn a_dynamic_choice_accepts_anything_including_empty() {
        let kind = SettingKind::DynamicChoice {
            source: crate::registry::ChoiceSource::WhisperModels,
        };
        assert!(validate_against_kind(&kind, "").is_ok());
        assert!(validate_against_kind(&kind, "large-v3").is_ok());
    }

    /// An unknown key used to write a row nothing would ever read. This is the
    /// check that turns that typo into an error the caller sees.
    #[test]
    fn an_unknown_key_is_refused() {
        let write = SettingWrite {
            key: "whisper_modle".into(),
            value: "base.en".into(),
        };
        assert!(write.validate().is_err());
    }

    #[test]
    fn a_known_key_with_a_legal_value_is_accepted() {
        let write = SettingWrite {
            key: "auto_inject".into(),
            value: "false".into(),
        };
        assert!(write.validate().is_ok());
    }
}
