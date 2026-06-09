# Echo Plugins

Echo supports native plugins loaded as shared libraries (`.dll` / `.dylib` /
`.so`).

> ⚠️ **Experimental & trust model.** Plugins run **in-process with full trust**.
> The manifest permission list is **advisory** in this version — it is not yet
> enforced by a sandbox. Only install plugins you trust. True sandboxing (a WASM
> runtime) is a future goal.

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

## Export contract

A plugin library must export `echo_plugin_create`, returning a heap-allocated
boxed trait object the host takes ownership of:

```rust
// Build a cdylib that depends on Echo's plugin traits.
#[no_mangle]
pub extern "C" fn echo_plugin_create() -> *mut Box<dyn Plugin> {
    Box::into_raw(Box::new(Box::new(MyPlugin::default())))
}
```

The `Plugin` trait (and capability traits `AsrPlugin`, `OutputPlugin`,
`AudioPlugin`, `DictionaryPlugin`) are defined in
`echo-app/src-tauri/src/core/plugins/mod.rs`:

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &PluginContext) -> Result<()>;
    fn on_unload(&self) -> Result<()>;
}
```

> **ABI note:** because the trait object crosses the dynamic-library boundary,
> plugins must be built against the same Echo version/toolchain as the host.
> A dedicated `echo-sdk` crate (with an `#[echo_plugin]` macro and stable
> re-exports) is planned to make this ergonomic.

## Installing

In the app: **Plugins** tab → **Install from file** → select your library
(the sibling `plugin.json` is read automatically). Toggle enable/disable or
uninstall from the same screen. Installed plugins are copied to the app data
directory under `plugins/<name>/`.
