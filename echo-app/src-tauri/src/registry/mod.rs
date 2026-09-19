/*!
 * SOURCE OF TRUTH KEYWORDS: CAPABILITIES, capabilities, capability, setting_def,
 *   default_settings, nav_items, hotkey_defs, all_settings, validate_registry,
 *   RegistrySnapshot
 * WHAT:  The single source of truth for what Echo has. One table of
 *        capabilities, plus the lookups every other layer uses to read it.
 * WHY:   A feature is an entry here, never a new pattern. Adding one wires up
 *        its settings control, its permission preflight, its nav placement, its
 *        hotkey and its metrics at once, because all five read this table
 *        instead of keeping their own copies. The rule that follows: if you are
 *        writing a `match` on a feature name anywhere outside this module, the
 *        branch belongs in the table instead.
 *
 *        THE DEFAULTS HERE ARE NOW THE ONLY ONES. Before this table, every
 *        read site carried its own: `whisper_model` was defaulted in three
 *        files, `asr_provider`'s `"local"` in four, and `auto_inject`'s `true`
 *        was spelled as `.map(|v| v != "false").unwrap_or(true)` at each site
 *        that wanted it. Two copies of a default is one default and one bug
 *        waiting for someone to change the other.
 *
 *        Built through LazyLock rather than as a `const`, because the entries
 *        hold owned Strings and Vecs. That is deliberate: the same structs
 *        cross to TypeScript through specta unchanged, so the frontend reads
 *        the identical table rather than a hand-maintained mirror of it.
 * WHERE: Read by ipc/factory.rs (preflight), the settings commands (defaults
 *        and validation), the settings view (control generation) and the
 *        window shell (nav).
 */

pub mod capability;

#[cfg(test)]
mod reachability;

use std::sync::LazyLock;

pub use capability::{
    Capability, CapabilityKey, ChoiceSource, HotkeyDef, LatencyStage, MetricDef, NavDef,
    OsPermission, SettingChoice, SettingDef, SettingKind, SettingSection, SettingValue,
    VisibleWhen,
};

use crate::core::asr::model_manager::{DEFAULT_MODEL, DEFAULT_NEMO_MODEL};
use crate::core::hotkeys::{
    DEFAULT_HOTKEY, DEFAULT_MODE, DEFAULT_RETRY_HOTKEY, DEFAULT_UNDO_HOTKEY,
};
use crate::core::injection::DEFAULT_SETTLE_MS;
use crate::core::wake::DEFAULT_THRESHOLD;

/// Shorthand for the common case: a setting with no permission dependency that
/// is not hidden behind the advanced disclosure.
fn setting(
    key: &str,
    label: &str,
    description: &str,
    section: SettingSection,
    kind: SettingKind,
    default: SettingValue,
) -> SettingDef {
    SettingDef {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        section,
        kind,
        default,
        requires_permission: Vec::new(),
        advanced: false,
        visible_when: None,
    }
}

/// Shows this setting only when `key` holds one of `any_of`. See VisibleWhen.
fn only_when(mut def: SettingDef, key: &str, any_of: &[&str]) -> SettingDef {
    def.visible_when = Some(VisibleWhen {
        key: key.to_string(),
        any_of: any_of.iter().map(|v| v.to_string()).collect(),
    });
    def
}

fn toggle(
    key: &str,
    label: &str,
    description: &str,
    section: SettingSection,
    default: bool,
) -> SettingDef {
    setting(
        key,
        label,
        description,
        section,
        SettingKind::Toggle,
        SettingValue::Bool(default),
    )
}

fn choice(
    key: &str,
    label: &str,
    description: &str,
    section: SettingSection,
    options: Vec<SettingChoice>,
    default: &str,
) -> SettingDef {
    setting(
        key,
        label,
        description,
        section,
        SettingKind::Choice { options },
        SettingValue::Text(default.to_string()),
    )
}

fn dynamic(
    key: &str,
    label: &str,
    description: &str,
    section: SettingSection,
    source: ChoiceSource,
    default: &str,
) -> SettingDef {
    setting(
        key,
        label,
        description,
        section,
        SettingKind::DynamicChoice { source },
        SettingValue::Text(default.to_string()),
    )
}

fn number(
    key: &str,
    label: &str,
    description: &str,
    section: SettingSection,
    min: f64,
    max: f64,
    step: f64,
    unit: Option<&str>,
    default: f64,
) -> SettingDef {
    setting(
        key,
        label,
        description,
        section,
        SettingKind::Number {
            min,
            max,
            step,
            unit: unit.map(str::to_string),
        },
        SettingValue::Number(default),
    )
}

/// Marks a setting as advanced — it exists to unstick a specific machine, not
/// to be browsed.
fn advanced(mut def: SettingDef) -> SettingDef {
    def.advanced = true;
    def
}

/// Declares that this setting cannot do anything without an OS grant, whatever
/// its value. The UI says why rather than offering a control that lies.
fn needs(mut def: SettingDef, permission: OsPermission) -> SettingDef {
    def.requires_permission.push(permission);
    def
}

/**
 * SOURCE OF TRUTH KEYWORDS: CAPABILITIES
 * WHAT:  Every feature Echo has, each with the settings it owns.
 * WHERE: The table the whole app reads. Iterated by every lookup below.
 */
pub static CAPABILITIES: LazyLock<Vec<Capability>> = LazyLock::new(|| {
    vec![
        Capability {
            key: CapabilityKey::Dictation,
            name: "Dictation".into(),
            description: "Press the hotkey, speak, and the words land where the cursor is.".into(),
            requires: vec![OsPermission::Microphone],
            nav: None,
            hotkey: Some(HotkeyDef {
                id: "dictation.toggle".into(),
                label: "Start and stop dictating".into(),
                default: DEFAULT_HOTKEY.into(),
                setting_key: Some("hotkey".into()),
            }),
            metrics: vec![
                MetricDef {
                    stage: LatencyStage::CaptureStart,
                    label: "Microphone ready".into(),
                    user_facing: false,
                },
                MetricDef {
                    stage: LatencyStage::ChunkDecode,
                    label: "Chunk decode".into(),
                    user_facing: false,
                },
                MetricDef {
                    stage: LatencyStage::TailDecode,
                    label: "Final decode".into(),
                    user_facing: true,
                },
                MetricDef {
                    stage: LatencyStage::TotalFinalize,
                    label: "Stop to text".into(),
                    user_facing: true,
                },
            ],
            settings: vec![
                setting(
                    "hotkey",
                    "Dictation hotkey",
                    "Held or tapped, depending on the mode below.",
                    SettingSection::SettingsDictation,
                    SettingKind::Hotkey,
                    SettingValue::Text(DEFAULT_HOTKEY.into()),
                ),
                choice(
                    "recording_mode",
                    "How recording ends",
                    "The hotkey always starts it. What stops it is the choice.",
                    SettingSection::SettingsDictation,
                    vec![
                        SettingChoice::described(
                            "toggle",
                            "Tap the hotkey again",
                            "The recording runs until you say so.",
                        ),
                        SettingChoice::described(
                            "hold",
                            "Hold while speaking",
                            "Releasing the key ends it.",
                        ),
                        SettingChoice::described(
                            "auto",
                            "Stop when you stop talking",
                            "Ends on a pause, with no second keypress.",
                        ),
                    ],
                    DEFAULT_MODE,
                ),
                needs(
                    dynamic(
                        "audio_device",
                        "Microphone",
                        "Empty means whichever device the system is using.",
                        SettingSection::SettingsMicrophone,
                        ChoiceSource::InputDevices,
                        "",
                    ),
                    OsPermission::Microphone,
                ),
                toggle(
                    "warm_mic",
                    "Keep the microphone ready",
                    "Opens the input stream before you press the hotkey, so the first word is \
                     not the one that gets clipped. Costs a little battery.",
                    SettingSection::SettingsMicrophone,
                    true,
                ),
                toggle(
                    "sound_cues",
                    "Play a sound when recording starts and stops",
                    "The pill is often not where you are looking — that is the point of \
                     dictating into another app — so a sound is the feedback that reliably \
                     lands.",
                    SettingSection::SettingsGeneral,
                    false,
                ),
                choice(
                    "vad_engine",
                    "Speech detection",
                    "How Echo decides you have stopped talking.",
                    SettingSection::SettingsMicrophone,
                    vec![
                        SettingChoice::described(
                            "silero",
                            "Neural — ignores background noise",
                            "A small model that tells speech from keyboard clatter and fans.",
                        ),
                        SettingChoice::described(
                            "energy",
                            "Simple — loudness only",
                            "Worth trying if speech is cut off, or a noisy room keeps it awake.",
                        ),
                        SettingChoice::described(
                            "none",
                            "Off",
                            "Record until you stop it yourself.",
                        ),
                    ],
                    "silero",
                ),
                choice(
                    "pill_size",
                    "Pill size",
                    "How much of the screen the recording indicator takes.",
                    SettingSection::SettingsGeneral,
                    vec![
                        SettingChoice::new("large", "Large"),
                        SettingChoice::new("small", "Small"),
                        SettingChoice::described("line", "A line", "Thin enough to ignore."),
                    ],
                    "large",
                ),
            ],
        },
        Capability {
            key: CapabilityKey::Models,
            name: "Voice engine".into(),
            description: "Which model turns your speech into text, and where it runs.".into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "Voice engine".into(),
                route: "engine".into(),
                icon: "audio-waveform".into(),
                order: 40,
            }),
            hotkey: None,
            metrics: vec![],
            settings: vec![
                choice(
                    "asr_provider",
                    "Engine",
                    "Offline runs on this machine and the audio never leaves it. A cloud \
                     provider is faster on a slow machine and costs you an API key.",
                    SettingSection::EngineSpeech,
                    vec![
                        SettingChoice::described(
                            "local",
                            "Offline (Whisper)",
                            "Nothing leaves this machine.",
                        ),
                        SettingChoice::described(
                            "nemo",
                            "Offline (NeMo)",
                            "Streams partials as you speak.",
                        ),
                        SettingChoice::new("cloud", "A cloud provider"),
                    ],
                    "local",
                ),
                dynamic(
                    "whisper_model",
                    "Whisper model",
                    "Bigger models are more accurate and slower. The `.en` models are \
                     English-only and better at it than the multilingual model of the same size.",
                    SettingSection::EngineSpeech,
                    ChoiceSource::WhisperModels,
                    DEFAULT_MODEL,
                ),
                dynamic(
                    "nemo_model",
                    "NeMo model",
                    "Used when the NeMo engine is selected.",
                    SettingSection::EngineSpeech,
                    ChoiceSource::NemoModels,
                    DEFAULT_NEMO_MODEL,
                ),
                dynamic(
                    "language",
                    "Spoken language",
                    "Auto-detect costs a little accuracy. Pinning the language you actually \
                     speak is the cheapest accuracy win available.",
                    SettingSection::EngineSpeech,
                    ChoiceSource::Languages,
                    "auto",
                ),
                toggle(
                    "gpu_enabled",
                    "Use the GPU when one is available",
                    "Falls back to the processor by itself if a run fails, and stays there \
                     for the rest of the session.",
                    SettingSection::EngineAdvanced,
                    true,
                ),
                advanced(number(
                    "whisper_threads",
                    "Processor threads",
                    "Zero lets Whisper choose. Raise it only if decoding is slow and the \
                     machine is otherwise idle.",
                    SettingSection::EngineAdvanced,
                    0.0,
                    32.0,
                    1.0,
                    None,
                    0.0,
                )),
                toggle(
                    "stream_partials",
                    "Show words as you say them",
                    "Types a running approximation into the app and corrects it when you \
                     stop. Off by default because the correction is visible.",
                    SettingSection::SettingsDictation,
                    false,
                ),
            ],
        },
        Capability {
            key: CapabilityKey::Providers,
            name: "Cloud providers".into(),
            description: "Ten transcription services, each on your own key.".into(),
            requires: vec![],
            nav: None,
            hotkey: None,
            metrics: vec![],
            settings: vec![],
        },
        Capability {
            key: CapabilityKey::Output,
            name: "Output".into(),
            description: "What Echo does with the words once it has them.".into(),
            requires: vec![OsPermission::Accessibility],
            nav: Some(NavDef {
                label: "Output".into(),
                route: "output".into(),
                icon: "keyboard".into(),
                order: 50,
            }),
            hotkey: None,
            metrics: vec![
                MetricDef {
                    stage: LatencyStage::Assemble,
                    label: "Formatting".into(),
                    user_facing: false,
                },
                MetricDef {
                    stage: LatencyStage::Inject,
                    label: "Typing it in".into(),
                    user_facing: true,
                },
            ],
            settings: vec![
                needs(
                    toggle(
                        "auto_inject",
                        "Type the transcript into the focused app",
                        "Off leaves it on the clipboard for you to paste.",
                        SettingSection::OutputInsert,
                        true,
                    ),
                    OsPermission::Accessibility,
                ),
                choice(
                    "injection_method",
                    "How to insert it",
                    "Pasting is instant but borrows the clipboard. Typing is slower and \
                     survives apps that refuse a paste.",
                    SettingSection::OutputInsert,
                    vec![
                        SettingChoice::described(
                            "auto",
                            "Decide per app",
                            "Paste long text, type short.",
                        ),
                        SettingChoice::new("paste", "Always paste"),
                        SettingChoice::new("type", "Always type"),
                    ],
                    // NOT "auto". An unset injection_method is read as typing by
                    // core::injection::use_paste_for, so declaring "auto" here
                    // would show every untouched install a control that
                    // disagrees with what the app actually does.
                    "type",
                ),
                number(
                    "inject_delay_ms",
                    "Insert delay",
                    "For an app that needs a moment to take focus back after the hotkey.",
                    SettingSection::OutputAdvanced,
                    0.0,
                    2000.0,
                    10.0,
                    Some("ms"),
                    0.0,
                ),
                only_when(
                    number(
                        "clipboard_settle_ms",
                        "Clipboard hold",
                        "How long Echo waits before putting your old clipboard back. Raise it \
                         if a paste lands empty on a slow machine.",
                        SettingSection::OutputAdvanced,
                        0.0,
                        2000.0,
                        10.0,
                        Some("ms"),
                        DEFAULT_SETTLE_MS as f64,
                    ),
                    "injection_method",
                    &["paste", "auto"],
                ),
                toggle(
                    "auto_edit",
                    "Clean up filler and stutters",
                    "Drops \"um\" and the repeated half-word, in the languages that have rules.",
                    SettingSection::OutputFormatting,
                    true,
                ),
                toggle(
                    "spoken_punctuation",
                    "Take spoken punctuation",
                    "Saying \"comma\" writes one. Off by default: it costs you the word.",
                    SettingSection::OutputFormatting,
                    false,
                ),
                toggle(
                    "format_numbers",
                    "Write numbers as digits",
                    "\"Twenty past nine\" becomes \"9:20\".",
                    SettingSection::OutputFormatting,
                    true,
                ),
                toggle(
                    "format_tidy",
                    "Tidy spacing and capitals",
                    "Sentence case, one space after a full stop.",
                    SettingSection::OutputFormatting,
                    true,
                ),
                toggle(
                    "app_style_enabled",
                    "Follow the focused app's house style",
                    "A terminal takes the words exactly as spoken; a chat box does not.",
                    SettingSection::OutputApps,
                    false,
                ),
                needs(
                    toggle(
                        "block_secure_fields",
                        "Never type into a password field",
                        "Echo asks the accessibility API what the focused field is. Where the \
                         API will not answer, Echo declines rather than guesses.",
                        SettingSection::OutputApps,
                        true,
                    ),
                    OsPermission::Accessibility,
                ),
            ],
        },
        Capability {
            key: CapabilityKey::Dictionary,
            name: "Custom dictionary".into(),
            description: "The names and jargon the model gets wrong, biased at the decoder \
                          rather than corrected afterwards."
                .into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "Dictionary".into(),
                route: "dictionary".into(),
                icon: "book-a".into(),
                order: 30,
            }),
            hotkey: None,
            metrics: vec![],
            settings: vec![
                toggle(
                    "auto_learn",
                    "Learn from corrections",
                    "When you fix a word by hand just after dictating it, Echo offers to \
                     remember the correction.",
                    SettingSection::OutputAdvanced,
                    true,
                ),
                toggle(
                    "scratch_that_enabled",
                    "\"Scratch that\" deletes the last thing said",
                    "Spoken, not typed.",
                    SettingSection::OutputAdvanced,
                    false,
                ),
                toggle(
                    "command_mode_enabled",
                    "Voice commands",
                    "A spoken prefix turns the rest of the sentence into an instruction \
                     rather than text.",
                    SettingSection::OutputAdvanced,
                    false,
                ),
                setting(
                    "command_prefix",
                    "Command prefix",
                    "The words that mark a sentence as an instruction.",
                    SettingSection::OutputAdvanced,
                    SettingKind::Text {
                        placeholder: Some("hey echo".into()),
                        max_len: Some(64),
                    },
                    SettingValue::Text("hey echo".into()),
                ),
                choice(
                    "command_llm_provider",
                    "Which model runs commands",
                    "Ollama runs on this machine. Anything else sends the sentence out.",
                    SettingSection::OutputAdvanced,
                    vec![
                        SettingChoice::described("ollama", "Ollama", "Local. Nothing leaves."),
                        SettingChoice::new("openai", "OpenAI"),
                    ],
                    "ollama",
                ),
                setting(
                    "command_llm_model",
                    "Command model",
                    "The model name as that provider spells it.",
                    SettingSection::OutputAdvanced,
                    SettingKind::Text {
                        placeholder: Some("llama3.2".into()),
                        max_len: Some(128),
                    },
                    SettingValue::Text("llama3.2".into()),
                ),
                advanced(setting(
                    "ollama_endpoint",
                    "Ollama endpoint",
                    "Where your Ollama is listening.",
                    SettingSection::OutputAdvanced,
                    SettingKind::Text {
                        placeholder: Some("http://localhost:11434".into()),
                        max_len: Some(256),
                    },
                    SettingValue::Text("http://localhost:11434".into()),
                )),
                toggle(
                    "auto_edit_llm",
                    "Let a model rewrite the transcript",
                    "Reads better and takes longer. Off by default because it is the one \
                     stage that can change what you said.",
                    SettingSection::OutputFormatting,
                    false,
                ),
            ],
        },
        Capability {
            key: CapabilityKey::WakeWord,
            name: "Wake word".into(),
            description: "Start dictating by saying a phrase rather than pressing a key.".into(),
            requires: vec![OsPermission::Microphone],
            nav: None,
            hotkey: None,
            metrics: vec![],
            settings: vec![
                toggle(
                    "wake_word_enabled",
                    "Listen for a wake word",
                    "Keeps the microphone open and runs a small spotter on it. Nothing is \
                     transcribed until the phrase is heard.",
                    SettingSection::SettingsDictation,
                    false,
                ),
                dynamic(
                    "wake_word_model",
                    "Wake phrase",
                    "Each phrase is its own downloaded model.",
                    SettingSection::SettingsDictation,
                    ChoiceSource::WakeWordModels,
                    "",
                ),
                number(
                    "wake_word_sensitivity",
                    "Sensitivity",
                    "Higher triggers more easily, and on more things that were not the phrase.",
                    SettingSection::SettingsDictation,
                    0.0,
                    1.0,
                    0.05,
                    None,
                    DEFAULT_THRESHOLD as f64,
                ),
            ],
        },
        Capability {
            key: CapabilityKey::History,
            name: "History".into(),
            description: "Every transcript, searchable, on this machine only.".into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "History".into(),
                route: "history".into(),
                icon: "list".into(),
                order: 10,
            }),
            hotkey: Some(HotkeyDef {
                id: "history.undo".into(),
                label: "Undo the last insert".into(),
                default: DEFAULT_UNDO_HOTKEY.into(),
                setting_key: None,
            }),
            metrics: vec![],
            settings: vec![
                toggle(
                    "history_enabled",
                    "Keep a history",
                    "Off means the transcript is delivered and then forgotten.",
                    SettingSection::Privacy,
                    true,
                ),
                number(
                    "history_retention_days",
                    "Delete transcripts older than",
                    "Zero keeps them forever. The sweep runs at launch and after every \
                     dictation, so the window is a promise rather than a launch-time tidy.",
                    SettingSection::Privacy,
                    0.0,
                    3650.0,
                    1.0,
                    Some("days"),
                    0.0,
                ),
            ],
        },
        Capability {
            key: CapabilityKey::Insights,
            name: "Insights".into(),
            description: "What your own history says about how you dictate.".into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "Insights".into(),
                route: "insights".into(),
                icon: "chart-line".into(),
                order: 20,
            }),
            hotkey: None,
            metrics: vec![],
            settings: vec![],
        },
        Capability {
            key: CapabilityKey::Profiles,
            name: "Per-app profiles".into(),
            description: "Override insert behaviour and dictionary scope for one application."
                .into(),
            requires: vec![OsPermission::Accessibility],
            nav: None,
            hotkey: None,
            metrics: vec![],
            settings: vec![],
        },
        Capability {
            key: CapabilityKey::Plugins,
            name: "Plugins".into(),
            description: "Third-party code that sees transcripts you have already agreed to keep."
                .into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "Plugins".into(),
                route: "plugins".into(),
                icon: "blocks".into(),
                order: 60,
            }),
            hotkey: None,
            metrics: vec![],
            settings: vec![],
        },
        Capability {
            key: CapabilityKey::Privacy,
            name: "Privacy".into(),
            description: "What left this machine, and what Echo counts.".into(),
            requires: vec![],
            nav: Some(NavDef {
                label: "Privacy".into(),
                route: "privacy".into(),
                icon: "shield".into(),
                order: 70,
            }),
            hotkey: None,
            metrics: vec![],
            settings: vec![toggle(
                "telemetry_enabled",
                "Count how Echo is used",
                "Stored in the local database and sent nowhere. Off means not even counted.",
                SettingSection::Privacy,
                false,
            )],
        },
        Capability {
            key: CapabilityKey::Settings,
            name: "Settings".into(),
            description: "The window this table generates.".into(),
            requires: vec![],
            nav: None,
            hotkey: None,
            metrics: vec![],
            settings: vec![
                choice(
                    "ui_language",
                    "Language of this window",
                    "Separate from the language you dictate in.",
                    SettingSection::SettingsGeneral,
                    vec![
                        SettingChoice::new("auto", "Match the system"),
                        SettingChoice::new("en", "English"),
                        SettingChoice::new("es", "Español"),
                        SettingChoice::new("de", "Deutsch"),
                        SettingChoice::new("fr", "Français"),
                    ],
                    "auto",
                ),
                toggle(
                    "retry_enabled",
                    "Retry on a stronger model",
                    "A second hotkey re-runs the last dictation on a bigger model, because by \
                     the time you notice a mistake the focus has moved on.",
                    SettingSection::SettingsDictation,
                    true,
                ),
                dynamic(
                    "retry_target",
                    "Retry with",
                    "Empty picks the best installed model that is not the one you just used.",
                    SettingSection::SettingsDictation,
                    ChoiceSource::WhisperModels,
                    "",
                ),
            ],
        },
        Capability {
            key: CapabilityKey::Onboarding,
            name: "Setup".into(),
            description: "First launch: the microphone, a model, and the hotkey.".into(),
            requires: vec![],
            nav: None,
            hotkey: None,
            metrics: vec![],
            settings: vec![toggle(
                "onboarding_complete",
                "Setup finished",
                "Not shown in settings; set by the setup flow.",
                SettingSection::SettingsGeneral,
                false,
            )],
        },
        Capability {
            key: CapabilityKey::Updates,
            name: "Updates".into(),
            description: "Checks a signed release feed, and can be switched off.".into(),
            requires: vec![],
            nav: None,
            hotkey: Some(HotkeyDef {
                id: "dictation.retry".into(),
                label: "Retry the last dictation".into(),
                default: DEFAULT_RETRY_HOTKEY.into(),
                setting_key: None,
            }),
            metrics: vec![],
            settings: vec![toggle(
                "check_updates_on_start",
                "Check for updates at launch",
                "The only network request Echo makes on its own. Off means Echo never reaches                  out unless you ask it to.",
                SettingSection::SettingsGeneral,
                true,
            )],
        },
    ]
});

/// Every capability, in table order.
pub fn capabilities() -> &'static [Capability] {
    &CAPABILITIES
}

/// One capability by key. Infallible by construction — the table is exhaustive
/// over `CapabilityKey`, and `validate_registry` is the test that keeps it so.
pub fn capability(key: CapabilityKey) -> &'static Capability {
    CAPABILITIES
        .iter()
        .find(|c| c.key == key)
        .unwrap_or_else(|| unreachable!("registry has no entry for {key:?}; validate_registry"))
}

/// Every setting in the app, flattened across capabilities.
pub fn all_settings() -> impl Iterator<Item = &'static SettingDef> {
    CAPABILITIES.iter().flat_map(|c| c.settings.iter())
}

/// One setting by key, wherever it lives.
pub fn setting_def(key: &str) -> Option<&'static SettingDef> {
    all_settings().find(|s| s.key == key)
}

/// The default a setting falls back to when nothing is stored, in the string
/// form the settings table holds. This is the ONLY place a default lives.
pub fn default_for(key: &str) -> Option<String> {
    setting_def(key).map(|d| d.default.to_stored())
}

/// Nav entries, already sorted. The rail renders this and nothing else.
pub fn nav_items() -> Vec<(&'static CapabilityKey, &'static NavDef)> {
    let mut items: Vec<_> = CAPABILITIES
        .iter()
        .filter_map(|c| c.nav.as_ref().map(|n| (&c.key, n)))
        .collect();
    items.sort_by_key(|(_, n)| n.order);
    items
}

/// Every hotkey the app binds, so conflict detection has one list to check.
pub fn hotkey_defs() -> Vec<&'static HotkeyDef> {
    CAPABILITIES
        .iter()
        .filter_map(|c| c.hotkey.as_ref())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Structural checks on the table itself. Reachability — whether anything
    /// actually READS these — is a separate and much harder test, and it lives
    /// in reachability.rs.
    #[test]
    fn validate_registry() {
        let mut keys = HashSet::new();
        for setting in all_settings() {
            assert!(
                keys.insert(setting.key.clone()),
                "duplicate setting key {}: two capabilities both claim it, so one of them \
                 silently loses",
                setting.key
            );
        }

        let mut capability_keys = HashSet::new();
        for capability in capabilities() {
            assert!(
                capability_keys.insert(capability.key),
                "duplicate capability {:?}",
                capability.key
            );
        }
    }

    /// A default that does not match its own declared kind is a crash waiting
    /// for the first user to open that pane, so it is caught here instead.
    #[test]
    fn every_default_matches_its_kind() {
        for setting in all_settings() {
            let ok = match (&setting.kind, &setting.default) {
                (SettingKind::Toggle, SettingValue::Bool(_)) => true,
                (SettingKind::Number { min, max, .. }, SettingValue::Number(n)) => {
                    assert!(
                        n >= min && n <= max,
                        "{} defaults to {n}, outside its own {min}..={max} range",
                        setting.key
                    );
                    true
                }
                (SettingKind::Text { .. }, SettingValue::Text(_)) => true,
                (SettingKind::Hotkey, SettingValue::Text(_)) => true,
                (SettingKind::DynamicChoice { .. }, SettingValue::Text(_)) => true,
                (SettingKind::Choice { options }, SettingValue::Text(v)) => {
                    assert!(
                        options.iter().any(|o| &o.value == v),
                        "{} defaults to {v:?}, which is not one of its own options",
                        setting.key
                    );
                    true
                }
                _ => false,
            };
            assert!(ok, "{} has a default of the wrong kind", setting.key);
        }
    }

    /// A condition pointing at a key that does not exist hides the control
    /// forever, and does it silently — the row simply never renders, which
    /// looks exactly like the setting not existing.
    #[test]
    fn every_condition_names_a_real_setting() {
        for setting in all_settings() {
            let Some(condition) = &setting.visible_when else {
                continue;
            };
            let target = setting_def(&condition.key).unwrap_or_else(|| {
                panic!(
                    "{} is shown only when {:?} has certain values, but there is no such setting",
                    setting.key, condition.key
                )
            });

            // And the values it waits for must be values that key can hold, or
            // the control is equally invisible for a subtler reason.
            if let SettingKind::Choice { options } = &target.kind {
                for wanted in &condition.any_of {
                    assert!(
                        options.iter().any(|o| &o.value == wanted),
                        "{} waits for {} to be {:?}, which is not one of its options",
                        setting.key,
                        condition.key,
                        wanted
                    );
                }
            }
        }
    }

    /// Two capabilities binding the same accelerator means one of them never
    /// fires, and the one that loses is decided by registration order.
    #[test]
    fn no_two_hotkeys_share_a_default() {
        let mut seen = HashSet::new();
        for hotkey in hotkey_defs() {
            assert!(
                seen.insert(hotkey.default.clone()),
                "{} defaults to {}, which another capability already binds",
                hotkey.id,
                hotkey.default
            );
        }
    }

    /// Nav order decides the rail, so a tie is a rail that reorders itself
    /// between runs depending on how the vec happened to sort.
    #[test]
    fn nav_order_is_unambiguous() {
        let mut seen = HashSet::new();
        for (key, nav) in nav_items() {
            assert!(
                seen.insert(nav.order),
                "{key:?} claims nav order {}, which is already taken",
                nav.order
            );
        }
    }
}
