# hello-echo

A complete, compiling [Echo](https://github.com/vedantnimbarte/Echo) plugin —
the smallest one that does something you can see afterwards.

This is the output of the scaffold behind **Plugins → Build one → Scaffold a
project**, checked in and built as part of the workspace so that a template
which stops compiling fails the test suite rather than somebody's afternoon.
Copy it, or generate your own from the app.

## What it does

`on_load` appends a line to `hello-echo.log` in the data directory Echo hands
the plugin, and `on_unload` does nothing. That is the whole plugin. It is
enough to prove the loader found your library, called your code, and gave you a
place to write.

## Build

```
cargo build --release
```

Inside this repository the crate depends on `echo-sdk` by path, so it always
matches the host. A plugin of your own outside the repo takes `echo-sdk = "0.1"`
from crates.io.

## The `entry` field is per-platform

`plugin.json` names the library file Echo stores and loads, and Cargo names that
file differently on each platform — replacing hyphens with underscores, and
adding the local prefix and extension:

| Platform | `cargo build --release` produces  | `entry`                |
| -------- | --------------------------------- | ---------------------- |
| Windows  | `target/release/hello_echo.dll`   | `hello_echo.dll`       |
| macOS    | `target/release/libhello_echo.dylib` | `libhello_echo.dylib` |
| Linux    | `target/release/libhello_echo.so` | `libhello_echo.so`     |

The committed `plugin.json` names the Windows artifact because that is where it
was generated. **Building on macOS or Linux, edit `entry` to match the table
before installing** — or scaffold a fresh project from the app, which fills it
in for the machine you are on.

## Install

Echo → **Plugins** → **Install from file** → pick the built library.

`plugin.json` has to sit in the same directory as the file you pick: Echo reads
the manifest from next to it. The simplest way is to copy `plugin.json` into
`target/release/` after building.

## Careful

A plugin is not sandboxed. It runs inside Echo with your account's privileges,
and the `permissions` list in the manifest is advisory — nothing enforces it.
See [PLUGINS.md](../../../../PLUGINS.md) for what that means in full.
