import { useState } from "react";
import { Plus, Trash2, Crosshair } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type AppProfile } from "../../ipc/commands";

/**
 * Per-app overrides. Each field can be left on "Global", which inherits the
 * setting from the sections above — so a profile can pin one behaviour (never
 * auto-insert into a password manager) without freezing everything else.
 *
 * The app is identified by executable name on Windows, bundle id on macOS, and
 * window class on Linux/X11. Where the platform can't tell us (Wayland, or
 * macOS without Automation permission) profiles simply don't apply and the
 * global settings are used.
 */
export function AppProfiles() {
  const qc = useQueryClient();
  const { data: profiles = [] } = useQuery({
    queryKey: ["app-profiles"],
    queryFn: commands.listAppProfiles,
  });
  const { data: dictProfiles = [] } = useQuery({
    queryKey: ["dict-profiles"],
    queryFn: commands.listProfiles,
  });

  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    qc.invalidateQueries({ queryKey: ["app-profiles"] });
  }

  async function guard(fn: () => Promise<unknown>) {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
    refresh();
  }

  /** Read whatever is focused right now so the user doesn't have to know the
   *  executable name. They get a moment to switch windows first. */
  async function detectCurrent() {
    setError(null);
    const app = await commands.getForegroundApp();
    if (!app) {
      setError(
        "Couldn't detect the focused app. On Wayland this isn't available; on macOS, grant Automation permission for System Events."
      );
      return;
    }
    setPending(app);
  }

  async function addPending() {
    if (!pending) return;
    await guard(() =>
      commands.saveAppProfile({
        id: null,
        app_match: pending,
        label: null,
        auto_inject: null,
        injection_method: null,
        profile_id: null,
        enabled: true,
      })
    );
    setPending(null);
  }

  function update(p: AppProfile, patch: Partial<AppProfile>) {
    void guard(() => commands.saveAppProfile({ ...p, ...patch }));
  }

  return (
    <div className="space-y-3">
      <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
        Override how Echo behaves in specific apps. Anything left on “Global”
        follows the settings above.
      </p>

      <div className="flex gap-1.5">
        <input
          className="field flex-1"
          placeholder="Application (e.g. code.exe)"
          value={pending ?? ""}
          onChange={(e) => setPending(e.target.value)}
        />
        <button
          onClick={detectCurrent}
          className="btn-ghost shrink-0 px-2.5 text-[12px]"
          title="Use whichever app is focused right now"
        >
          <Crosshair className="h-3.5 w-3.5" />
          Detect
        </button>
        <button
          onClick={addPending}
          disabled={!pending?.trim()}
          className="btn-primary shrink-0 px-2.5 text-[12px]"
        >
          <Plus className="h-3.5 w-3.5" />
          Add
        </button>
      </div>

      {profiles.length === 0 ? (
        <p className="text-[11px] text-[var(--ink-muted)]">No app profiles yet.</p>
      ) : (
        <ul className="space-y-1.5">
          {profiles.map((p) => (
            <li key={p.id} className="glass rounded-lg px-3 py-2">
              <div className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={p.enabled}
                  onChange={(e) => update(p, { enabled: e.target.checked })}
                  className="h-3.5 w-3.5 accent-white"
                  title="Enable this profile"
                />
                <span className="flex-1 truncate font-mono text-[12px] text-[var(--ink)]">
                  {p.app_match}
                </span>
                <button
                  onClick={() => p.id !== null && guard(() => commands.deleteAppProfile(p.id!))}
                  className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
                  title="Delete profile"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>

              <div className="mt-2 grid grid-cols-3 gap-1.5">
                <label className="block">
                  <span className="mb-0.5 block text-[9.5px] uppercase tracking-wide text-[var(--ink-faint)]">
                    Insert text
                  </span>
                  <select
                    className="field text-[11px]"
                    value={
                      p.auto_inject === null ? "global" : p.auto_inject ? "on" : "off"
                    }
                    onChange={(e) =>
                      update(p, {
                        auto_inject:
                          e.target.value === "global" ? null : e.target.value === "on",
                      })
                    }
                  >
                    <option value="global">Global</option>
                    <option value="on">Always</option>
                    <option value="off">Never</option>
                  </select>
                </label>

                <label className="block">
                  <span className="mb-0.5 block text-[9.5px] uppercase tracking-wide text-[var(--ink-faint)]">
                    Method
                  </span>
                  <select
                    className="field text-[11px]"
                    value={p.injection_method ?? "global"}
                    onChange={(e) =>
                      update(p, {
                        injection_method:
                          e.target.value === "global" ? null : e.target.value,
                      })
                    }
                  >
                    <option value="global">Global</option>
                    <option value="type">Type</option>
                    <option value="paste">Paste</option>
                  </select>
                </label>

                <label className="block">
                  <span className="mb-0.5 block text-[9.5px] uppercase tracking-wide text-[var(--ink-faint)]">
                    Dictionary
                  </span>
                  <select
                    className="field text-[11px]"
                    value={p.profile_id === null ? "global" : String(p.profile_id)}
                    onChange={(e) =>
                      update(p, {
                        profile_id:
                          e.target.value === "global" ? null : Number(e.target.value),
                      })
                    }
                  >
                    <option value="global">Global only</option>
                    {dictProfiles.map((d) => (
                      <option key={d.id} value={String(d.id)}>
                        + {d.name}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </li>
          ))}
        </ul>
      )}

      {error && <p className="text-[11px] font-medium text-[var(--ink)]">{error}</p>}
    </div>
  );
}
