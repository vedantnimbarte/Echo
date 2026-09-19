/**
 * SOURCE OF TRUTH KEYWORDS: EngineLane, laneOf, lane, chooseLane
 * WHAT:  The offline / cloud choice, and the half of the engine page it swaps.
 * WHY:   It fails the "is this a value?" test that decides what the registry
 *        can generate:
 *
 *          EngineLane   — `asr_provider` IS a registry setting, and a plain
 *                         picker over it would be wrong in a specific way.
 *                         Choosing "offline" is a whole decision and commits;
 *                         choosing "cloud" is not — WHICH provider is still
 *                         unanswered, and pointing dictation at one before a
 *                         key is stored breaks dictation mid-setup. So the
 *                         cloud side opens the provider list and commits only
 *                         when a provider is actually ready.
 *        The neural-detector notice used to live here too. It does not any
 *        more: "this machine cannot honour that setting" is a general shape,
 *        so it became SettingControl's `unavailable` prop and now renders on
 *        the control it is about rather than floating above the section.
 * WHERE: Composed into SettingsView's EXTRAS table.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Cloud, Laptop } from "lucide-react";

import { useState } from "react";

import { commands } from "../../ipc/commands";
import { ModelSelector } from "./ModelSelector";
import { CloudProviders } from "./CloudProviders";

/** Which side of the offline/cloud split a stored provider falls on. */
function laneOf(provider: string | null | undefined): "local" | "cloud" {
  return provider === "local" || provider === "nemo" || !provider ? "local" : "cloud";
}

/**
 * The lane choice AND the half of the page it swaps.
 *
 * One component rather than two, because the lane is UI state that has not been
 * committed: picking "cloud" shows the provider list without pointing dictation
 * at anything. Splitting the picker from what it reveals would mean lifting
 * that uncommitted state into the settings view, which would then be holding a
 * piece of engine logic that belongs here.
 */
export function EngineLane() {
  const qc = useQueryClient();

  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: model } = useQuery({
    queryKey: ["setting", "whisper_model"],
    queryFn: () => commands.getSetting("whisper_model"),
  });
  const { data: providers = [] } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });

  const setProvider = useMutation({
    mutationFn: (v: string) => commands.setSetting("asr_provider", v),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
      qc.invalidateQueries({ queryKey: ["settings-snapshot"] });
    },
  });

  const committed = laneOf(provider);
  // What the user is LOOKING at, which is the committed lane until they click.
  const [browsing, setBrowsing] = useState<"local" | "cloud" | null>(null);
  const active = browsing ?? committed;
  const activeCloud = providers.find((p) => p.id === provider);

  function choose(next: "local" | "cloud") {
    setBrowsing(next);
    // Picking offline is the whole decision, so it commits. Picking cloud is
    // not — see the module WHY.
    if (next === "local" && committed !== "local") setProvider.mutate("local");
  }

  const options = [
    {
      id: "local" as const,
      Icon: Laptop,
      title: "On this machine",
      sub: "Whisper runs offline. Audio never leaves your computer.",
      running: active === "local" ? `Whisper ${model || "base.en"}` : null,
    },
    {
      id: "cloud" as const,
      Icon: Cloud,
      title: "A cloud provider",
      sub: "Faster on a slow machine. Audio is sent to the provider you choose.",
      running: activeCloud ? `${activeCloud.label}, ${activeCloud.model}` : null,
    },
  ];

  return (
    <div>
    <div role="radiogroup" aria-label="Where transcription runs" className="grid grid-cols-2 gap-2.5">
      {options.map(({ id, Icon, title, sub, running }) => {
        const selected = active === id;
        return (
          <button
            key={id}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => choose(id)}
            data-selected={selected}
            className="material interactive focus-ring flex flex-col items-start gap-2 text-left"
            style={{
              padding: "var(--space-4)",
              borderRadius: "var(--radius-input)",
              borderColor: selected ? "var(--accent)" : "var(--border-hairline)",
              cursor: "pointer",
            }}
          >
            <Icon className="h-4 w-4" style={{ color: "var(--text-secondary)" }} />
            <span
              style={{
                color: "var(--text-primary)",
                fontSize: "var(--text-body-size)",
                fontWeight: "var(--text-label-weight)",
              }}
            >
              {title}
            </span>
            <span
              style={{
                color: "var(--text-secondary)",
                fontSize: "var(--text-caption-size)",
                lineHeight: "var(--text-caption-line)",
              }}
            >
              {sub}
            </span>
            {/* The selected card carries what is actually running, which a
                dropdown could only say by being open. */}
            {running ? (
              <span
                style={{
                  color: "var(--text-tertiary)",
                  fontSize: "var(--text-caption-size)",
                }}
              >
                Running {running}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>

      {/* The page swaps its whole lower half on this choice, heading included:
          the heading is what says which half you are looking at, and without it
          the provider list and the model list read as the same section having
          changed its mind. */}
      <div style={{ marginTop: "var(--space-6)" }}>
        <h3
          style={{
            margin: "0 0 var(--space-3)",
            color: "var(--text-secondary)",
            fontSize: "var(--text-label-size)",
            fontWeight: "var(--text-label-weight)",
            textTransform: "uppercase",
            letterSpacing: "0.06em",
          }}
        >
          {active === "local" ? "Local models" : "Cloud providers"}
        </h3>
        {active === "local" ? <ModelSelector /> : <CloudProviders />}
      </div>
    </div>
  );
}
