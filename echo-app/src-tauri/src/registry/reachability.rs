/*!
 * SOURCE OF TRUTH KEYWORDS: reachability, every_setting_is_consumed,
 *   every_metric_is_recorded, SOURCE_ROOTS, KNOWN_UNREACHABLE, consumption_site
 * WHAT:  Asserts that every setting the registry declares is READ by something,
 *        and every latency stage it declares is RECORDED by something. Reads
 *        the crate's own source — and the frontend's — at test time and looks
 *        for a consumption site.
 * WHY:   The other guardrails check STRUCTURE: that a key is unique, that a
 *        default matches its kind, that two hotkeys do not collide. None of
 *        them check REACHABILITY, and a registry entry can pass every one of
 *        them while being read by nobody: it generates a control, the control
 *        saves, the value comes back, and the app's behaviour never changes.
 *        The user has told us something and been agreed with.
 *
 *        That is not hypothetical here. Writing this table turned up
 *        `history_retention_days` with a correct `apply_retention` behind it,
 *        and four separate `unwrap_or` defaults for `whisper_model` that could
 *        each have drifted from the others without anything failing. The class
 *        of bug is real and it is silent.
 *
 *        A grep-the-source test is crude, and crude is the point: it fails for
 *        a reason anyone can check by hand in ten seconds, and it cannot be
 *        satisfied by a mock. The alternative — trusting a reviewer to notice
 *        an absence — is what lets a dead setting ship.
 *
 *        BOTH TREES ARE SCANNED, and that is the one real difference from the
 *        app this is ported from. Some of Echo's settings are consumed only in
 *        TypeScript — `pill_size` is read by the pill webview, `ui_language` by
 *        the locale switcher — and a Rust-only scan would call them dead and be
 *        wrong. A setting is reachable if ANY layer reads it.
 * WHERE: Compiled into the crate's tests by registry/mod.rs.
 */

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{all_settings, capabilities, LatencyStage};

/// The trees a consumption site may live in, relative to the crate root.
///
/// `../src` is the frontend. Scanning it is what makes a TypeScript-only
/// setting count as reachable — see the module WHY.
const SOURCE_ROOTS: [&str; 2] = ["src", "../src"];

/// Files that DECLARE rather than consume. A key appearing only here proves
/// nothing: the registry naming its own setting is not somebody reading it.
fn is_declaration_site(path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    p.contains("/registry/")
}

/// Extensions worth reading. Anything else is an asset.
fn is_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("rs") | Some("ts") | Some("tsx")
    )
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Neither is source, and both are enormous.
        if name == "node_modules" || name == "target" || name == "dist" {
            continue;
        }
        if path.is_dir() {
            collect(&path, out);
        } else if is_source(&path) && !is_declaration_site(&path) {
            out.push(path);
        }
    }
}

/// Every consuming source file, with its text, read once and shared by both
/// tests — the tree is a few hundred files and reading it twice is the
/// difference between a fast test and one people start skipping.
fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    for tree in SOURCE_ROOTS {
        collect(&root.join(tree), &mut paths);
    }
    paths
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)))
        .collect()
}

/// Where a needle is read, if anywhere. The quotes matter: a bare substring
/// search would match `history_enabled` inside `history_enabled_at` and call a
/// dead setting live.
fn consumption_site<'a>(sources: &'a [(PathBuf, String)], needle: &str) -> Option<&'a Path> {
    let quoted = [format!("\"{needle}\""), format!("'{needle}'")];
    sources
        .iter()
        .find(|(_, text)| quoted.iter().any(|q| text.contains(q.as_str())))
        .map(|(path, _)| path.as_path())
}

/**
 * SOURCE OF TRUTH KEYWORDS: every_setting_is_consumed
 * WHAT:  Every declared setting is read by at least one file that is not the
 *        registry itself.
 * WHY:   A setting nothing reads is a control that lies to the user. See the
 *        module WHY.
 */
#[test]
fn every_setting_is_consumed() {
    let sources = sources();
    let mut dead = Vec::new();

    for setting in all_settings() {
        if consumption_site(&sources, &setting.key).is_none() {
            dead.push(setting.key.clone());
        }
    }

    assert!(
        dead.is_empty(),
        "these settings are declared and read by nothing, so changing them does nothing:\n  {}\n\
         Either wire each one up, or delete it from the registry. Do not add it to an \
         exception list — an exception list is how a dead setting becomes permanent.",
        dead.join("\n  ")
    );
}

/**
 * SOURCE OF TRUTH KEYWORDS: every_metric_is_recorded, KNOWN_UNREACHABLE
 * WHAT:  Every declared latency stage is recorded by something.
 * WHY:   A stage nothing writes is a panel that reads empty forever, and the
 *        panel cannot tell that apart from "you have not dictated yet". Both
 *        look like no data; only one is a bug.
 * WHERE: The stages are written by core/telemetry/latency.rs.
 */
#[test]
fn every_metric_is_recorded() {
    let sources = sources();
    let mut dead = Vec::new();

    for capability in capabilities() {
        for metric in &capability.metrics {
            if KNOWN_UNREACHABLE.contains(&metric.stage) {
                continue;
            }
            // Recorded by naming the enum variant, which is what makes this
            // greppable at all — a stage recorded through a variable would be
            // invisible here, and that is a reason not to record one that way.
            let variant = format!("LatencyStage::{:?}", metric.stage);
            if !sources.iter().any(|(_, text)| text.contains(&variant)) {
                dead.push(format!("{:?} ({})", metric.stage, capability.key.as_str()));
            }
        }
    }

    assert!(
        dead.is_empty(),
        "these latency stages are declared and recorded by nothing:\n  {}",
        dead.join("\n  ")
    );
}

/**
 * SOURCE OF TRUTH KEYWORDS: KNOWN_UNREACHABLE
 * WHAT:  Stages declared ahead of the code that will record them.
 * WHY:   THIS LIST IS A DEBT, NOT AN EXEMPTION, and it is EMPTY — which is the
 *        state it is supposed to be in. It was briefly populated while the
 *        latency recorder was being built, and the stages that still had no
 *        recording site when it landed were deleted from the registry rather
 *        than parked here. That is the rule: if this list stops shrinking, the
 *        feature stopped being built, and the honest move is to remove the
 *        undelivered stages.
 * WHERE: Consulted by every_metric_is_recorded.
 */
const KNOWN_UNREACHABLE: &[LatencyStage] = &[];

/// The scan is only meaningful if it is actually reading files. A typo in a
/// root, a moved crate, a renamed directory — any of those would empty the
/// corpus and turn both tests above into unconditional passes, which is the
/// worst failure mode a guardrail has.
#[test]
fn the_scan_finds_source_to_scan() {
    let sources = sources();
    assert!(
        sources.len() > 50,
        "only {} source files found — the reachability scan is looking in the wrong place, \
         and until that is fixed both tests above pass for the wrong reason",
        sources.len()
    );

    let has_rust = sources
        .iter()
        .any(|(p, _)| p.extension().is_some_and(|e| e == "rs"));
    let has_ts = sources
        .iter()
        .any(|(p, _)| p.extension().is_some_and(|e| e == "ts" || e == "tsx"));
    assert!(has_rust, "no Rust source found");
    assert!(
        has_ts,
        "no frontend source found — a TypeScript-only setting would read as dead"
    );
}

/// Keys are matched by exact quoted string, so a key that is a prefix of
/// another must not borrow its consumption site. This is the check on the
/// checker.
#[test]
fn matching_is_not_a_substring_match() {
    let sources: Vec<(PathBuf, String)> = vec![(
        PathBuf::from("fake.rs"),
        "let x = get(\"history_enabled_at\");".to_string(),
    )];
    assert!(consumption_site(&sources, "history_enabled_at").is_some());
    assert!(
        consumption_site(&sources, "history_enabled").is_none(),
        "a prefix matched a longer key's site, so a dead setting would read as live"
    );

    let mut seen = HashSet::new();
    for setting in all_settings() {
        seen.insert(setting.key.as_str());
    }
    assert!(seen.contains("history_enabled"));
}
