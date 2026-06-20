// Read-aloud queue: splits text into speakable chunks and pipelines synthesis so
// playback starts after the *first* segment instead of waiting for the whole
// note, prefetching the next segment while the current one plays. Uses the local
// Piper TTS (`ttsSpeak`) when available; falls back to the webview's system
// speech (speech.ts) if Piper isn't there or the first segment fails.

import { playSamples, stopPlayback } from "./audio";
import { speak, stopSpeaking } from "./speech";
import type { TtsResponse } from "./types";

/** A running read-aloud session; call `stop` to cancel it. */
export interface ReadAloudHandle {
  stop: () => void;
}

/** Split Markdown into speakable chunks: drop fenced code, flatten list/heading/
 *  quote markers and emphasis, reduce links to their label, then split into
 *  sentence-sized pieces so synthesis can start quickly and stay bounded. */
export function splitForSpeech(markdown: string): string[] {
  const noCode = markdown.replace(/```[\s\S]*?```/g, " ");
  const chunks: string[] = [];
  for (const paragraph of noCode.split(/\n{2,}/)) {
    const clean = paragraph
      .split("\n")
      .map((line) => line.replace(/^\s*([#>]+|[*\-+]|\d+\.)\s*/, "").trim())
      .join(" ")
      .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1") // links / images → label
      .replace(/[*_`~]/g, "") // emphasis / inline-code marks
      .replace(/\s+/g, " ")
      .trim();
    if (!clean) continue;
    for (const sentence of clean.match(/[^.!?]+[.!?]*\s*/g) ?? [clean]) {
      const text = sentence.trim();
      if (text) chunks.push(text);
    }
  }
  return chunks;
}

/** Play `audio` to completion, resolving when it ends (or is stopped). */
function playToEnd(audio: TtsResponse, onStopRef: { resolve: (() => void) | null }): Promise<void> {
  return new Promise((resolve) => {
    onStopRef.resolve = resolve;
    playSamples(audio.samples, audio.sampleRate, () => {
      onStopRef.resolve = null;
      resolve();
    });
  });
}

/**
 * Start reading `markdown` aloud. Returns a handle whose `stop()` cancels
 * playback and any further synthesis. `onDone` fires when the queue finishes or
 * the fallback ends (not when stopped).
 */
export function readAloud(
  markdown: string,
  ttsSpeak: (text: string) => Promise<TtsResponse>,
  onDone: () => void,
): ReadAloudHandle {
  const chunks = splitForSpeech(markdown);
  let stopped = false;
  const playState: { resolve: (() => void) | null } = { resolve: null };

  const stop = () => {
    stopped = true;
    stopPlayback();
    stopSpeaking();
    playState.resolve?.(); // unblock a chunk we're awaiting
    playState.resolve = null;
  };

  void (async () => {
    if (chunks.length === 0) {
      onDone();
      return;
    }
    let current: TtsResponse;
    try {
      current = await ttsSpeak(chunks[0]);
    } catch {
      // Piper unavailable: read the whole thing with system speech instead.
      if (!stopped) speak(chunks.join(" "), { onEnd: onDone });
      return;
    }
    for (let i = 0; i < chunks.length && !stopped; i++) {
      // Synthesize the next segment while the current one plays.
      const next =
        i + 1 < chunks.length ? ttsSpeak(chunks[i + 1]).catch(() => null) : Promise.resolve(null);
      await playToEnd(current, playState);
      if (stopped) return;
      const ready = await next;
      if (!ready) break;
      current = ready;
    }
    if (!stopped) onDone();
  })();

  return { stop };
}
