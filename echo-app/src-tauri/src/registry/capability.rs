/*!
 * SOURCE OF TRUTH KEYWORDS: Capability, CapabilityKey, SettingDef, NavDef,
 *   HotkeyDef, MetricDef, SettingSection, SettingKind, SettingValue,
 *   SettingChoice, ChoiceSource, OsPermission, LatencyStage
 * WHAT:  The shape of one registry entry — everything Echo knows about a
 *        feature, in one struct.
 * WHY:   This is the type that makes "adding a feature is an entry, not a
 *        pattern" true. One entry declares the settings the feature owns, the
 *        OS permissions it needs, where it sits in the window, which hotkey it
 *        binds and which latency stages it emits — and the settings UI, the
 *        command factory's preflight and the stats queries all read it rather
 *        than each keeping their own list.
 *
 *        Echo needs this more than the app it is ported from did. Before it,
 *        `whisper_model` was read in three places with three separately
 *        written defaults, `asr_provider`'s default `"local"` was spelled out
 *        at four call sites, and the settings panel held its own 1361-line
 *        idea of what the app has. Six lists that must agree is how a codebase
 *        develops features that are half-wired.
 * WHERE: Instantiated in registry/mod.rs; consumed by ipc/factory.rs, the
 *        settings commands, and mirrored to TypeScript by specta.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/**
 * SOURCE OF TRUTH KEYWORDS: CapabilityKey
 * WHAT:  The closed set of features Echo has.
 * WHY:   An enum rather than a string, so a typo is a build error and a new
 *        feature forces every exhaustive match to be revisited deliberately.
 * WHERE: The identity of every registry entry; carried on every IPC command so
 *        the factory knows which entry to preflight against.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityKey {
    /// The core loop: hotkey, record, transcribe, deliver.
    Dictation,
    History,
    Insights,
    Dictionary,
    Models,
    Providers,
    Plugins,
    Profiles,
    WakeWord,
    Output,
    Privacy,
    Settings,
    Onboarding,
    Updates,
}

impl CapabilityKey {
    /// Stable string form, used in tracing spans and metric rows.
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityKey::Dictation => "dictation",
            CapabilityKey::History => "history",
            CapabilityKey::Insights => "insights",
            CapabilityKey::Dictionary => "dictionary",
            CapabilityKey::Models => "models",
            CapabilityKey::Providers => "providers",
            CapabilityKey::Plugins => "plugins",
            CapabilityKey::Profiles => "profiles",
            CapabilityKey::WakeWord => "wake_word",
            CapabilityKey::Output => "output",
            CapabilityKey::Privacy => "privacy",
            CapabilityKey::Settings => "settings",
            CapabilityKey::Onboarding => "onboarding",
            CapabilityKey::Updates => "updates",
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: OsPermission
 * WHAT:  An operating-system grant a feature cannot work without.
 * WHY:   Declared per capability so the command factory can turn a missing
 *        grant into a typed, actionable error before a handler runs, rather
 *        than each adapter discovering it separately and failing in its own
 *        dialect. Echo already asks the OS these two questions; what it did
 *        not have was one name for the answer.
 * WHERE: On Capability::requires and SettingDef::requires_permission; checked
 *        by ipc/factory.rs::preflight_permissions.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OsPermission {
    /// Recording. Without it there is no audio at all.
    Microphone,
    /// Typing into another app, and reading which app is focused. Without it
    /// Echo still transcribes and still reaches the clipboard.
    Accessibility,
}

impl OsPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            OsPermission::Microphone => "microphone",
            OsPermission::Accessibility => "accessibility",
        }
    }
}

// LatencyStage lives in core::telemetry::latency, not here. The registry
// DECLARES which stages a capability emits; the stages themselves are a fact
// about the pipeline, and core sits below this layer — see layering.rs.
pub use crate::core::telemetry::latency::LatencyStage;

/**
 * SOURCE OF TRUTH KEYWORDS: SettingSection
 * WHAT:  Which page and section of the settings window a setting renders in.
 * WHY:   Grouping is presentation, but it belongs to the DECLARATION. A setting
 *        that does not say where it goes forces the settings view to keep its
 *        own ordering list, which is the exact duplication the registry exists
 *        to remove — and that list is the thing that silently goes stale when a
 *        setting is added.
 *
 *        The variants are page-and-section rather than a coarse topic, because
 *        the window is split that way: four pages, each with its own tabs, and
 *        a setting placed only to the page would still need a second list to
 *        decide its tab. Written as `PageSection` in one name so a setting
 *        declares its whole home in one value.
 *
 *        `*Advanced` sections are for settings that exist to unstick a specific
 *        machine. That is a different question from `SettingDef::advanced`,
 *        which hides a row behind a disclosure within whatever section it is
 *        in; a setting can be in an Advanced section without being hidden, and
 *        usually is.
 * WHERE: On every SettingDef; read by the settings view to place its controls.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettingSection {
    SettingsGeneral,
    SettingsDictation,
    SettingsMicrophone,
    EngineSpeech,
    /// Holds no settings — only the file-transcription and crash-recovery
    /// blocks. A section can be all EXTRAS: audio a crash interrupted is
    /// something nobody would think to look for on a tab they did not know
    /// existed, so it gets a name of its own rather than being tucked under
    /// Advanced.
    EngineTools,
    EngineAdvanced,
    OutputInsert,
    OutputFormatting,
    OutputApps,
    OutputAdvanced,
    Privacy,
}

impl SettingSection {
    /// Which settings page this section belongs to. The page ids are the ones
    /// the frontend router already uses.
    pub fn page(&self) -> &'static str {
        match self {
            SettingSection::SettingsGeneral
            | SettingSection::SettingsDictation
            | SettingSection::SettingsMicrophone => "settings",
            SettingSection::EngineSpeech
            | SettingSection::EngineTools
            | SettingSection::EngineAdvanced => "engine",
            SettingSection::OutputInsert
            | SettingSection::OutputFormatting
            | SettingSection::OutputApps
            | SettingSection::OutputAdvanced => "output",
            SettingSection::Privacy => "privacy",
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: ChoiceSource
 * WHAT:  Where a dynamic setting's options come from.
 * WHY:   Input devices, installed models, cloud providers and supported
 *        languages cannot be listed in a static declaration — they depend on
 *        the machine, on what has been downloaded, and on which engine is
 *        selected. Naming the SOURCE keeps the setting declarative anyway: the
 *        registry still says what the control is, and the frontend resolves the
 *        options, so adding a device picker stays a registry entry rather than
 *        a bespoke form.
 * WHERE: Carried by SettingKind::DynamicChoice; resolved by the SettingControl
 *        component through the matching query command.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChoiceSource {
    InputDevices,
    WhisperModels,
    NemoModels,
    CloudProviders,
    /// Languages the SELECTED engine declares support for — which is what stops
    /// the UI offering a language on an engine that cannot speak it.
    Languages,
    /// The languages a formatting stage actually has rules for, so "which
    /// languages does this cover" is a fact to read rather than one to discover
    /// by being ignored.
    PunctuationLanguages,
    NumberLanguages,
    CleanupLanguages,
    WakeWordModels,
    Profiles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SettingChoice {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SettingChoice {
    pub fn new(value: &str, label: &str) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            description: None,
        }
    }

    pub fn described(value: &str, label: &str, description: &str) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            description: Some(description.to_string()),
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: SettingKind
 * WHAT:  How a setting renders, and what values it will accept.
 * WHERE: On every SettingDef; the settings view switches its control on this
 *        and on nothing else.
 */
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "type")]
pub enum SettingKind {
    Toggle,
    Text {
        placeholder: Option<String>,
        max_len: Option<u32>,
    },
    Number {
        min: f64,
        max: f64,
        step: f64,
        unit: Option<String>,
    },
    Choice {
        options: Vec<SettingChoice>,
    },
    /// A choice whose options are only knowable at runtime.
    DynamicChoice {
        source: ChoiceSource,
    },
    Hotkey,
}

/**
 * SOURCE OF TRUTH KEYWORDS: SettingValue
 * WHAT:  A stored setting value, closed over the kinds above.
 * WHY:   Validating a value against its declared kind is only possible because
 *        both are typed. Echo stores settings as strings in SQLite and will
 *        keep doing so — this type is the shape they are validated as on the
 *        way in and handed to the UI as, so "true" for a Number is rejected at
 *        the boundary rather than surfacing as an `unwrap_or` three layers
 *        down quietly substituting a default.
 * WHERE: Produced and validated by services/settings.rs against the registry's
 *        SettingDef before a write is accepted.
 */
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "type", content = "value")]
pub enum SettingValue {
    Bool(bool),
    Text(String),
    Number(f64),
}

impl SettingValue {
    /// The string form Echo's settings table stores.
    ///
    /// Booleans are `"true"`/`"false"` and numbers are plain decimal, because
    /// that is what every existing row already holds — a new encoding here
    /// would silently reset every setting on first launch after the upgrade.
    pub fn to_stored(&self) -> String {
        match self {
            SettingValue::Bool(b) => b.to_string(),
            SettingValue::Text(s) => s.clone(),
            SettingValue::Number(n) => {
                // Integers store without a trailing ".0": the existing rows are
                // written by `to_string()` on an integer type, and a parser
                // expecting `2` must not start receiving `2.0`.
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: VisibleWhen
 * WHAT:  A condition on ANOTHER setting that decides whether this one is shown.
 * WHY:   Some controls only mean anything for certain values of another. The
 *        clipboard hold is the worked example: it says how long to let the
 *        target app read the clipboard, and Echo only borrows the clipboard
 *        when it is pasting — so on "always type" the control is there,
 *        adjustable, and governs nothing.
 *
 *        Declared rather than coded in the view, for the same reason placement
 *        is: a condition living in the frontend is a second place that has to
 *        learn about every new setting, and the one that silently goes stale.
 *
 *        Deliberately only "this key is one of these values" — not an
 *        expression language. Every condition the app actually has fits, and a
 *        richer form would invite logic into a table that other layers have to
 *        be able to read without evaluating anything.
 * WHERE: On SettingDef::visible_when; honoured by the settings view.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VisibleWhen {
    /// The setting this one depends on.
    pub key: String,
    /// Values of that setting for which this control is shown.
    pub any_of: Vec<String>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: SettingDef
 * WHAT:  One setting, fully described: how it renders, what it defaults to, and
 *        what it requires to be usable.
 * WHY:   `default` is typed as SettingValue and validated against `kind` at
 *        startup, so a mismatch is caught the first time the app runs rather
 *        than the first time a user opens that pane.
 *
 *        `requires_permission` is what lets settings explain that a toggle
 *        cannot do anything instead of offering one that silently fails. A
 *        toggle that is ON while the permission behind it is missing is a
 *        control that lies: the user has said yes, the app agrees, and nothing
 *        happens.
 * WHERE: Declared in registry/mod.rs; rendered by the SettingControl component;
 *        enforced on write by the settings service.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SettingDef {
    /// Globally unique, stable. This is the database key — renaming one
    /// silently resets that setting for every existing user.
    pub key: String,
    pub label: String,
    pub description: String,
    pub section: SettingSection,
    pub kind: SettingKind,
    pub default: SettingValue,
    /// OS grants without which this setting cannot do anything, whatever its
    /// value.
    pub requires_permission: Vec<OsPermission>,
    /// Hidden behind a disclosure. For settings that exist to unstick a
    /// specific machine, not ones people are expected to browse.
    pub advanced: bool,
    /// Shown only when another setting has one of a set of values. None means
    /// always shown.
    pub visible_when: Option<VisibleWhen>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: NavDef
 * WHAT:  A capability's placement in the window's nav rail.
 * WHERE: Present means the capability appears in the nav; absent means it does
 *        not. Read by the window shell.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NavDef {
    pub label: String,
    /// Route segment, e.g. "history". Matched by the router.
    pub route: String,
    /// Lucide icon name. Resolved by the frontend.
    pub icon: String,
    pub order: u32,
}

/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyDef
 * WHAT:  A global shortcut a capability binds, and its default.
 * WHY:   Declared here so conflict detection has one list to check against
 *        rather than discovering a clash at registration time, when the only
 *        available response is a failure the user cannot interpret.
 * WHERE: Read at startup by the hotkey commands and by the rebind UI.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HotkeyDef {
    /// Stable id, e.g. "dictation.toggle".
    pub id: String,
    pub label: String,
    /// Accelerator string in the form tauri-plugin-global-shortcut parses.
    pub default: String,
    /// The setting key holding the user's override, if it is rebindable.
    pub setting_key: Option<String>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: MetricDef
 * WHAT:  A latency stage a capability is expected to emit.
 * WHY:   Declaring metrics closes the loop in both directions: the insights
 *        panel cannot read a stage nothing writes, and nothing can write a
 *        stage that is never surfaced. Both are silent failures otherwise.
 * WHERE: Written by core/telemetry/latency.rs, read by the insights queries.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MetricDef {
    pub stage: LatencyStage,
    pub label: String,
    /// Shown on the insights latency panel rather than kept for diagnostics.
    pub user_facing: bool,
}

/**
 * SOURCE OF TRUTH KEYWORDS: Capability
 * WHAT:  One feature of Echo, completely described.
 * WHERE: The elements of registry::CAPABILITIES.
 */
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Capability {
    pub key: CapabilityKey,
    pub name: String,
    pub description: String,
    pub settings: Vec<SettingDef>,
    /// OS grants required before any command on this capability may run. The
    /// command factory enforces this — handlers never check permissions.
    pub requires: Vec<OsPermission>,
    pub nav: Option<NavDef>,
    pub metrics: Vec<MetricDef>,
    pub hotkey: Option<HotkeyDef>,
}

impl Capability {
    pub fn setting(&self, key: &str) -> Option<&SettingDef> {
        self.settings.iter().find(|s| s.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored form is the existing database encoding, and a change to it
    /// resets every user's settings on upgrade. This is the check that fails
    /// if someone "tidies" it.
    #[test]
    fn stored_form_matches_the_existing_encoding() {
        assert_eq!(SettingValue::Bool(true).to_stored(), "true");
        assert_eq!(SettingValue::Bool(false).to_stored(), "false");
        assert_eq!(SettingValue::Text("local".into()).to_stored(), "local");
        // An integer setting must not gain a ".0" — `inject_delay_ms` is parsed
        // with `parse::<u64>()`, which rejects it.
        assert_eq!(SettingValue::Number(0.0).to_stored(), "0");
        assert_eq!(SettingValue::Number(250.0).to_stored(), "250");
        assert_eq!(SettingValue::Number(0.5).to_stored(), "0.5");
    }
}
