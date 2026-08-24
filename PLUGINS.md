# Echo Plugins

Echo supports native plugins loaded as shared libraries (`.dll` / `.dylib` /
`.so`).

## ⚠️ Security: read this before installing anything

**A plugin is not sandboxed. Installing one is equivalent to running an
arbitrary program with your user account.**

Echo loads plugins with `dlopen`/`LoadLibrary` into its own process. That means
a plugin can:

- read and write any file your user can, not just its own data directory
- read your microphone, your transcripts, and your dictionary
- make network requests — including ones the built-in
  [egress log](README.md) cannot see, because it only instruments Echo's own
  request code
- read and use anything Echo has in memory, including a decrypted API key
- crash Echo, since it shares the process

**The `permissions` list in `plugin.json` is advisory and is not enforced.** It
documents what the author says the plugin needs. Nothing stops a plugin
declaring `["dictionary"]` and then doing something else entirely. Treat it as a
README line, not a security boundary.

Because there is no technical boundary, the only real control is consent:
installing requires an explicit confirmation that names these risks, and every
load writes a warning to the log. That stops a plugin being loaded *silently*.
It cannot stop a malicious plugin from doing whatever it wants once loaded.

**Practical advice:** install plugins you have built yourself or whose source
you have read and compiled. Do not install a prebuilt binary from someone you
do not trust.

Real enforcement needs an out-of-process or WASM runtime with a capability API.
That is a future goal, not a current property — the design is tracked in
`plan.md` §9.1.

## Manifest

Ship a `plugin.json` next to your compiled library:

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "What it does",
  "author": "You",
  "permissions": ["asr", "output"],
  "entry": "my_plugin.dll"
}
```

- `permissions` may include `asr`, `output`, `audio`, `dictionary`.
- `entry` is the shared library file name.

## The `echo-sdk` crate

Plugins compile against [`echo-sdk`](echo-app/src-tauri/echo-sdk) — the crate
that defines the `Plugin` trait, manifest types, and the `export_plugin!` macro.
Depend on it and implement `Plugin`:

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
echo-sdk = "0.1"
```

```rust
use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};

#[derive(Default)]
struct MyPlugin;

impl Plugin for MyPlugin {
    fn name(&self) -> &str { "my-plugin" }
    fn version(&self) -> &str { "1.0.0" }
    fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    fn on_unload(&self) -> PluginResult<()> { Ok(()) }
}

// Generates the `echo_plugin_create` FFI entry point the host looks up.
export_plugin!(MyPlugin);
```

`export_plugin!` emits the `echo_plugin_create` symbol that returns a
heap-allocated boxed trait object the host takes ownership of, so you never
write the `unsafe extern "C"` boilerplate by hand. Your plugin type must also
implement `Default`.

The capability traits `OutputPlugin` and `AudioPlugin` also live in `echo-sdk`;
`AsrPlugin` and `DictionaryPlugin` are defined by the host (they reference
Echo-internal types) in `echo-app/src-tauri/src/core/plugins/mod.rs`.

> **ABI note:** because the trait object crosses the dynamic-library boundary,
> plugins must be built against a matching `echo-sdk` version and the same Rust
> toolchain as the host.

## Installing

In the app: **Plugins** tab → **Install from file** → select your library
(the sibling `plugin.json` is read automatically). Toggle enable/disable or
uninstall from the same screen. Installed plugins are copied to the app data
directory under `plugins/<name>/`.
