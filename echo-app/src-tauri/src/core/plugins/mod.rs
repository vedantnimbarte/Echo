pub mod dispatch;
pub mod integrity;
pub mod loader;
pub mod scaffold;

// The plugin API lives in the standalone `echo-sdk` crate so external plugin
// authors compile against the same trait/manifest definitions the host uses
// (the FFI is only sound if both sides share them). Re-exported here so
// `crate::core::plugins::…` paths keep working. Only what the host names is
// re-exported; the capability traits are reached through `Plugin::as_*`.
//
// All four capability traits are SDK types now. `AsrPlugin` and
// `DictionaryPlugin` used to live here and name host-internal types
// (`AsrProvider`, the engine's `DictionaryEntry`), which meant no crate outside
// this one could implement them — a trait nobody can write is not an API. The
// SDK defines plain types for each, and `dispatch` translates at the boundary.
pub use echo_sdk::{Plugin, PluginContext, PluginInfo, PluginManifest, Transcript};
