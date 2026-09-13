# Echo Plugins

Echo supports native plugins loaded as shared libraries (`.dll` / `.dylib` /
`.so`).

**In the app:** *Plugins → Build one* walks through the same material with
copy-paste snippets, and will scaffold a project that already compiles. This
file is the reference; that is the walkthrough.

**A working example:** [`echo-app/src-tauri/plugin-examples/hello-echo`](echo-app/src-tauri/plugin-examples/hello-echo)
is the scaffold's own output, checked in and built as part of the workspace — so
the snippets below are known to compile rather than assumed to.

## What runs, and when

Echo opens your library, calls `on_load` when the plugin is enabled, and calls
`on_unload` when it is disabled or Echo quits. Your plugin gets a data
directory. **Only enabled plugins take part in anything below**: disabling one
unloads it, and every stage reads the loaded set as it goes, so it stops being
called mid-recording too.

A plugin does more by implementing a capability trait from `echo-sdk` and
saying so. The host holds your plugin as a `Box<dyn Plugin>`, and Rust cannot
downcast a trait object to another trait, so each capability has a method on
`Plugin` that defaults to `None`:

```rust
fn as_output(&self) -> Option<&dyn OutputPlugin> { Some(self) }
```

Without that line Echo never calls the trait, however it is implemented.

In the order a spoken sentence meets them:

| # | Capability | Called | Can it change the result? |
| - | ---------- | ------ | ------------------------- |
| 1 | `AudioPlugin::process` | Every captured chunk (16 kHz mono `f32`, tens of ms), before voice detection. | Yes — detection and the decoder hear what you return. |
| 2 | `AsrPlugin::transcribe` | Each whole utterance, **only if the user selects your engine**, listed as `plugin:<name>`. | It *is* the result. On failure the offline engine retries the utterance. |
| 3 | `DictionaryPlugin::entries` | When the plugin is enabled and whenever the dictionary changes — not per transcript. | Yes, through the dictionary: your entries apply after the user's own, so theirs win. |
| 4 | `OutputPlugin::on_transcript` | Each delivered transcript, **after** it has been typed, on a background thread, in order. | No. It observes. |

Two of those deserve their reasons:

- **Output observes rather than transforms.** Delivering the user's words is the
  one thing that must not fail, and a hook able to rewrite or swallow them puts
  every sentence at the mercy of the least careful plugin installed. So it runs
  after the text is typed, on its own thread: a slow webhook never delays
  typing, and a crash cannot take the words back. It is not sent transcripts
  from an app whose history the user turned off — a plugin that writes text
  down is history by another name — nor anything the password guard, "scratch
  that" or a failed command-mode run withheld. It receives the text, the focused
  app's id, and the language.
- **Audio is the hot path.** It runs for every chunk while the microphone is
  open, so the stage only exists when some enabled plugin offers it, Echo logs
  what it cost after each recording, and a hook that errors (or returns an empty
  buffer, which downstream means "microphone stopped") has that chunk's changes
  thrown away and is not called again until the plugin is re-enabled.

### Errors and panics

Return `PluginError` from a hook and Echo logs it and carries on without that
plugin's contribution. A panic is contained too, but not by Echo: your library
has its own copy of the Rust standard library, and a panic from one copy cannot
be caught by another — the process would abort. So `export_plugin!` wraps your
plugin in `echo_sdk::Guarded`, which catches the panic *inside your library* and
hands Echo an error instead. That relies on unwinding, Cargo's default; build
with `panic = "abort"` and a panic takes Echo with it.

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

### What Echo does enforce: the file cannot change underneath you

Consent is worth little if the thing you consented to can be swapped afterwards.
So Echo records a SHA-256 of the library when you install it, and checks it on
every load. **If the file has changed, the plugin is disabled instead of
loaded**, and the log says so with both fingerprints.

That closes a real hole — anything able to write into the plugins directory
(another program, a sync client, an installer) could otherwise replace a plugin
you vetted and inherit Echo's privileges on the next launch, without a word.

It is *not* a sandbox, and it does not make the permission list enforceable.
Once your trusted plugin's code runs, it can still do everything in the list
above. What it guarantees is narrower and worth stating exactly: **the code
running is the code you agreed to run.**

A plugin installed before fingerprints existed adopts its current hash on the
next load rather than being locked out — the file on disk is the one you have
been running all along.

### Why there is no sandbox

A real boundary means the OS enforcing it: a child process with a restricted
token or seccomp filter, or a WASM runtime. Either replaces this FFI with a wire
protocol, because a trait object cannot cross a process boundary — audio,
transcripts and dictionary entries would have to be serialised, and every
existing plugin would need rewriting.

That is a project, not a patch, and pretending otherwise is how a checkbox ends
up in Settings with nothing behind it.

**Practical advice:** install plugins you have built yourself or whose source
you have read and compiled. Do not install a prebuilt binary from someone you
do not trust.

Real enforcement needs an out-of-process or WASM runtime with a capability API.
That is a future goal, not a current property.

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

- `permissions` may include `asr`, `output`, `audio`, `dictionary`. It is what
  the user is shown before installing, not what decides the calls — that is the
  `as_*` methods — so keep the two in agreement.
- `entry` is the shared library file name — **the one field people get wrong.**
  Cargo replaces hyphens with underscores and decorates the name per platform,
  so a crate called `my-plugin` builds to `my_plugin.dll` on Windows,
  `libmy_plugin.dylib` on macOS and `libmy_plugin.so` on Linux. `plugin.json` is
  therefore not portable as written; a scaffolded project has it right for the
  machine that generated it.

## The `echo-sdk` crate

Plugins compile against [`echo-sdk`](echo-app/src-tauri/echo-sdk) — the crate
that defines the `Plugin` trait, manifest types, and the `export_plugin!` macro.
Depend on it and implement `Plugin`:

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
write the `unsafe extern "C"` boilerplate by hand. It also wraps your type in
`Guarded` (see *Errors and panics* above) and emits `echo_plugin_abi_version`.
Your plugin type must also implement `Default`.

All four capability traits — `AudioPlugin`, `AsrPlugin`, `DictionaryPlugin`,
`OutputPlugin` — and the plain data types they exchange (`Transcript`,
`Transcription`, `DictionaryEntry`) live in `echo-sdk`. Nothing a plugin
implements refers to a type inside Echo.

### ABI and versions

The trait object crosses the dynamic-library boundary as a raw vtable, so both
sides must agree on the trait's exact shape. Adding the `as_*` methods changed
that shape: **a plugin built against echo-sdk 0.1 must be rebuilt against 0.2**
(its code needs no changes unless it wants a capability).

A mismatch is not left to chance. `echo_sdk::ABI_VERSION` is compiled into your
library behind a plain C function, and Echo reads it before calling anything
else in the library. A library without the marker (echo-sdk 0.1) or with a
different number is refused at install or enable, with a message saying to
rebuild. The marker is compiled in rather than read from `plugin.json`, because
a hand-edited manifest can claim a version the binary was never built with.

What Echo cannot detect is a different **Rust toolchain**: build with the same
Rust version and target as the Echo you install into.

## Installing

In the app: **Plugins → Installed → Install from file** → select your library
(the sibling `plugin.json` is read automatically, so the two must be in the same
directory). Toggle enable/disable or uninstall from the same screen. Installed
plugins are copied to the app data directory under `plugins/<name>/`.

Changing a plugin means building and installing again. The fingerprint above is
checked on every load, so a library edited underneath Echo is disabled rather
than loaded — including when the edit was your own rebuild.
