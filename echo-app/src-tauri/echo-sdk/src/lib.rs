//! # echo-sdk
//!
//! The public API for writing **Echo** plugins. Echo loads plugins as native
//! shared libraries (`.dll` / `.dylib` / `.so`) at runtime, so a plugin and the
//! host must agree on the exact shape of the [`Plugin`] trait — that shared
//! definition lives here.
//!
//! A minimal plugin:
//!
//! ```
//! use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};
//!
//! #[derive(Default)]
//! struct HelloPlugin;
//!
//! impl Plugin for HelloPlugin {
//!     fn name(&self) -> &str { "hello" }
//!     fn version(&self) -> &str { "0.1.0" }
//!     fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
//!     fn on_unload(&self) -> PluginResult<()> { Ok(()) }
//! }
//!
//! // Emits the entry points the host looks for.
//! export_plugin!(HelloPlugin);
//! ```
//!
//! To do more than exist, implement a capability — [`OutputPlugin`],
//! [`AudioPlugin`], [`DictionaryPlugin`] or [`AsrPlugin`] — and return
//! `Some(self)` from the matching `as_*` method on [`Plugin`].
//!
//! Ship the compiled library next to a `plugin.json` describing it (see
//! [`PluginManifest`]).

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Error returned by plugin hooks.
///
/// Deliberately dependency-free (just a message) so the SDK stays decoupled
/// from the host's error type and async runtime. The host converts this into
/// its own error on the boundary.
#[derive(Debug, Clone)]
pub struct PluginError(pub String);

impl PluginError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PluginError {}

impl From<String> for PluginError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for PluginError {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Result alias used throughout the plugin API.
pub type PluginResult<T> = std::result::Result<T, PluginError>;

/// Read-only accessor a plugin uses to query host settings. Returns `None` for
/// keys the host does not expose to plugins.
pub type SettingsAccessor = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Declared capabilities a plugin may request in its manifest. The permission
/// model is advisory in this version (plugins run in-process); true sandboxing
/// (e.g. a WASM runtime) is a future goal.
///
/// What Echo actually calls is decided by the `as_*` methods on [`Plugin`], not
/// by this list: declaring `output` without implementing it gets no calls, and
/// implementing it without declaring it still does. The list is what the user
/// is shown before installing, so keep the two in agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPermission {
    Asr,
    Output,
    Audio,
    Dictionary,
}

/// Manifest shipped alongside a plugin as `plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// Shared-library file name (e.g. `my_plugin.dll`).
    pub entry: String,
}

/// Summary of an installed plugin returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    /// What the plugin *declares* it needs. Advisory — see [`PluginPermission`].
    /// Surfaced so a user can at least see what was claimed before enabling it.
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
}

/// Runtime context handed to a plugin on load.
pub struct PluginContext {
    /// Directory the plugin may use for its own data files.
    pub data_dir: PathBuf,
    /// Read-only accessor for host settings.
    pub settings: SettingsAccessor,
}

/// The shape of the trait objects below, as a number the host can check before
/// it touches one.
///
/// A plugin hands Echo a `Box<dyn Plugin>`, and a trait object is a pointer to
/// a vtable whose layout is fixed when the plugin is compiled. Adding a method
/// to [`Plugin`] moves the slots, so a library built against an older SDK does
/// not fail to load — it loads, and the host calls whatever happens to sit
/// where `as_output` now is. That is undefined behaviour with no error message.
/// So [`export_plugin!`] bakes this constant into the library behind a C-ABI
/// function (a `u32` has one layout on every compiler), and the host refuses
/// any library whose number is missing or different before calling anything
/// else in it.
///
/// Bumped whenever a trait here changes shape, together with the crate's minor
/// version. `1` was the lifecycle-only API of echo-sdk 0.1, which exported no
/// marker at all; `2` (echo-sdk 0.2) added capabilities.
pub const ABI_VERSION: u32 = 2;

/// Base trait every plugin implements. `Send + Sync` because plugins live in
/// shared application state and are called from Echo's worker threads.
///
/// ## Capabilities
///
/// The host holds a `Box<dyn Plugin>`, and Rust cannot downcast a trait object
/// to some other trait the value behind it happens to implement. So a plugin
/// *says* what else it is: override the matching `as_*` method to return
/// `Some(self)`, and Echo starts calling that capability. The defaults return
/// `None`, so a plugin that only wants lifecycle hooks writes nothing extra.
///
/// ```
/// # use echo_sdk::*;
/// # #[derive(Default)] struct Counter;
/// impl Plugin for Counter {
///     fn name(&self) -> &str { "counter" }
///     fn version(&self) -> &str { "0.1.0" }
///     fn on_load(&self, _: &PluginContext) -> PluginResult<()> { Ok(()) }
///     fn on_unload(&self) -> PluginResult<()> { Ok(()) }
///     fn as_output(&self) -> Option<&dyn OutputPlugin> { Some(self) }
/// }
///
/// impl OutputPlugin for Counter {
///     fn on_transcript(&self, t: &Transcript) -> PluginResult<()> {
///         println!("{} characters", t.text.chars().count());
///         Ok(())
///     }
/// }
/// ```
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()>;
    fn on_unload(&self) -> PluginResult<()>;

    /// Return `Some(self)` to be told about every delivered transcript.
    fn as_output(&self) -> Option<&dyn OutputPlugin> {
        None
    }

    /// Return `Some(self)` to process captured audio before speech detection.
    fn as_audio(&self) -> Option<&dyn AudioPlugin> {
        None
    }

    /// Return `Some(self)` to add entries to the user's dictionary.
    fn as_dictionary(&self) -> Option<&dyn DictionaryPlugin> {
        None
    }

    /// Return `Some(self)` to offer a transcription engine.
    fn as_asr(&self) -> Option<&dyn AsrPlugin> {
        None
    }
}

/// A finished transcript, as Echo delivered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// The text after the dictionary, formatting and command mode — what went
    /// to the focused app, or would have with auto-typing off.
    pub text: String,
    /// The focused application's identifier when the text was delivered: an
    /// executable name on Windows, a bundle id on macOS. `None` when Echo could
    /// not tell.
    pub app: Option<String>,
    /// Language the decoder reported, or else the one the user configured.
    /// `None` when neither is known (auto-detect with a silent decoder).
    pub language: Option<String>,
}

/// A plugin that is told about each finished transcript.
///
/// **It observes; it cannot change or stop delivery.** Echo types the text
/// first and calls this afterwards, on a background thread, one transcript at
/// a time and in order. That is deliberate: delivering the user's words is the
/// one thing that must not fail, and a hook able to rewrite or swallow them
/// would put every sentence at the mercy of the least careful plugin installed
/// — with the words gone, not merely garbled, when it went wrong. Changing
/// words belongs to [`DictionaryPlugin`], which runs before delivery through a
/// stage the user can already see and control.
///
/// Take as long as you need: a slow call never delays typing, though it does
/// queue the next transcript behind it. A transcript from an app where the user
/// has turned history off is not sent — a plugin that writes text down is
/// history by another name. Password fields, "scratch that" and a failed
/// command-mode run deliver nothing, so they reach no plugin either.
pub trait OutputPlugin {
    fn on_transcript(&self, transcript: &Transcript) -> PluginResult<()>;
}

/// A plugin that processes captured audio before speech detection sees it.
///
/// `samples` is one capture chunk of 16 kHz mono `f32`, typically tens of
/// milliseconds long. **This is the hot path**: it runs on every chunk while
/// the microphone is open, ahead of voice detection and transcription, so time
/// spent here is latency added to every word. Echo logs what audio plugins cost
/// at the end of each recording.
///
/// Change the samples or the length as you like, but keep the format. Returning
/// an error — or emptying the buffer, which downstream reads as "the microphone
/// stopped" — discards what you did to that chunk and stops Echo calling this
/// hook until the plugin is next enabled, rather than logging the same failure
/// thirty times a second.
pub trait AudioPlugin {
    fn process(&self, samples: &mut Vec<f32>) -> PluginResult<()>;
}

/// One replacement a plugin adds to the dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryEntry {
    /// What the decoder writes, matched case-insensitively. Blank is ignored.
    pub phrase: String,
    /// What to write instead. Blank deletes the phrase.
    pub replacement: String,
}

/// A plugin that adds entries to the dictionary.
///
/// Asked when the plugin is enabled and whenever the user's dictionary changes
/// — not per transcript, so return what you have rather than fetching it. Your
/// entries apply everywhere, *after* the user's own: a replacement the user
/// wrote for the same phrase wins, because theirs has already rewritten the
/// text by the time yours is tried. They also bias the local decoder as
/// vocabulary hints, exactly as the user's entries do.
pub trait DictionaryPlugin {
    fn entries(&self) -> PluginResult<Vec<DictionaryEntry>>;
}

/// What a transcription engine heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcription {
    pub text: String,
    /// The language detected, if the engine knows. Formatting uses it.
    pub language: Option<String>,
}

/// A plugin that transcribes speech.
///
/// Once the plugin is enabled its engine is registered as
/// `plugin:<your plugin name>` beside the built-in ones, and does nothing until
/// the user selects it — offering an engine is not the same as taking over
/// dictation. `audio` is one whole utterance of 16 kHz mono `f32`; `language`
/// is an ISO 639-1 code, or `None` to detect it. Called on a worker thread, one
/// utterance at a time.
///
/// If it fails and Echo's offline engine is installed, that utterance is
/// transcribed locally instead of being lost.
pub trait AsrPlugin {
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> PluginResult<Transcription>;
}

/// The plugin as the host really receives it: every hook caught at the library
/// boundary.
///
/// A plugin library carries its own copy of the Rust standard library, and a
/// panic raised by one copy of std cannot be caught by another — the unwinder
/// sees a foreign exception and aborts the process. So `catch_unwind` on Echo's
/// side of the boundary would protect nothing; it has to run *inside* the
/// plugin, compiled against the plugin's std. [`export_plugin!`] wraps your
/// type in this, which turns a panic in any hook into a [`PluginError`] the
/// host logs and carries on from.
///
/// `name` and `version` are not wrapped: they return borrowed strings, there is
/// nothing to hand back in their place, and they have no business panicking.
///
/// This only holds while the plugin unwinds on panic, which is Cargo's default.
/// Set `panic = "abort"` in your profile and a panic ends Echo, wrapper or not.
pub struct Guarded<P>(pub P);

/// Run a hook, turning a panic into an error.
fn guard<T>(hook: &str, call: impl FnOnce() -> PluginResult<T>) -> PluginResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_else(|payload| {
        let why = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".into());
        Err(PluginError(format!("{hook} panicked: {why}")))
    })
}

/// The wrapped type stopped offering a capability between the host asking and
/// the host calling. An error, not a panic: that would be the one outcome this
/// wrapper exists to rule out.
fn withdrawn(what: &str) -> PluginError {
    PluginError(format!("the {what} capability is no longer offered"))
}

impl<P: Plugin> Plugin for Guarded<P> {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn version(&self) -> &str {
        self.0.version()
    }
    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        guard("on_load", || self.0.on_load(ctx))
    }
    fn on_unload(&self) -> PluginResult<()> {
        guard("on_unload", || self.0.on_unload())
    }
    // Each `as_*` answers with the wrapper rather than the inner type, so the
    // capability call itself comes back through the guarded impls below.
    fn as_output(&self) -> Option<&dyn OutputPlugin> {
        self.0.as_output().map(|_| self as &dyn OutputPlugin)
    }
    fn as_audio(&self) -> Option<&dyn AudioPlugin> {
        self.0.as_audio().map(|_| self as &dyn AudioPlugin)
    }
    fn as_dictionary(&self) -> Option<&dyn DictionaryPlugin> {
        self.0
            .as_dictionary()
            .map(|_| self as &dyn DictionaryPlugin)
    }
    fn as_asr(&self) -> Option<&dyn AsrPlugin> {
        self.0.as_asr().map(|_| self as &dyn AsrPlugin)
    }
}

impl<P: Plugin> OutputPlugin for Guarded<P> {
    fn on_transcript(&self, transcript: &Transcript) -> PluginResult<()> {
        guard("on_transcript", || {
            self.0
                .as_output()
                .ok_or_else(|| withdrawn("output"))?
                .on_transcript(transcript)
        })
    }
}

impl<P: Plugin> AudioPlugin for Guarded<P> {
    fn process(&self, samples: &mut Vec<f32>) -> PluginResult<()> {
        guard("process", || {
            self.0
                .as_audio()
                .ok_or_else(|| withdrawn("audio"))?
                .process(samples)
        })
    }
}

impl<P: Plugin> DictionaryPlugin for Guarded<P> {
    fn entries(&self) -> PluginResult<Vec<DictionaryEntry>> {
        guard("entries", || {
            self.0
                .as_dictionary()
                .ok_or_else(|| withdrawn("dictionary"))?
                .entries()
        })
    }
}

impl<P: Plugin> AsrPlugin for Guarded<P> {
    fn transcribe(&self, audio: &[f32], language: Option<&str>) -> PluginResult<Transcription> {
        guard("transcribe", || {
            self.0
                .as_asr()
                .ok_or_else(|| withdrawn("asr"))?
                .transcribe(audio, language)
        })
    }
}

/// Emit the FFI entry points the host loader looks up.
///
/// `echo_plugin_create` returns your plugin wrapped in [`Guarded`], so a panic
/// in any hook becomes an error rather than an aborted Echo.
/// `echo_plugin_abi_version` returns the [`ABI_VERSION`] this library was built
/// against, so the host can refuse a mismatch before calling anything else.
///
/// The plugin type must implement [`Plugin`] and [`Default`]. The host takes
/// ownership of the returned boxed trait object.
///
/// ```
/// # use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};
/// #[derive(Default)]
/// struct MyPlugin;
/// impl Plugin for MyPlugin {
///     fn name(&self) -> &str { "my-plugin" }
///     fn version(&self) -> &str { "0.1.0" }
///     fn on_load(&self, _: &PluginContext) -> PluginResult<()> { Ok(()) }
///     fn on_unload(&self) -> PluginResult<()> { Ok(()) }
/// }
/// export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin_ty:ty) => {
        /// FFI constructor called by the Echo host after `dlopen`.
        #[no_mangle]
        pub extern "C" fn echo_plugin_create() -> *mut ::std::boxed::Box<dyn $crate::Plugin> {
            let plugin: ::std::boxed::Box<dyn $crate::Plugin> = ::std::boxed::Box::new(
                $crate::Guarded(<$plugin_ty as ::std::default::Default>::default()),
            );
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(plugin))
        }

        /// The SDK shape this library was compiled against. Read by the host
        /// before `echo_plugin_create`, which it will not call on a mismatch.
        #[no_mangle]
        pub extern "C" fn echo_plugin_abi_version() -> u32 {
            $crate::ABI_VERSION
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Explodes;

    impl Plugin for Explodes {
        fn name(&self) -> &str {
            "explodes"
        }
        fn version(&self) -> &str {
            "0"
        }
        fn on_load(&self, _: &PluginContext) -> PluginResult<()> {
            panic!("on purpose")
        }
        fn on_unload(&self) -> PluginResult<()> {
            Ok(())
        }
        fn as_output(&self) -> Option<&dyn OutputPlugin> {
            Some(self)
        }
    }

    impl OutputPlugin for Explodes {
        fn on_transcript(&self, _: &Transcript) -> PluginResult<()> {
            panic!("{}", String::from("formatted, so the payload is a String"))
        }
    }

    /// The wrapper is the whole of the panic story for a real plugin, since the
    /// host cannot catch one from another copy of std.
    #[test]
    fn a_panicking_hook_comes_back_as_an_error() {
        let plugin = Guarded(Explodes);
        let ctx = PluginContext {
            data_dir: PathBuf::new(),
            settings: Arc::new(|_| None),
        };
        let err = plugin.on_load(&ctx).unwrap_err();
        assert!(err.0.contains("on purpose"), "{err}");

        let t = Transcript {
            text: "hi".into(),
            app: None,
            language: None,
        };
        let err = plugin.as_output().unwrap().on_transcript(&t).unwrap_err();
        assert!(err.0.contains("String"), "{err}");
    }

    /// Wrapping must not invent capabilities the plugin never offered.
    #[test]
    fn the_wrapper_offers_only_what_the_plugin_does() {
        let plugin = Guarded(Explodes);
        assert!(plugin.as_output().is_some());
        assert!(plugin.as_audio().is_none());
        assert!(plugin.as_dictionary().is_none());
        assert!(plugin.as_asr().is_none());
    }
}
