use std::sync::Arc;

use crate::core::asr::AsrProvider;
use crate::core::dictionary::DictionaryEntry;

pub mod loader;

// The base plugin API lives in the standalone `echo-sdk` crate so external
// plugin authors can compile against the same trait/manifest definitions the
// host uses (the FFI is only sound if both sides share them). Re-exported here
// so existing `crate::core::plugins::…` paths keep working unchanged.
pub use echo_sdk::{
    AudioPlugin, OutputPlugin, Plugin, PluginContext, PluginError, PluginInfo, PluginManifest,
    PluginPermission, PluginResult,
};

// The capability traits below reference host-internal types (`AsrProvider`,
// `DictionaryEntry`), so they stay in the app rather than the SDK — pulling
// those into the public SDK would couple it to the host's async runtime and
// error type. They build on the re-exported `Plugin` trait above.

/// A plugin that contributes an ASR provider.
pub trait AsrPlugin: Plugin {
    fn as_asr_provider(&self) -> Arc<dyn AsrProvider>;
}

/// A plugin that contributes dictionary entries.
pub trait DictionaryPlugin: Plugin {
    fn entries(&self) -> Vec<DictionaryEntry>;
}
