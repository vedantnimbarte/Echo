# echo-sdk

The public API for writing [Echo](https://github.com/vedantnimbarte/Echo)
plugins.

Echo loads plugins as native shared libraries at runtime, so a plugin and the
host must share the exact same `Plugin` trait definition. That shared definition
lives in this crate — depend on `echo-sdk` and you are guaranteed to be
ABI-compatible with the host that declares the same version.

## Writing a plugin

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
echo-sdk = "0.2"
```

```rust
use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};

#[derive(Default)]
struct HelloPlugin;

impl Plugin for HelloPlugin {
    fn name(&self) -> &str { "hello" }
    fn version(&self) -> &str { "0.1.0" }
    fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    fn on_unload(&self) -> PluginResult<()> { Ok(()) }
}

export_plugin!(HelloPlugin);
```

`export_plugin!` generates the `echo_plugin_create` entry point the host looks
up after `dlopen`, plus an ABI marker the host checks first, and wraps your
plugin so a panic in a hook becomes an error instead of an aborted Echo. Your
plugin type must also implement `Default`.

## Capabilities

To do more than load, implement `AudioPlugin`, `AsrPlugin`, `DictionaryPlugin`
or `OutputPlugin`, and return `Some(self)` from the matching `as_*` method on
`Plugin` — that is how the host, which only holds a `dyn Plugin`, finds out.
Each trait's documentation says exactly when Echo calls it;
[PLUGINS.md](../../../PLUGINS.md) has the whole pipeline in order.

## Versions

**0.2 changed the `Plugin` vtable** (the `as_*` methods). A library built
against 0.1 is refused by an Echo speaking 0.2, and must be rebuilt; its code
needs no changes. `ABI_VERSION` is bumped, with the minor version, whenever a
trait's shape changes again.

## Shipping

Build the `cdylib` and place it next to a `plugin.json` manifest:

```json
{
  "name": "hello",
  "version": "0.1.0",
  "description": "Says hello.",
  "author": "you",
  "permissions": ["output"],
  "entry": "hello_plugin.dll"
}
```

Then install it from Echo's Plugins tab (or via the `install_plugin` command).

## Safety

Plugins run **in-process with full trust** — loading a native library is
inherently unsafe, and the `permissions` list is advisory in this version. Only
install plugins you trust. A sandboxed (e.g. WASM) runtime is a future goal.

## License

MIT
