/**
 * SOURCE OF TRUTH KEYWORDS: LatencyPanel, StageLatency, p50, p95
 * WHAT:  What Echo's own timings say about how fast it is, per stage.
 * WHY:   THE HONEST PAIR, NOT THE FLATTERING NUMBER. The median is what
 *        dictation usually feels like; the p95 is the one people actually
 *        remember, because a delay you hit one time in twenty is a delay you
 *        have noticed. Showing only the median is how an app measures well and
 *        feels slow, so both are always shown and neither is emphasised over
 *        the other.
 *
 *        The sample count is shown too. A p95 over four dictations is not a
 *        p95, and a reader who can see that is in a position to discount it —
 *        one who cannot is being quietly misled by a number with a confident
 *        name.
 *
 *        Nothing here is a benchmark. These are measurements from this
 *        machine, this model and this microphone, which is the only version of
 *        the number worth putting in front of the person who owns them.
 * WHERE: Rendered by InsightsPanel; fed by the latency_summary command.
 */

import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

interface StageLatency {
  stage: string;
  label: string;
  p50_ms: number;
  p95_ms: number;
  samples: number;
}

/** Milliseconds, in the unit that reads best at that size. */
function duration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}

export function LatencyPanel() {
  const { data: stages } = useQuery({
    queryKey: ["latency-summary"],
    queryFn: () => invoke<StageLatency[]>("latency_summary"),
  });

  // Nothing measured yet is not a failure and not worth a panel saying so —
  // the first few dictations will fill it.
  if (!stages || stages.length === 0) return null;

  return (
    <section style={{ marginTop: "var(--space-8)" }}>
      <h2
        style={{
          margin: "0 0 var(--space-1)",
          color: "var(--text-primary)",
          fontSize: "var(--text-heading-size)",
          lineHeight: "var(--text-heading-line)",
          fontWeight: "var(--text-heading-weight)",
          letterSpacing: "var(--text-heading-tracking)",
        }}
      >
        How fast it actually is
      </h2>
      <p
        style={{
          margin: "0 0 var(--space-4)",
          color: "var(--text-secondary)",
          fontSize: "var(--text-caption-size)",
          lineHeight: "var(--text-caption-line)",
          maxWidth: "58ch",
        }}
      >
        Measured on this machine over your recent dictations, not a benchmark.
        The typical figure is what it usually feels like; the slowest one in
        twenty is the one you remember.
      </p>

      <div>
        {stages.map((stage) => (
          <div key={stage.stage} className="setting-row">
            <div style={{ minWidth: 0, flex: 1 }}>
              <div
                style={{
                  color: "var(--text-primary)",
                  fontSize: "var(--text-body-size)",
                  fontWeight: "var(--text-label-weight)",
                }}
              >
                {stage.label}
              </div>
              <div
                style={{
                  color: "var(--text-tertiary)",
                  fontSize: "var(--text-caption-size)",
                }}
              >
                {stage.samples} measurement{stage.samples === 1 ? "" : "s"}
              </div>
            </div>

            <div style={{ display: "flex", gap: "var(--space-6)" }}>
              {[
                { label: "Typical", value: stage.p50_ms },
                { label: "Slowest in 20", value: stage.p95_ms },
              ].map((figure) => (
                <div key={figure.label} style={{ textAlign: "right" }}>
                  <div
                    className="tabular"
                    style={{
                      color: "var(--text-primary)",
                      fontSize: "var(--text-body-size)",
                      fontVariantNumeric: "tabular-nums",
                    }}
                  >
                    {duration(figure.value)}
                  </div>
                  <div
                    style={{
                      color: "var(--text-tertiary)",
                      fontSize: "var(--text-caption-size)",
                    }}
                  >
                    {figure.label}
                  </div>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
