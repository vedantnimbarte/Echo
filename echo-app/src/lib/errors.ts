/**
 * SOURCE OF TRUTH KEYWORDS: errorMessage, errorCode, isRecoverable, EchoError,
 *   ErrorCode, SerializedError
 * WHAT:  Reads an error thrown by an IPC call — its message, its stable code,
 *        and whether retrying could help.
 * WHY:   NOTHING IN THE UI MAY BRANCH ON MESSAGE TEXT. Rust serialises every
 *        command failure as `{ code, message, recoverable }` (see
 *        src-tauri/src/error.rs), and the code is the only field that survives
 *        a copy edit or a translation. A `String(e)` on that object prints
 *        "[object Object]", which is why every catch site goes through
 *        `errorMessage` instead.
 *
 *        The functions are defensive on purpose. An error reaching here is not
 *        always ours: the webview throws strings for a dropped IPC channel,
 *        plugin code throws whatever it likes, and a thrown `Error` is still an
 *        Error. Each of those has to end as readable text rather than as a
 *        second failure inside the failure handler.
 * WHERE: Every `catch` in the app. The codes come from Rust's ErrorCode enum.
 */

/** The stable codes Rust emits. Mirrors `ErrorCode` in src-tauri/src/error.rs. */
export type ErrorCode =
  | "INVALID_INPUT"
  | "PERMISSION_REQUIRED"
  | "ENGINE_NOT_READY"
  | "ALREADY_IN_PROGRESS"
  | "AUDIO_DEVICE"
  | "TRANSCRIPTION"
  | "INJECTION"
  | "STORAGE"
  | "PLUGIN"
  | "CONFIG"
  | "NOT_FOUND";

export interface SerializedError {
  code: ErrorCode;
  message: string;
  recoverable: boolean;
}

function isSerialized(e: unknown): e is SerializedError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as SerializedError).message === "string" &&
    typeof (e as SerializedError).code === "string"
  );
}

/**
 * Human-readable text for any thrown value.
 *
 * Never returns an empty string: a failure that renders as nothing is worse
 * than one that renders badly, because the user sees a control that did not
 * work and no reason at all.
 */
export function errorMessage(e: unknown): string {
  if (isSerialized(e)) return e.message;
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** The stable code, or null for anything that did not come from Rust. */
export function errorCode(e: unknown): ErrorCode | null {
  return isSerialized(e) ? e.code : null;
}

/**
 * Whether the same call could succeed if tried again in a moment.
 *
 * This is what the pill reads to decide whether to draw a failure at all. The
 * engine still warming in the seconds after launch is the most likely error a
 * new user will ever see, on their first keypress, and painting it as a failure
 * says the app is broken when it is seconds old.
 *
 * Unknown errors are NOT treated as recoverable: a UI that offers a retry for
 * something permanently broken sends the user round a loop.
 */
export function isRecoverable(e: unknown): boolean {
  return isSerialized(e) && e.recoverable;
}
