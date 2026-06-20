// Frontend side of live dictation. Capture, VAD segmentation and transcription
// all run in the Rust backend (see src-tauri/src/dictation.rs); this module just
// starts/stops a session and subscribes to the stream of transcript/level/error
// events so the UI can insert text as the user speaks.

import { listen } from "@tauri-apps/api/event";

import { startDictation, stopDictation } from "./api";

export { startDictation, stopDictation };

export interface DictationHandlers {
  /** A finalized transcript chunk to insert into the note. */
  onSegment: (text: string) => void;
  /** A live, in-progress transcript of the current utterance (ghost text), sent
   *  repeatedly while speaking and superseded by the next `onSegment`. */
  onPartial?: (text: string) => void;
  /** Input level in `[0, 1]` for the meter. */
  onLevel?: (level: number) => void;
  /** A non-fatal error (e.g. no microphone, transcription failure). */
  onError?: (message: string) => void;
  /** Capture is live. */
  onStarted?: () => void;
  /** Capture ended (worker exited). */
  onStopped?: () => void;
}

/** Subscribe to the backend dictation events. Resolves to an unsubscribe fn. */
export async function subscribeDictation(handlers: DictationHandlers): Promise<() => void> {
  const unlisteners = await Promise.all([
    listen<string>("dictation_segment", (e) => handlers.onSegment(e.payload)),
    listen<string>("dictation_partial", (e) => handlers.onPartial?.(e.payload)),
    listen<number>("dictation_level", (e) => handlers.onLevel?.(e.payload)),
    listen<string>("dictation_error", (e) => handlers.onError?.(e.payload)),
    listen("dictation_started", () => handlers.onStarted?.()),
    listen("dictation_stopped", () => handlers.onStopped?.()),
  ]);
  return () => unlisteners.forEach((unlisten) => unlisten());
}
