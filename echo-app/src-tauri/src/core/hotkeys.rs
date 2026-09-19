/*!
 * SOURCE OF TRUTH KEYWORDS: DEFAULT_HOTKEY, DEFAULT_MODE, DEFAULT_UNDO_HOTKEY,
 *   DEFAULT_RETRY_HOTKEY, UNBOUND
 * WHAT:  The accelerators and recording mode Echo ships with.
 * WHY:   These live in core rather than in commands/hotkey.rs, where they used
 *        to, because the REGISTRY needs them — it declares them as the defaults
 *        for the `hotkey` and `recording_mode` settings — and the registry sits
 *        below commands in the dependency order. A constant reached for by both
 *        a low layer and a high one belongs in the low one; the alternative was
 *        an upward import, which layering.rs now fails the build over.
 * WHERE: Declared as registry defaults in registry/mod.rs; bound by
 *        commands/hotkey.rs.
 */

/// Default global hotkey used when none is configured.
pub const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+Space";

/// Default recording mode. Historically `"manual"`, which meant this.
pub const DEFAULT_MODE: &str = "toggle";

/// Take back the last insert. Global, because by the time you notice the
/// mistake the focus is in the app that received the text.
///
/// Alt rather than Shift: `Ctrl+Shift+Z` is *redo* in most editors, and a
/// global binding would take it away from every app on the machine.
pub const DEFAULT_UNDO_HOTKEY: &str = "CommandOrControl+Alt+Z";

/// Re-decode the last utterance on a stronger model.
pub const DEFAULT_RETRY_HOTKEY: &str = "CommandOrControl+Alt+R";

/// Stored in place of an accelerator to leave a fix-up unbound. A global
/// shortcut is taken from every other app on the machine, so being able to give
/// one back matters more here than for the dictation hotkey.
pub const UNBOUND: &str = "off";
