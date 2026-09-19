/*!
 * SOURCE OF TRUTH KEYWORDS: execute, CommandSpec, Reentrancy, ResourceUse,
 *   Validate, preflight_permissions, ExclusiveGuard, permission_state
 * WHAT:  The command factory. Every IPC command runs through `execute`, which
 *        validates the input, preflights permissions, guards reentrancy, opens
 *        a tracing span, runs the handler and maps whatever comes back to an
 *        EchoError.
 * WHY:   This exists so those five concerns have ONE implementation instead of
 *        one per handler. A permission check written twenty times is a
 *        permission check that is wrong in three places; a validation step a
 *        handler can forget is a validation step that will be forgotten.
 *
 *        Echo had the second problem already. `inject_text` opened with its own
 *        `if text.is_empty() { return Ok(()) }`, `start_recording` had no
 *        reentrancy guard beyond a bare `Mutex<bool>` that callers had to
 *        remember to consult, and nothing tied a command to the OS grant its
 *        feature needs — the accessibility check was a command the FRONTEND had
 *        to remember to call first.
 *
 *        The corollary is the rule that matters when writing a handler: do NOT
 *        re-check any of it. If a handler is checking a permission, the
 *        capability's `requires` list in the registry is wrong, and that is
 *        where the fix belongs.
 * WHERE: Called by the commands in commands/. Reads capability metadata from
 *        registry/.
 */

use std::collections::HashSet;
use std::future::Future;
use std::sync::Mutex;

use crate::error::{EchoError, Result};
use crate::registry::{self, CapabilityKey, OsPermission};

/**
 * SOURCE OF TRUTH KEYWORDS: Validate
 * WHAT:  The contract every command input implements.
 * WHY:   Rust has no Zod, so validation is a trait the factory calls rather
 *        than a schema it interprets. The effect is the same and the guarantee
 *        is stronger: an input type that does not implement this cannot be
 *        passed to `execute` at all, so "someone forgot to validate" is a
 *        compile error rather than a code review.
 * WHERE: Implemented by command input types in commands/.
 */
pub trait Validate {
    /// Return a user-facing reason. The factory turns it into an EchoError.
    fn validate(&self) -> std::result::Result<(), String>;
}

/// Inputs with nothing to check still opt in explicitly, so the absence of a
/// check is a decision someone made rather than one nobody made.
impl Validate for () {
    fn validate(&self) -> std::result::Result<(), String> {
        Ok(())
    }
}

/// A non-empty string, which is most of what Echo's commands take.
impl Validate for String {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.trim().is_empty() {
            return Err("That cannot be empty.".into());
        }
        Ok(())
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: Reentrancy
 * WHAT:  Whether concurrent calls to this command are allowed.
 * WHY:   Reads are safe to overlap and should not be serialised — a settings
 *        window opening three panes at once must not queue. Anything that
 *        mutates recording state is Exclusive, which is what makes a
 *        double-fired hotkey harmless rather than a second capture racing the
 *        first for the microphone.
 * WHERE: Declared per command in its CommandSpec.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reentrancy {
    /// One at a time per capability.
    Exclusive,
    /// Overlapping calls are fine.
    Concurrent,
}

/**
 * SOURCE OF TRUTH KEYWORDS: ResourceUse
 * WHAT:  Whether a command actually touches the OS resources its capability
 *        declares, or merely reports on them.
 * WHY:   A capability's `requires` list describes what the FEATURE needs — the
 *        microphone, for dictation. It does not follow that every command on
 *        that capability needs it. `is_recording` only reads a flag, and
 *        preflighting it against the microphone would stop the pill rendering
 *        its own idle state until permission was granted: the app looks broken
 *        in exactly the moment it is trying to explain how to fix it.
 *
 *        So the requirement is enforced where the resource is USED. Acquires is
 *        the default, because defaulting to "no permission needed" would let a
 *        new command quietly skip a check nobody notices is missing.
 * WHERE: Declared per command in its CommandSpec; read by preflight_permissions.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceUse {
    /// Uses the capability's OS resources. Preflight applies. The default.
    Acquires,
    /// Only reports on them. No OS resource is touched, so no grant is needed.
    Reports,
}

/**
 * SOURCE OF TRUTH KEYWORDS: CommandSpec
 * WHAT:  The declaration a command hands the factory.
 * WHERE: One per command function in commands/.
 */
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Used in the tracing span and in errors.
    pub name: &'static str,
    /// Which registry entry governs this command's permissions.
    pub capability: CapabilityKey,
    pub reentrancy: Reentrancy,
    pub resource_use: ResourceUse,
}

impl CommandSpec {
    pub const fn new(name: &'static str, capability: CapabilityKey) -> Self {
        Self {
            name,
            capability,
            reentrancy: Reentrancy::Concurrent,
            resource_use: ResourceUse::Acquires,
        }
    }

    pub const fn exclusive(mut self) -> Self {
        self.reentrancy = Reentrancy::Exclusive;
        self
    }

    /// This command only reads state; it needs no OS grant. See ResourceUse.
    pub const fn reports(mut self) -> Self {
        self.resource_use = ResourceUse::Reports;
        self
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: ExclusiveRegistry, ExclusiveGuard, begin_exclusive
 * WHAT:  The set of capabilities currently running an exclusive command, and
 *        the guard that releases one.
 * WHY:   Keyed by capability rather than by command name, because the clash
 *        being prevented is two commands fighting over the same resource, not
 *        one command running twice: `start_recording` and `stop_recording` must
 *        not interleave any more than two `start_recording`s must.
 *
 *        The guard releases on Drop because the paths that would otherwise leak
 *        a claim are the error paths — and a capability that stays claimed
 *        after a failed call is a feature that is dead until restart.
 * WHERE: Owned by AppState; claimed by execute for the life of one call.
 */
#[derive(Default)]
pub struct ExclusiveRegistry {
    held: Mutex<HashSet<CapabilityKey>>,
}

impl ExclusiveRegistry {
    /// Claims `key`, or returns None if something else already holds it.
    pub fn begin(&self, key: CapabilityKey) -> Option<ExclusiveGuard<'_>> {
        let mut held = self.held.lock().ok()?;
        if held.insert(key) {
            Some(ExclusiveGuard {
                registry: self,
                key,
            })
        } else {
            None
        }
    }
}

pub struct ExclusiveGuard<'a> {
    registry: &'a ExclusiveRegistry,
    key: CapabilityKey,
}

impl Drop for ExclusiveGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut held) = self.registry.held.lock() {
            held.remove(&self.key);
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: permission_state
 * WHAT:  Whether the OS currently grants one permission.
 * WHY:   One place that answers the question, so a command never has to know
 *        which platform API holds the answer. Microphone returns true on every
 *        platform because Echo cannot ask ahead of time anywhere it runs — the
 *        OS answers by failing the stream open, which the audio layer already
 *        turns into a typed error. Claiming otherwise here would be a preflight
 *        that lies in the reassuring direction, which is the worse one.
 * WHERE: Called by preflight_permissions.
 */
fn permission_state(permission: OsPermission) -> bool {
    match permission {
        // See the WHY: not knowable before the stream is opened.
        OsPermission::Microphone => true,
        OsPermission::Accessibility => crate::commands::injection::check_accessibility_permission(),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: preflight_permissions
 * WHAT:  Rejects a call whose capability needs an OS grant the app does not
 *        have, before the handler runs.
 * WHY:   A missing grant becomes a typed, actionable error rather than a
 *        failure deep inside an adapter that the UI can only render as "it
 *        didn't work".
 * WHERE: Step 2 of execute.
 */
fn preflight_permissions(capability: CapabilityKey) -> Result<()> {
    for permission in &registry::capability(capability).requires {
        if !permission_state(*permission) {
            return Err(EchoError::PermissionDenied(match permission {
                OsPermission::Microphone => {
                    "Echo needs permission to use the microphone.".to_string()
                }
                OsPermission::Accessibility => {
                    "Echo needs accessibility permission to type into other apps. Until then \
                     the transcript goes to the clipboard."
                        .to_string()
                }
            }));
        }
    }
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: execute
 * WHAT:  Runs one command through every cross-cutting concern.
 * WHY:   The order is deliberate and each step depends on the one before it:
 *          1. Validate  — an invalid input must never reach a handler, and must
 *                         never cost a permission prompt or a lock.
 *          2. Preflight — a missing OS grant becomes a typed, actionable error
 *                         rather than a failure deep inside an adapter.
 *          3. Guard     — claimed only after we know the call is legal, so an
 *                         invalid call cannot lock out a valid one.
 *          4. Handler   — business logic, and nothing else.
 *          5. Trace     — one place that knows whether a command succeeded.
 * WHERE: Called by every command that has been moved onto the factory.
 */
pub async fn execute<I, O, F, Fut>(
    exclusive: &ExclusiveRegistry,
    spec: CommandSpec,
    input: I,
    handler: F,
) -> Result<O>
where
    I: Validate,
    F: FnOnce(I) -> Fut,
    Fut: Future<Output = Result<O>>,
{
    let span = tracing::info_span!(
        "command",
        name = spec.name,
        capability = spec.capability.as_str(),
    );
    let _entered = span.enter();

    // 1. Validation. The input's own schema is the source of truth.
    if let Err(reason) = input.validate() {
        tracing::warn!(reason, "input rejected");
        return Err(EchoError::InvalidInput(reason));
    }

    // 2. Permission preflight, straight from the registry declaration — but
    // only for commands that actually use the resource. See ResourceUse.
    if spec.resource_use == ResourceUse::Acquires {
        preflight_permissions(spec.capability)?;
    }

    // 3. Reentrancy. Held for the life of the call, released on any exit.
    let _guard = match spec.reentrancy {
        Reentrancy::Exclusive => match exclusive.begin(spec.capability) {
            Some(guard) => Some(guard),
            None => {
                tracing::debug!("rejected a reentrant call");
                return Err(EchoError::AlreadyInProgress(
                    "That is already in progress.".into(),
                ));
            }
        },
        Reentrancy::Concurrent => None,
    };

    // 4. The handler. Only what is specific to this task.
    let started = std::time::Instant::now();
    let result = handler(input).await;

    // 5. One place that knows how a command ended.
    match &result {
        Ok(_) => tracing::debug!(elapsed_ms = started.elapsed().as_millis() as u64, "ok"),
        Err(e) => tracing::warn!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            code = ?e.code(),
            error = %e,
            "failed"
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysInvalid;
    impl Validate for AlwaysInvalid {
        fn validate(&self) -> std::result::Result<(), String> {
            Err("no".into())
        }
    }

    #[tokio::test]
    async fn an_invalid_input_never_reaches_the_handler() {
        let exclusive = ExclusiveRegistry::default();
        let spec = CommandSpec::new("test", CapabilityKey::Settings);

        let result = execute(&exclusive, spec, AlwaysInvalid, |_| async {
            panic!("handler ran despite invalid input");
            #[allow(unreachable_code)]
            Ok::<(), EchoError>(())
        })
        .await;

        assert!(matches!(result, Err(EchoError::InvalidInput(_))));
    }

    /// The whole point of step 3: a second call is refused while the first is
    /// still running, so a double-fired hotkey cannot start two captures.
    #[tokio::test]
    async fn an_exclusive_capability_refuses_a_second_call() {
        let exclusive = ExclusiveRegistry::default();
        let guard = exclusive
            .begin(CapabilityKey::Dictation)
            .expect("first claim");

        let spec = CommandSpec::new("test", CapabilityKey::Dictation)
            .exclusive()
            .reports();
        let result = execute(&exclusive, spec, (), |_| async { Ok::<(), EchoError>(()) }).await;

        assert!(matches!(result, Err(EchoError::AlreadyInProgress(_))));
        drop(guard);

        // And released, the same call goes through — the guard must not leak.
        let result = execute(&exclusive, spec, (), |_| async { Ok::<(), EchoError>(()) }).await;
        assert!(result.is_ok());
    }

    /// The error paths are the ones that leak a claim, so this is the case
    /// worth pinning: a capability left claimed after a failure is a feature
    /// that stays dead until the app restarts.
    #[tokio::test]
    async fn a_failing_handler_still_releases_its_claim() {
        let exclusive = ExclusiveRegistry::default();
        let spec = CommandSpec::new("test", CapabilityKey::Dictation)
            .exclusive()
            .reports();

        let result = execute(&exclusive, spec, (), |_| async {
            Err::<(), _>(EchoError::Config("boom".into()))
        })
        .await;
        assert!(result.is_err());

        assert!(
            exclusive.begin(CapabilityKey::Dictation).is_some(),
            "the claim outlived the failed call"
        );
    }

    /// Concurrent is the default and must stay that way: settings panes open
    /// several reads at once and queueing them is visible as a stutter.
    #[tokio::test]
    async fn concurrent_commands_do_not_queue() {
        let exclusive = ExclusiveRegistry::default();
        let spec = CommandSpec::new("test", CapabilityKey::Settings).reports();
        let held = exclusive.begin(CapabilityKey::Settings);
        assert!(held.is_some());

        let result = execute(&exclusive, spec, (), |_| async { Ok::<(), EchoError>(()) }).await;
        assert!(
            result.is_ok(),
            "a concurrent command waited on an exclusive claim"
        );
    }
}
