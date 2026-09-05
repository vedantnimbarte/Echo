/**
 * Short tones for the start and end of recording.
 *
 * The pill is the only feedback that dictation started, and it is frequently
 * not where the user is looking — the whole point is that they are typing into
 * something else. A sound is the one channel that works regardless of where
 * their eyes are, and it is how you notice the hotkey *didn't* fire before
 * speaking a whole sentence into nothing.
 *
 * Off by default: an app that starts making noise after an update is a bad
 * surprise, so this is something the user turns on.
 */

/** Rising pair for "listening", falling pair for "stopped". */
const START_NOTES = [523.25, 659.25]; // C5, E5
const STOP_NOTES = [587.33, 440.0]; // D5, A4

const NOTE_SECONDS = 0.09;
const GAP_SECONDS = 0.025;
/** Short fade in and out; a square-edged gate on a sine is heard as a click. */
const ATTACK_SECONDS = 0.015;
/** Quiet enough to sit under speech rather than compete with it. */
const PEAK_GAIN = 0.2;
/** Exponential ramps cannot reach zero, so they aim here instead. */
const MIN_GAIN = 0.0001;

let context: AudioContext | null = null;

function audioContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext })
      .webkitAudioContext;
  if (!Ctor) return null;

  // A context can be closed out from under us (device change, tab suspension),
  // and a closed one throws on every use rather than reopening itself.
  if (!context || context.state === "closed") context = new Ctor();
  return context;
}

function play(notes: number[]): void {
  const ctx = audioContext();
  if (!ctx) return;

  // Browsers start contexts suspended until a gesture. A dictation always
  // follows one, but the resume is async and we do not want to wait on it.
  if (ctx.state === "suspended") void ctx.resume();

  notes.forEach((frequency, i) => {
    const start = ctx.currentTime + i * (NOTE_SECONDS + GAP_SECONDS);
    const end = start + NOTE_SECONDS;

    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.value = frequency;

    const gain = ctx.createGain();
    gain.gain.setValueAtTime(MIN_GAIN, start);
    gain.gain.exponentialRampToValueAtTime(PEAK_GAIN, start + ATTACK_SECONDS);
    gain.gain.exponentialRampToValueAtTime(MIN_GAIN, end);

    osc.connect(gain).connect(ctx.destination);
    osc.start(start);
    osc.stop(end + 0.01);
  });
}

/** Rising tone: Echo is listening. */
export function cueStart(): void {
  try {
    play(START_NOTES);
  } catch {
    // Audio output is a nicety; never let it break a dictation.
  }
}

/** Falling tone: Echo stopped listening. */
export function cueStop(): void {
  try {
    play(STOP_NOTES);
  } catch {
    // As above.
  }
}
