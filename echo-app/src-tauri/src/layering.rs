/*!
 * SOURCE OF TRUTH KEYWORDS: layering, nothing_imports_upward, LAYER_ORDER,
 *   KNOWN_UPWARD_IMPORTS, layer_of
 * WHAT:  Enforces the dependency direction: infrastructure, then contracts,
 *        then implementations, then the boundary. Nothing imports from a layer
 *        above its own.
 * WHY:   Without a check, the direction is a thing people mean rather than a
 *        thing that holds, and it decays in the ordinary way — someone needs a
 *        constant, the constant lives one layer up, and reaching for it is one
 *        line while moving it is twenty.
 *
 *        That happened during this very change. The registry needed
 *        DEFAULT_HOTKEY, which lived in commands/hotkey.rs — two layers up. The
 *        one-line fix was `use crate::commands::hotkey::DEFAULT_HOTKEY` and it
 *        compiled perfectly well; the right fix was moving the constants down
 *        into core/hotkeys.rs, which is what happened instead, because this
 *        test existed to refuse the shortcut. That is the entire argument for
 *        it: the wrong version compiles.
 *
 *        Crude on purpose — it reads `use crate::x` lines rather than building
 *        a real module graph, so it misses fully-qualified paths written
 *        inline, and it will keep missing them. It catches the shape violations
 *        actually take, it fails for a reason anyone can check in ten seconds,
 *        and it cannot be satisfied by a mock.
 * WHERE: Compiled into the crate's tests by lib.rs.
 */

use std::path::{Path, PathBuf};

/**
 * SOURCE OF TRUTH KEYWORDS: LAYER_ORDER
 * WHAT:  The layers, lowest first. A module may import from its own layer or
 *        any below it, never above.
 * WHY:   This order is DESCRIPTIVE — it was derived from the imports the crate
 *        actually has, not asserted and then enforced. Writing down an order
 *        the code does not follow produces a test that fails on day one and is
 *        deleted on day two.
 *
 *          error     — no dependencies at all; everything may use it.
 *          storage   — SQLite and the keychain. Knows nothing about audio.
 *          core      — the domain: capture, engines, formatting, injection.
 *          registry  — reads core's constants to declare what the app has.
 *          platform  — OS specifics, beside registry because neither uses the
 *                      other.
 *          ipc       — the command factory. Needs the registry to preflight.
 *          state     — the shared handle, which holds one of everything below.
 *          commands  — handlers. Allowed to reach anything.
 *          boundary  — lib/tray/cli/benchmark/selftest: wiring, and the top.
 * WHERE: Read by nothing_imports_upward.
 */
const LAYER_ORDER: &[&[&str]] = &[
    &["error"],
    &["storage"],
    &["core"],
    &["registry", "platform"],
    &["ipc"],
    &["state"],
    &["commands"],
    &[
        "tray",
        "cli",
        "benchmark",
        "selftest",
        "lib",
        "main",
        // Test-only modules. They sit at the top because a test may reach for
        // anything, and because a guardrail that had to be placed below what it
        // inspects would be the tail wagging the dog.
        "layering",
        "pipeline_tests",
    ],
];

/**
 * SOURCE OF TRUTH KEYWORDS: KNOWN_UPWARD_IMPORTS
 * WHAT:  Violations that exist and are not being fixed right now.
 * WHY:   THIS LIST IS A DEBT, NOT AN EXEMPTION, and it is empty. It is kept
 *        because the alternative — deleting the mechanism — means the first
 *        violation someone genuinely cannot fix today gets solved by deleting
 *        the test instead. An entry here is a decision with a name on it; no
 *        entry at all is a decision nobody made.
 * WHERE: Consulted by nothing_imports_upward.
 */
const KNOWN_UPWARD_IMPORTS: &[(&str, &str)] = &[];

/// Which layer a path belongs to, by its top-level module.
fn layer_of(path: &Path) -> Option<(usize, String)> {
    let normalised = path.to_string_lossy().replace('\\', "/");
    let after_src = normalised.split("/src/").nth(1)?;
    let head = after_src.split('/').next()?;
    let module = head.trim_end_matches(".rs").to_string();

    LAYER_ORDER
        .iter()
        .position(|layer| layer.contains(&module.as_str()))
        .map(|rank| (rank, module))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The module a `use crate::...` line points at.
fn imported_module(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("use crate::")?;
    let module = rest
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()?;
    (!module.is_empty()).then_some(module)
}

#[test]
fn nothing_imports_upward() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    rust_files(&root, &mut paths);

    assert!(
        paths.len() > 50,
        "only {} Rust files found — the scan is looking in the wrong place, and until that \
         is fixed this test passes for the wrong reason",
        paths.len()
    );

    let mut violations = Vec::new();

    for path in &paths {
        let Some((rank, module)) = layer_of(path) else {
            panic!(
                "{} is not in any declared layer. Add its module to LAYER_ORDER — deciding \
                 where a new top-level module sits is the point of the list.",
                path.display()
            );
        };

        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };

        for line in text.lines() {
            let Some(target) = imported_module(line) else {
                continue;
            };
            let Some(target_rank) = LAYER_ORDER.iter().position(|l| l.contains(&target)) else {
                continue;
            };

            if target_rank > rank
                && !KNOWN_UPWARD_IMPORTS.contains(&(module.as_str(), target))
            {
                violations.push(format!(
                    "{}: {} (layer {}) imports {} (layer {})",
                    path.display(),
                    module,
                    rank,
                    target,
                    target_rank
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "these imports point upward, against the dependency direction:\n  {}\n\
         Move what is being reached for DOWN to the layer that needs it, rather than \
         reaching up for it. See core/hotkeys.rs for the worked example.",
        violations.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parser is the whole test, so its edges are worth pinning.
    #[test]
    fn import_lines_are_read_correctly() {
        assert_eq!(imported_module("use crate::core::audio;"), Some("core"));
        assert_eq!(
            imported_module("    use crate::registry::{self, CapabilityKey};"),
            Some("registry")
        );
        assert_eq!(imported_module("use crate::error::Result;"), Some("error"));
        // Not a crate-relative import, so not this test's business.
        assert_eq!(imported_module("use std::path::Path;"), None);
        assert_eq!(imported_module("use super::thing;"), None);
    }

    #[test]
    fn a_path_maps_to_its_top_level_module() {
        let (rank, module) = layer_of(Path::new("/x/src-tauri/src/core/asr/local.rs")).unwrap();
        assert_eq!(module, "core");
        assert_eq!(rank, 2);

        let (_, module) = layer_of(Path::new("/x/src-tauri/src/error.rs")).unwrap();
        assert_eq!(module, "error");
    }
}
