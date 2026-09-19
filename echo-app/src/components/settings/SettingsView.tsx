/**
 * SOURCE OF TRUTH KEYWORDS: SettingsView, SettingsPage, useSettingsSnapshot,
 *   EXTRAS, search, registry-generated settings
 * WHAT:  The settings window. Pages and sections come from the registry; the
 *        controls are generated from it; the few blocks a declaration cannot
 *        express are composed in beside them.
 * WHY:   This replaces a 1361-line panel that held its own idea of what the app
 *        has — every setting hand-written as its own block of JSX, which is how
 *        it had drifted into three different ways of drawing a toggle and how
 *        settings that existed in Rust ended up with no control at all.
 *
 *        THE REGISTRY DECIDES PLACEMENT, not this file. A setting declares its
 *        page and section, so adding one is a Rust entry and nothing else. The
 *        only list here is EXTRAS — the bespoke components — and each entry in
 *        it is a thing that genuinely cannot be a declaration: a model download
 *        with a progress bar, a key that goes to the keychain, a log.
 *
 *        SEARCH READS THE REGISTRY TOO. The old panel carried hand-written
 *        keyword arrays per group, which is a second thing to update per
 *        setting and the first thing anyone forgets; labels and descriptions
 *        are already the words someone would type.
 * WHERE: Rendered by App.tsx. Controls come from
 *        components/global/setting-control.
 */

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { Search } from "lucide-react";

import { errorMessage } from "../../lib/errors";
import { SettingControl } from "../global/setting-control/SettingControl";
import {
  SECTION_LABELS,
  SECTION_ORDER,
  SECTION_PAGE,
  type OsPermission,
  type SettingDef,
  type SettingSection,
  type SettingsSnapshot,
} from "../global/setting-control/types";
import { Page, Tabs } from "../common/Page";
import { TelemetrySettings } from "./TelemetrySettings";
import { WakeWordSettings } from "./WakeWordSettings";
import { AppProfiles } from "./AppProfiles";
import { EgressLog } from "./EgressLog";
import { Performance } from "./Performance";
import { AudioImport } from "./AudioImport";
import { About } from "./About";
import { GlobalHotkey, LaunchAtLogin, MicrophoneTest, PillSizePicker } from "./extras";
import { EngineLane } from "./engine-extras";

export type SettingsPage = "settings" | "engine" | "output" | "privacy" | "about";

/**
 * The blocks a registry declaration cannot express, and where each one sits.
 *
 * Every entry here is something with its own behaviour rather than its own
 * value: a download with progress, a key that goes to the OS keychain, a log
 * you read. A setting that is merely *important* does not belong here — it
 * belongs in the registry like the rest, and putting it here to get a custom
 * layout is how the old panel grew to 1361 lines.
 */
const EXTRAS: Partial<
  Record<SettingSection, { title: string | null; key: string; node: React.ReactNode }[]>
> = {
  SETTINGS_GENERAL: [
    { title: "Starting Echo", key: "starting-echo", node: <LaunchAtLogin /> },
    { title: "Pill", key: "pill", node: <PillSizePicker /> },
  ],
  SETTINGS_MICROPHONE: [
    { title: "Check it works", key: "check-it-works", node: <MicrophoneTest /> },
  ],
  // EngineLane owns the model list and the provider list, because which of
  // them is on screen is the lane choice. See engine-extras.tsx.
  ENGINE_SPEECH: [
    { title: null, key: "engine-lane", node: <EngineLane /> },
  ],
  ENGINE_TOOLS: [{ title: "Transcribe a file", key: "transcribe-a-file", node: <AudioImport /> }],
  ENGINE_ADVANCED: [{ title: "Performance", key: "performance", node: <Performance /> }],
  SETTINGS_DICTATION: [
    { title: "Global hotkey", key: "global-hotkey", node: <GlobalHotkey /> },
    { title: "Wake word", key: "wake-word", node: <WakeWordSettings /> },
  ],
  OUTPUT_APPS: [{ title: "Per-app profiles", key: "per-app-profiles", node: <AppProfiles /> }],
  PRIVACY: [
    { title: "What left this machine", key: "what-left-this-machine", node: <EgressLog /> },
    { title: "Usage counting", key: "usage-counting", node: <TelemetrySettings /> },
  ],
};

const PAGE_META: Record<SettingsPage, { title: string; description: string }> = {
  settings: {
    title: "Settings",
    description: "How Echo starts listening, and what the window itself does.",
  },
  engine: {
    title: "Voice engine",
    description: "Which model turns your speech into text, and where it runs.",
  },
  output: {
    title: "Output",
    description: "What Echo does with the words once it has them.",
  },
  privacy: {
    title: "Privacy",
    description: "What is kept, what left this machine, and what you can delete.",
  },
  about: { title: "About", description: "Version, licence, and where to report a problem." },
};

function useSettingsSnapshot() {
  return useQuery({
    queryKey: ["settings-snapshot"],
    queryFn: () => invoke<SettingsSnapshot>("settings_snapshot"),
  });
}

export function SettingsView({ page }: { page: SettingsPage }) {
  const qc = useQueryClient();
  const { data, isLoading, error } = useSettingsSnapshot();
  const [picked, setPicked] = useState<SettingSection | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [q, setQ] = useState("");
  const [writeError, setWriteError] = useState<string | null>(null);

  const query = q.trim().toLowerCase();
  const searching = query.length > 0;

  // Only an explicit `false` means "not available", so nothing flashes a
  // warning on the way in while the probe is still answering.
  const { data: sileroReady } = useQuery({
    queryKey: ["silero-available"],
    queryFn: () => invoke<boolean>("silero_available"),
  });

  // Audio a crash interrupted waits under Tools, and nobody would think to look
  // on a tab for something they did not know survived — so when there is some,
  // that is the tab the engine page opens on.
  const { data: recoveredData } = useQuery({
    queryKey: ["recovered-recordings"],
    queryFn: () => invoke<string[] | null>("recovered_recordings"),
  });
  // `?? []`, not a default on the destructure: a default only covers undefined,
  // and this resolves to null whenever the backend has nothing to report.
  const recovered = recoveredData ?? [];

  const { data: accessibility = true } = useQuery({
    queryKey: ["accessibility-permission"],
    queryFn: () => invoke<boolean>("check_accessibility_permission"),
  });
  const missingPermissions: OsPermission[] = accessibility ? [] : ["ACCESSIBILITY"];

  /**
   * Settings this machine cannot honour. One entry today; a map rather than a
   * special case so the second one is an entry rather than another branch.
   */
  const unavailable: Record<string, { value: string; reason: string }> =
    sileroReady === false
      ? {
          vad_engine: {
            value: "energy",
            reason:
              "The neural model didn’t load on this machine, so Echo is using the simple detector.",
          },
        }
      : {};

  const save = useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) =>
      invoke<void>("set_setting", { key, value }),
    onMutate: async ({ key, value }) => {
      // Optimistic. A toggle that waits for a round trip before moving feels
      // broken; the write is a SQLite row and essentially never fails, but when
      // it does the value snaps back and says why rather than staying where the
      // user put it.
      await qc.cancelQueries({ queryKey: ["settings-snapshot"] });
      const previous = qc.getQueryData<SettingsSnapshot>(["settings-snapshot"]);
      qc.setQueryData<SettingsSnapshot>(["settings-snapshot"], (old) =>
        old
          ? {
              ...old,
              values: old.values.map((v) =>
                v.key === key ? { ...v, value, is_set: true } : v,
              ),
            }
          : old,
      );
      setWriteError(null);
      return { previous };
    },
    onError: (e, _vars, context) => {
      if (context?.previous) qc.setQueryData(["settings-snapshot"], context.previous);
      setWriteError(errorMessage(e));
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ["settings-snapshot"] }),
  });

  const values = useMemo(() => {
    const map = new Map<string, string>();
    for (const v of data?.values ?? []) map.set(v.key, v.value);
    return map;
  }, [data]);

  /**
   * Whether a setting's declared condition currently holds.
   *
   * A control whose condition fails is not rendered at all rather than
   * disabled: unlike a missing permission, there is nothing for the user to
   * fix and nothing to explain — the setting simply does not apply to the
   * choice they have made.
   */
  const conditionHolds = (def: SettingDef) => {
    if (!def.visible_when) return true;
    return def.visible_when.any_of.includes(values.get(def.visible_when.key) ?? "");
  };

  const bySection = useMemo(() => {
    const groups = new Map<SettingSection, SettingDef[]>();
    for (const capability of data?.capabilities ?? []) {
      for (const def of capability.settings) {
        // Not a knob: the setup flow owns it and there is nothing to decide.
        if (def.key === "onboarding_complete") continue;
        // Rendered by a bespoke block in EXTRAS instead — a generated row for
        // the same key would be a second control writing the same value, and
        // the two would disagree the moment one of them was used.
        if (def.key === "pill_size" || def.key === "hotkey") continue;
        const list = groups.get(def.section) ?? [];
        list.push(def);
        groups.set(def.section, list);
      }
    }
    return groups;
  }, [data]);

  if (page === "about") {
    return (
      <Page title={PAGE_META.about.title} description={PAGE_META.about.description}>
        {/* About takes the group-title decorator the old panel used to prefix
            headings with their page while searching. About is never searched
            into — it holds no settings — so it gets the identity. */}
        <About label={(title) => title} />
      </Page>
    );
  }

  const meta = PAGE_META[page];

  if (isLoading) {
    return (
      <Page title={meta.title} description={meta.description}>
        <p style={{ color: "var(--text-tertiary)", fontSize: "var(--text-body-size)" }}>
          Loading settings…
        </p>
      </Page>
    );
  }

  if (error) {
    return (
      <Page title={meta.title} description={meta.description}>
        <p style={{ color: "var(--danger)", fontSize: "var(--text-body-size)" }}>
          {errorMessage(error)}
        </p>
      </Page>
    );
  }

  const pageSections = SECTION_ORDER.filter(
    (s) => SECTION_PAGE[s] === page && (bySection.has(s) || EXTRAS[s]),
  );
  // A tab picked on one page stops matching on the next, so the first tab takes
  // over with no effect to keep in step.
  const tab = pageSections.includes(picked as SettingSection)
    ? (picked as SettingSection)
    : page === "engine" && recovered.length > 0 && pageSections.includes("ENGINE_TOOLS")
      ? "ENGINE_TOOLS"
      : pageSections[0];

  // Search reaches across every page: splitting settings up hides things by
  // design, and without this, finding a control means guessing which page.
  const matches = (def: SettingDef) => {
    const options =
      def.kind.type === "CHOICE"
        ? def.kind.options.map((o) => `${o.value} ${o.label} ${o.description ?? ""}`).join(" ")
        : "";
    return `${def.label} ${def.description} ${def.key} ${options}`
      .toLowerCase()
      .includes(query);
  };

  const shown: SettingSection[] = searching ? SECTION_ORDER : tab ? [tab] : [];

  const search = (
    <div className="relative" style={{ width: 184 }}>
      <Search
        className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2"
        style={{ color: "var(--text-tertiary)" }}
      />
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Search settings"
        aria-label="Search settings"
        className="field focus-ring py-1.5 pl-8 pr-2.5"
      />
    </div>
  );

  const anyAdvanced = shown.some((s) =>
    (bySection.get(s) ?? []).some((d) => d.advanced),
  );

  return (
    <Page
      title={searching ? "Search" : meta.title}
      description={
        searching
          ? `Everything matching “${q.trim()}”, from every page of this window.`
          : meta.description
      }
      actions={search}
      // A search already shows every section at once, so the strip would be
      // offering to narrow to one and then not doing it.
      tabs={
        searching || pageSections.length < 2 ? undefined : (
          <Tabs
            label={`${meta.title} sections`}
            tabs={pageSections.map((s) => ({ id: s, label: SECTION_LABELS[s] }))}
            current={tab}
            onSelect={setPicked}
          />
        )
      }
    >
      {writeError ? (
        <p
          role="alert"
          style={{
            color: "var(--danger)",
            fontSize: "var(--text-caption-size)",
            marginBottom: "var(--space-4)",
          }}
        >
          {writeError}
        </p>
      ) : null}

      {shown.map((section) => {
        const defs = (bySection.get(section) ?? [])
          .filter(conditionHolds)
          .filter((d) => (searching ? matches(d) : showAdvanced || !d.advanced));
        const extras = searching ? [] : (EXTRAS[section] ?? []);
        if (defs.length === 0 && extras.length === 0) return null;

        return (
          <section key={section} style={{ marginBottom: "var(--space-8)" }}>
            {/* While searching, groups arrive out of context — say where each
                one lives, which is a page and a section inside it. */}
            <h2
              style={{
                margin: "0 0 var(--space-2)",
                color: "var(--text-primary)",
                fontSize: "var(--text-heading-size)",
                lineHeight: "var(--text-heading-line)",
                fontWeight: "var(--text-heading-weight)",
                letterSpacing: "var(--text-heading-tracking)",
              }}
            >
              {searching
                ? `${PAGE_META[SECTION_PAGE[section] as SettingsPage].title} · ${SECTION_LABELS[section]}`
                : SECTION_LABELS[section]}
            </h2>

            <div>
              {defs.map((def) => (
                <SettingControl
                  key={def.key}
                  def={def}
                  value={values.get(def.key) ?? ""}
                  missingPermissions={missingPermissions}
                  unavailable={unavailable[def.key]}
                  onChange={(next) => save.mutate({ key: def.key, value: next })}
                />
              ))}
            </div>

            {extras.map((extra) => (
              <div key={extra.key} style={{ marginTop: "var(--space-6)" }}>
                {extra.title ? (
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
                  {extra.title}
                </h3>
                ) : null}
                {extra.node}
              </div>
            ))}
          </section>
        );
      })}

      {searching &&
      shown.every(
        (s) => (bySection.get(s) ?? []).filter(conditionHolds).filter(matches).length === 0,
      ) ? (
        <p style={{ color: "var(--text-tertiary)", fontSize: "var(--text-body-size)" }}>
          Nothing matches “{q.trim()}”.
        </p>
      ) : null}

      {anyAdvanced && !searching ? (
        <button
          type="button"
          className="interactive focus-ring"
          onClick={() => setShowAdvanced((v) => !v)}
          style={{
            height: "var(--control-height-sm)",
            padding: "0 var(--space-3)",
            borderRadius: "var(--radius-input)",
            color: "var(--text-secondary)",
            fontSize: "var(--text-label-size)",
            cursor: "pointer",
          }}
        >
          {showAdvanced ? "Hide advanced settings" : "Show advanced settings"}
        </button>
      ) : null}
    </Page>
  );
}
