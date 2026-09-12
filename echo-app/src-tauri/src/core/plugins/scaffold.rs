//! Write a starter plugin project someone can build without typing any of it.
//!
//! The first plugin is the hard one: a `cdylib` crate type you have to know to
//! ask for, an `entry` in the manifest that has to match the artifact your
//! platform actually produces, and an FFI symbol you must not write by hand.
//! None of it is difficult and all of it is easy to get wrong once, which is
//! where most people stop. So Echo writes a project that already compiles, and
//! the guide explains what it wrote.
//!
//! The templates live here rather than in the command so they can be rendered
//! and asserted on without a running Tauri app.

use std::path::{Path, PathBuf};

use crate::error::{EchoError, Result};

/// Longest name we will write to disk. Not a filesystem limit — a name this
/// long is a mistake, and refusing early beats a confusing `create_dir` error.
const MAX_NAME: usize = 64;

/// Reject anything that is not a plain crate name.
///
/// This is the security boundary of the whole feature: the name becomes a path
/// segment under a directory the user picked, so `..`, `/`, `\`, a drive letter
/// or a NUL would let a scaffold land somewhere they did not choose. Allowing
/// only `[a-z][a-z0-9-]*` excludes every one of those by construction rather
/// than by blocklist, and is a subset of what Cargo accepts, so anything that
/// passes here is also a legal package name.
pub fn validate_name(name: &str) -> Result<()> {
    let bad = |why: &str| Err(EchoError::Plugin(format!("Plugin name {why}.")));

    if name.is_empty() {
        return bad("cannot be empty");
    }
    if name.len() > MAX_NAME {
        return bad(&format!("cannot be longer than {MAX_NAME} characters"));
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return bad("must start with a lowercase letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return bad("may only contain lowercase letters, digits and hyphens");
    }
    Ok(())
}

/// The library file name this platform's toolchain will actually produce.
///
/// Cargo turns hyphens into underscores for the library target, and every
/// platform decorates it differently. Guessing this wrong is the single most
/// common reason a hand-written `plugin.json` fails to install, so the manifest
/// is generated for the machine generating it rather than left to the author.
pub fn artifact_name(crate_name: &str) -> String {
    let lib = crate_name.replace('-', "_");
    if cfg!(target_os = "windows") {
        format!("{lib}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{lib}.dylib")
    } else {
        format!("lib{lib}.so")
    }
}

/// `hello-echo` becomes `HelloEcho`, so the generated struct reads like one
/// someone wrote.
fn type_name(crate_name: &str) -> String {
    crate_name
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// One file of the generated project.
pub struct ScaffoldFile {
    pub path: &'static str,
    pub contents: String,
}

/// Render every file of a starter project, without touching the disk.
pub fn render(name: &str) -> Result<Vec<ScaffoldFile>> {
    validate_name(name)?;
    let ty = type_name(name);
    let entry = artifact_name(name);

    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

# Echo loads a plugin with dlopen/LoadLibrary, so it has to be a C-ABI dynamic
# library. A plain Rust `lib` cannot be loaded at runtime.
[lib]
crate-type = ["cdylib"]

[dependencies]
echo-sdk = "0.1"
# Working inside a clone of the Echo repository instead? Depend on the crate
# directly and you are guaranteed the version the host was built against:
# echo-sdk = {{ path = "../echo-app/src-tauri/echo-sdk" }}
"#
    );

    let lib_rs = format!(
        r#"//! {name} — an Echo plugin.
//!
//! Echo calls `on_load` once when the plugin is enabled and `on_unload` when it
//! is disabled or Echo quits. This one writes a line into its own data
//! directory so there is something to look at after the first install.

use std::io::Write;

use echo_sdk::{{export_plugin, Plugin, PluginContext, PluginError, PluginResult}};

#[derive(Default)]
struct {ty};

impl Plugin for {ty} {{
    fn name(&self) -> &str {{
        "{name}"
    }}

    fn version(&self) -> &str {{
        "0.1.0"
    }}

    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {{
        // `ctx.data_dir` is the directory Echo hands you to keep files in.
        // Everything here returns a PluginError rather than panicking: a panic
        // crosses the library boundary into Echo and takes the app with it.
        std::fs::create_dir_all(&ctx.data_dir).map_err(|e| PluginError::new(e.to_string()))?;

        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ctx.data_dir.join("{name}.log"))
            .map_err(|e| PluginError::new(e.to_string()))?;

        writeln!(log, "{name} loaded").map_err(|e| PluginError::new(e.to_string()))?;
        Ok(())
    }}

    fn on_unload(&self) -> PluginResult<()> {{
        Ok(())
    }}
}}

// Emits `echo_plugin_create`, the one symbol Echo looks up after opening the
// library. Write it by hand and you own the unsafe; this macro does not.
export_plugin!({ty});
"#
    );

    let plugin_json = format!(
        r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "description": "An Echo plugin",
  "author": "",
  "permissions": [],
  "entry": "{entry}"
}}
"#
    );

    let readme = format!(
        r#"# {name}

An [Echo](https://github.com/vedantnimbarte/Echo) plugin.

## Build

```
cargo build --release
```

The library lands at `target/release/{entry}`, which is the name `plugin.json`
already expects.

## Install

Echo → **Plugins** → **Install from file** → pick `target/release/{entry}`.

`plugin.json` must sit next to the file you pick, so either install straight
from `target/release/` after copying the manifest there, or keep the pair
together somewhere of your own.

## Careful

A plugin is not sandboxed. It runs inside Echo with your account's privileges,
and the `permissions` list above is advisory — nothing enforces it.
"#
    );

    Ok(vec![
        ScaffoldFile {
            path: "Cargo.toml",
            contents: cargo_toml,
        },
        ScaffoldFile {
            path: "src/lib.rs",
            contents: lib_rs,
        },
        ScaffoldFile {
            path: "plugin.json",
            contents: plugin_json,
        },
        ScaffoldFile {
            path: "README.md",
            contents: readme,
        },
        ScaffoldFile {
            path: ".gitignore",
            contents: "/target\n".to_string(),
        },
    ])
}

/// Write a starter project into `parent/<name>/` and return the directory.
///
/// Refuses to write into a directory that already exists. Overwriting is how a
/// scaffold eats the plugin somebody has been working on for a week, and there
/// is no undo for that.
pub fn write(parent: &Path, name: &str) -> Result<PathBuf> {
    let files = render(name)?;
    let dir = parent.join(name);

    if dir.exists() {
        return Err(EchoError::Plugin(format!(
            "{} already exists. Pick another name, or another folder.",
            dir.display()
        )));
    }

    for file in &files {
        let path = dir.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| EchoError::Plugin(e.to_string()))?;
        }
        std::fs::write(&path, &file.contents).map_err(|e| EchoError::Plugin(e.to_string()))?;
    }

    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files of the example that read the same on every platform, and so can be
    /// held to the template byte for byte.
    const PORTABLE: [&str; 2] = ["src/lib.rs", ".gitignore"];

    fn example_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("plugin-examples")
            .join("hello-echo")
    }

    /// The whole point of the name rules: every one of these would otherwise
    /// put a directory somewhere the user did not pick.
    #[test]
    fn a_name_cannot_escape_the_folder_it_was_given() {
        for escape in [
            "..",
            "../evil",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "C:windows",
            "a\0b",
            ".hidden",
            "-leading-hyphen",
        ] {
            assert!(validate_name(escape).is_err(), "{escape} should be refused");
        }
    }

    #[test]
    fn a_plain_crate_name_is_accepted() {
        for ok in ["hello", "hello-echo", "plugin2", "a"] {
            assert!(validate_name(ok).is_ok(), "{ok} should be accepted");
        }
        assert!(
            validate_name("Hello").is_err(),
            "uppercase is not a crate name"
        );
        assert!(validate_name("").is_err());
        assert!(validate_name(&"a".repeat(MAX_NAME + 1)).is_err());
    }

    /// The manifest has to name the file the toolchain will really produce, so
    /// this is the one thing the author cannot check by reading the guide.
    #[test]
    fn the_manifest_entry_matches_this_platform_and_the_lib_target() {
        let entry = artifact_name("hello-echo");
        assert!(
            entry.contains("hello_echo"),
            "cargo replaces hyphens: {entry}"
        );

        if cfg!(target_os = "windows") {
            assert_eq!(entry, "hello_echo.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(entry, "libhello_echo.dylib");
        } else {
            assert_eq!(entry, "libhello_echo.so");
        }
    }

    #[test]
    fn the_generated_struct_reads_like_one_somebody_wrote() {
        assert_eq!(type_name("hello"), "Hello");
        assert_eq!(type_name("hello-echo"), "HelloEcho");
        assert_eq!(type_name("my-great-plugin"), "MyGreatPlugin");
    }

    /// The generated manifest is parsed by the same type the installer reads it
    /// with, so a scaffold that installs cleanly is not a coincidence.
    #[test]
    fn the_generated_manifest_is_one_the_installer_can_read() {
        let files = render("hello-echo").unwrap();
        let json = &files
            .iter()
            .find(|f| f.path == "plugin.json")
            .unwrap()
            .contents;
        let manifest: crate::core::plugins::PluginManifest = serde_json::from_str(json).unwrap();

        assert_eq!(manifest.name, "hello-echo");
        assert_eq!(manifest.entry, artifact_name("hello-echo"));
        assert!(manifest.permissions.is_empty());
    }

    #[test]
    fn writing_refuses_to_overwrite_what_is_already_there() {
        let tmp = std::env::temp_dir().join(format!("echo-scaffold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let dir = write(&tmp, "hello-echo").unwrap();
        assert!(dir.join("src/lib.rs").exists());
        assert!(dir.join("Cargo.toml").exists());

        // Second time round the work already there must survive.
        assert!(write(&tmp, "hello-echo").is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// `plugin-examples/hello-echo` is this template's output, checked in and
    /// built by the workspace — which is the only reason anyone can claim the
    /// snippets in the guide compile. If the template changes and the example
    /// does not, the committed example stops being evidence of anything.
    ///
    /// Only the platform-independent files are compared byte for byte.
    /// `Cargo.toml` is exempt because the example depends on the SDK by path
    /// while a real scaffold takes it from crates.io, and `plugin.json` and
    /// `README.md` are exempt because they name the artifact of whichever
    /// machine rendered them — comparing those would fail this test on every
    /// platform but the one the example was generated on.
    #[test]
    fn the_checked_in_example_is_what_this_template_produces() {
        let files = render("hello-echo").unwrap();

        for file in files.iter().filter(|f| PORTABLE.contains(&f.path)) {
            let on_disk = std::fs::read_to_string(example_dir().join(file.path))
                .unwrap_or_else(|e| panic!("{} is missing from the example: {e}", file.path));
            // Git may check out CRLF here; the comparison is about content.
            assert_eq!(
                on_disk.replace("\r\n", "\n"),
                file.contents,
                "plugin-examples/hello-echo/{} has drifted from the template in this file — re-render the example",
                file.path
            );
        }
    }

    /// The example's own manifest still has to be one the installer accepts,
    /// and has to name a library some platform really produces — the check the
    /// byte comparison above deliberately gives up.
    #[test]
    fn the_checked_in_example_names_a_real_library() {
        let json = std::fs::read_to_string(example_dir().join("plugin.json")).unwrap();
        let manifest: crate::core::plugins::PluginManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.name, "hello-echo");
        assert!(
            ["hello_echo.dll", "libhello_echo.dylib", "libhello_echo.so"]
                .contains(&manifest.entry.as_str()),
            "entry names no real cargo artifact: {}",
            manifest.entry
        );
    }

    #[test]
    fn a_refused_name_writes_nothing_at_all() {
        let tmp = std::env::temp_dir().join(format!("echo-scaffold-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        assert!(write(&tmp, "../escaped").is_err());
        // Nothing was created anywhere, including one level up.
        assert!(std::fs::read_dir(&tmp).unwrap().next().is_none());
        assert!(!tmp.parent().unwrap().join("escaped").exists());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
