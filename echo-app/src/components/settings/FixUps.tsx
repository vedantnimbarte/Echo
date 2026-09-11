import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";
import { HotkeyCapture } from "../common/HotkeyCapture";
import { Field, Check } from "../common/Page";

/**
 * What to do when the transcript is wrong.
 *
 * These are the after-the-fact controls: take the words back, or decode the
 * same audio again on something stronger. Both are global shortcuts, because
 * by the time you notice the mistake the focus is in the app that received the
 * text — Echo's own window isn't even on screen.
 */

/** The literal the backend stores for "don't bind this at all". */
const UNBOUND = "off";

function useSetting(key: string) {
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: ["setting", key],
    queryFn: () => commands.getSetting(key),
  });
  const mutation = useMutation({
    mutationFn: (v: string) => commands.setSetting(key, v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", key] }),
  });
  return [data, (v: string) => mutation.mutate(v)] as const;
}

export function FixUps() {
  const qc = useQueryClient();

  const { data: hotkeys } = useQuery({
    queryKey: ["fixup-hotkeys"],
    queryFn: commands.getFixupHotkeys,
  });
  const [undoKey, retryKey] = hotkeys ?? [UNBOUND, UNBOUND];

  const setHotkey = useMutation({
    mutationFn: ({ which, shortcut }: { which: "undo" | "retry"; shortcut: string }) =>
      commands.setFixupHotkey(which, shortcut),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["fixup-hotkeys"] }),
  });

  const { data: targets = [] } = useQuery({
    queryKey: ["retry-targets"],
    queryFn: commands.retryTargets,
  });

  const [scratch, setScratch] = useSetting("scratch_that_enabled");
  const [retryEnabled, setRetryEnabled] = useSetting("retry_enabled");
  const [retryTarget, setRetryTarget] = useSetting("retry_target");

  return (
    <div className="space-y-3.5">
      <Field label="Undo shortcut">
        <div className="flex items-center gap-2">
          <HotkeyCapture
            value={undoKey === UNBOUND ? "" : undoKey}
            onChange={(accel) => setHotkey.mutate({ which: "undo", shortcut: accel })}
          />
          {undoKey !== UNBOUND && (
            <button
              className="btn-ghost px-2.5 py-1.5 text-[13px]"
              onClick={() => setHotkey.mutate({ which: "undo", shortcut: UNBOUND })}
            >
              Unbind
            </button>
          )}
        </div>
      </Field>

      <Check checked={scratch === "true"} onChange={(v) => setScratch(v ? "true" : "false")}>
        Also undo when I say “scratch that” on its own
      </Check>

      <Check
        checked={retryEnabled !== "false"}
        onChange={(v) => setRetryEnabled(v ? "true" : "false")}
      >
        Keep the last utterance so it can be re-transcribed
      </Check>

      {retryEnabled !== "false" && (
        <>
          <Field label="Retry shortcut">
            <div className="flex items-center gap-2">
              <HotkeyCapture
                value={retryKey === UNBOUND ? "" : retryKey}
                onChange={(accel) => setHotkey.mutate({ which: "retry", shortcut: accel })}
              />
              {retryKey !== UNBOUND && (
                <button
                  className="btn-ghost px-2.5 py-1.5 text-[13px]"
                  onClick={() => setHotkey.mutate({ which: "retry", shortcut: UNBOUND })}
                >
                  Unbind
                </button>
              )}
            </div>
          </Field>

          <Field label="Retry with">
            <select
              className="field w-64"
              value={retryTarget ?? ""}
              onChange={(e) => setRetryTarget(e.target.value)}
            >
              <option value="">Largest local model installed</option>
              {targets.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </Field>
        </>
      )}

      {setHotkey.isError && (
        <p className="text-[13px] font-medium text-[var(--ink)]">
          {String(setHotkey.error)}
        </p>
      )}
    </div>
  );
}
