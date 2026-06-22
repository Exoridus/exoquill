// Read-aloud queue: splits text into speakable chunks and pipelines synthesis so
// playback starts after the *first* segment instead of waiting for the whole
// note, prefetching the next segment while the current one plays. Uses the local
// Piper TTS (`ttsSpeak`) when available; falls back to the webview's system
// speech (speech.ts) if Piper isn't there or the first segment fails.

import { decodePcm, pausePlayback, playSamples, resumePlayback, stopPlayback } from "./audio";
import { speak, stopSpeaking } from "./speech";
import type { TtsResponse } from "./types";

/** A running read-aloud session: `stop` cancels it; `pause`/`resume` suspend and
 *  continue playback (the queue waits while paused). */
export interface ReadAloudHandle {
  stop: () => void;
  pause: () => void;
  resume: () => void;
}

/** Strip the leading block marker from a line: ATX heading (`#`), blockquote
 *  (`>`), and unordered/ordered list bullets. */
function stripLineMarker(line: string): string {
  return line
    .replace(/^\s*#{1,6}\s+/, "")
    .replace(/^\s*>+\s?/, "")
    .replace(/^\s*([*\-+]|\d+[.)])\s+/, "")
    .trim();
}

/** A thematic break (`---`, `***`, `___`) — speak nothing for it. */
function isThematicBreak(line: string): boolean {
  return /^\s*([-*_])\s*(\1\s*){2,}$/.test(line);
}

/** A Markdown table's separator row (`| --- | :--: |`) — carries no words. */
function isTableSeparator(line: string): boolean {
  return /^[\s|:-]+$/.test(line) && line.includes("-") && line.includes("|");
}

/** Reduce inline Markdown to spoken words: links/images to their label, bare
 *  URLs to their host, and drop emphasis / inline-code marks. */
function cleanInline(text: string): string {
  return text
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1") // links / images → label
    .replace(/\bhttps?:\/\/(?:www\.)?([^\s/]+)\S*/gi, "$1") // bare URL → host
    .replace(/[*_`~]/g, "") // emphasis / inline-code marks
    .replace(/\s+/g, " ")
    .trim();
}

/** Reduce file paths and package names to a speakable tail: `src/core/Foo.ts` →
 *  `Foo`, `@codexo/exojs-tilemap` → `exojs-tilemap`. Reading every slash and
 *  extension aloud is noise; the last segment carries the meaning. */
function reducePaths(text: string): string {
  return text
    .replace(/[^\s]*\/[^\s]*/g, (token) => token.split("/").filter(Boolean).pop() ?? token)
    .replace(/\.(tsx?|jsx?|rs|py|json|jsonc|md|exe|onnx|gguf|toml|css|html?)\b/gi, "");
}

/** Map meaningful symbols to spoken words (so a table's ✅/❌ isn't lost), then
 *  drop arrows and bullets. */
function mapSymbols(text: string): string {
  return text
    .replace(/✅/g, " ja ")
    .replace(/❌/g, " nein ")
    .replace(/⚠️?/g, " Achtung ")
    .replace(/[×✕✖]/g, " mal ")
    .replace(/[→⟶➜➔⇒]/g, " ") // arrows → a pause
    .replace(/[↔⇄⟷]/g, " ")
    .replace(/[•◦▪‣·]/g, " "); // bullets / middots
}

/** Normalize already-flattened text for speech: reduce paths, map/strip symbols
 *  a TTS would mispronounce. Sentence punctuation (`.,!?;:`) is kept — Piper uses
 *  it for pauses/intonation, not literal speech. */
function speechNormalize(text: string): string {
  return mapSymbols(reducePaths(text))
    .replace(/[|#<>~^*_=`{}[\]]/g, " ") // residual markdown / structural symbols
    .replace(/\s+([.,!?;:])/g, "$1") // tidy space left before punctuation
    .replace(/\s+/g, " ")
    .trim();
}

/** Turn a Markdown table into spoken text. The first row is the header; each data
 *  row is read as `Header: value, Header: value` so a cell keeps its meaning, and
 *  rows are separate sentences. A header-only table just reads its cells. */
function tableToSpeech(lines: string[]): string {
  const splitRow = (line: string): string[] =>
    line
      .replace(/^\s*\|/, "")
      .replace(/\|\s*$/, "")
      .split("|")
      .map((cell) => cleanInline(cell));
  const rows = lines.filter((l) => l.includes("|") && !isTableSeparator(l)).map(splitRow);
  if (rows.length === 0) return "";
  const [header, ...body] = rows;
  if (body.length === 0) return header.filter(Boolean).join(", ");
  return body
    .map((cells) =>
      cells
        .map((cell, i) => (header[i] && cell ? `${header[i]}: ${cell}` : cell))
        .filter(Boolean)
        .join(", "),
    )
    .filter(Boolean)
    .join(". ");
}

/** Split Markdown into speakable chunks: drop fenced code, turn tables into
 *  spoken rows (with column labels), flatten list/heading/quote markers and
 *  emphasis, reduce links/paths, map/strip symbols a TTS would mispronounce, then
 *  split into sentence-sized pieces so synthesis can start quickly and stay
 *  bounded. */
export function splitForSpeech(markdown: string): string[] {
  const noCode = markdown.replace(/```[\s\S]*?```/g, " ");
  const chunks: string[] = [];
  for (const block of noCode.split(/\n{2,}/)) {
    const lines = block.split("\n");
    const isTable = lines.some(isTableSeparator) && lines.filter((l) => l.includes("|")).length >= 2;
    const flattened = isTable
      ? tableToSpeech(lines)
      : cleanInline(lines.map(stripLineMarker).filter((l) => l && !isThematicBreak(l)).join(" "));
    const clean = speechNormalize(flattened);
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
    playSamples(decodePcm(audio.pcm), audio.sampleRate, () => {
      onStopRef.resolve = null;
      resolve();
    });
  });
}

/** A one-shot, re-armable notification used to hand control between the producer
 *  and consumer of the synthesis buffer (no value, just "something changed"). */
function makeSignal(): { wait: () => Promise<void>; fire: () => void } {
  let resolve: () => void = () => {};
  let promise = new Promise<void>((r) => (resolve = r));
  return {
    wait: () => promise,
    fire: () => {
      resolve();
      promise = new Promise<void>((r) => (resolve = r));
    },
  };
}

/** How many segments to synthesize ahead of playback. A deeper buffer hides
 *  per-segment synthesis latency (e.g. XTTS, slower than real time) so short
 *  sentences after a long one don't drain it into an audible gap. */
const LOOKAHEAD = 3;

/** A sentence source that yields every speakable sentence of `markdown` up
 *  front — no LLM pass, so synthesis can start immediately. */
export async function* plainSource(markdown: string): AsyncGenerator<string> {
  for (const sentence of splitForSpeech(markdown)) yield sentence;
}

/** A sentence source that rewrites the note for speech through `prepare` (an LLM
 *  pass). All chunks are kicked off at once — the llama-server's parallel slots
 *  batch them, so preparation runs concurrently instead of one chunk after the
 *  next. Sentences are still yielded in order (playback needs order); a chunk
 *  whose preparation fails falls back to its raw text. `onProgress(done, total)`
 *  counts completions (any order). `prepare` may be a cache so an already-prepared
 *  chunk returns instantly. */
export async function* preparedSource(
  chunks: string[],
  prepare: (chunk: string) => Promise<string>,
  onProgress?: (done: number, total: number) => void,
): AsyncGenerator<string> {
  let done = 0;
  const pending = chunks.map((chunk) =>
    prepare(chunk)
      .then((formatted) => formatted || chunk)
      .catch(() => chunk)
      .finally(() => onProgress?.(++done, chunks.length)),
  );
  try {
    for (const chunkResult of pending) {
      const formatted = await chunkResult;
      for (const sentence of splitForSpeech(formatted)) yield sentence;
    }
  } finally {
    onProgress?.(chunks.length, chunks.length);
  }
}

/**
 * Start reading the sentences from `source` aloud. Returns a handle whose
 * `stop()` cancels playback and any further synthesis. `onDone` fires when the
 * queue finishes or the system-speech fallback ends (not when stopped).
 *
 * Pipeline: a producer pulls sentences from `source` and synthesizes them up to
 * `LOOKAHEAD` ahead into a bounded buffer; a consumer plays them in order. This
 * keeps a primed buffer so variable-length segments don't open audible gaps, and
 * lets a slow `source` (the LLM speech-prep pass) overlap with playback.
 *
 * `fallbackText` is read with the webview's system speech when no local TTS is
 * available (the first synthesis fails). `onPlaybackStart` fires once, when the
 * first audio actually begins — letting the UI distinguish the prepare/buffer
 * phase (cancel only) from playback (pause/stop).
 */
export interface ReadAloudOptions {
  /** Fires once when the first audio actually begins (UI: prepare → playback). */
  onPlaybackStart?: () => void;
  /** Synthesize the WHOLE text up front, then play it gapless — for backends
   *  slower than real time (Zonos), where streaming opens audible gaps between
   *  sentences. Off (streaming) for fast backends (Piper, XTTS). */
  prebuffer?: boolean;
  /** Progress of the prebuffer synthesis pass (`done`/`total` segments). */
  onPrepare?: (done: number, total: number) => void;
  /** Pre-synthesized audio to play directly — skips synthesis entirely. Used when
   *  the same voice+text was already rendered (cache hit). */
  cachedAudios?: TtsResponse[];
  /** Delivers the full set of synthesized segments once *generation* finishes
   *  (not on stop), so the caller can cache them and offer an audio export. Fires
   *  as soon as synthesis is done — before playback drains — and only for a
   *  complete read, so the cached audio is never partial. */
  onAudio?: (audios: TtsResponse[]) => void;
  /** Called instead of the system-speech fallback when the first synthesis fails
   *  — e.g. the chosen sidecar voice isn't warm yet. Lets the UI show a "voice
   *  loading" hint and play nothing, rather than a jarring robot voice. */
  onUnavailable?: () => void;
}

export function readAloud(
  source: AsyncIterable<string>,
  ttsSpeak: (text: string) => Promise<TtsResponse>,
  onDone: () => void,
  fallbackText: string,
  opts: ReadAloudOptions = {},
): ReadAloudHandle {
  const { onPlaybackStart, prebuffer, onPrepare, cachedAudios, onAudio, onUnavailable } = opts;
  const playState: { resolve: (() => void) | null } = { resolve: null };
  const buffer: (TtsResponse | null)[] = [];
  const hasItem = makeSignal();
  const hasSpace = makeSignal();
  let producerDone = false;
  let cancelled = false; // stop producing/consuming (user stop OR fallback)
  let userStopped = false; // the user pressed stop — suppress onDone

  const stop = () => {
    userStopped = true;
    cancelled = true;
    stopPlayback();
    stopSpeaking();
    playState.resolve?.(); // unblock a segment we're awaiting
    playState.resolve = null;
    hasItem.fire(); // unblock a waiting consumer
    hasSpace.fire(); // unblock a waiting producer
  };

  // Cache hit: play already-synthesized audio directly, no TTS at all.
  if (cachedAudios) {
    void (async () => {
      onPlaybackStart?.();
      for (const audio of cachedAudios) {
        if (cancelled) return;
        await playToEnd(audio, playState);
        if (cancelled) return;
      }
      if (!userStopped) onDone();
    })();
    return { stop, pause: pausePlayback, resume: resumePlayback };
  }

  // Prebuffer mode: synthesize everything first (with progress), then play it
  // back gapless. For real-time-slower backends, this trades an up-front wait for
  // a smooth read instead of a sentence-by-sentence stutter.
  if (prebuffer) {
    void (async () => {
      const sentences: string[] = [];
      for await (const s of source) {
        if (cancelled) return;
        sentences.push(s);
      }
      const audios: (TtsResponse | null)[] = [];
      for (let i = 0; i < sentences.length; i++) {
        if (cancelled) return;
        onPrepare?.(i, sentences.length);
        const audio = await ttsSpeak(sentences[i]).catch(() => null);
        if (i === 0 && audio === null) {
          // First synthesis failed: voice not warm / unavailable.
          cancelled = true;
          if (onUnavailable) {
            onUnavailable();
            return;
          }
          onPlaybackStart?.();
          speak(fallbackText, { onEnd: onDone });
          return;
        }
        audios.push(audio);
      }
      onPrepare?.(sentences.length, sentences.length);
      if (cancelled) return;
      const finalAudios = audios.filter((a): a is TtsResponse => a !== null);
      onAudio?.(finalAudios); // cache as soon as synthesis is done
      onPlaybackStart?.();
      for (const audio of finalAudios) {
        if (cancelled) return;
        await playToEnd(audio, playState);
        if (cancelled) return;
      }
      if (!userStopped) onDone();
    })();
    return { stop, pause: pausePlayback, resume: resumePlayback };
  }

  // Producer: synthesize sentences from `source`, keeping up to LOOKAHEAD ready
  // segments primed in `buffer`. A failed synthesis is buffered as `null`. Every
  // successful segment is also collected so the full audio can be cached/exported
  // the moment *generation* finishes — without waiting for playback to drain (so
  // the save button shows up as soon as synthesis is done, like the prebuffer
  // path, rather than only after the whole note has been read out).
  const produce = async () => {
    const produced: TtsResponse[] = [];
    try {
      for await (const sentence of source) {
        if (cancelled) return;
        while (buffer.length >= LOOKAHEAD && !cancelled) await hasSpace.wait();
        if (cancelled) return;
        const audio = await ttsSpeak(sentence).catch(() => null);
        if (audio) produced.push(audio);
        buffer.push(audio);
        hasItem.fire();
      }
    } finally {
      producerDone = true;
      hasItem.fire();
      // Full generation finished (not stopped): hand over the complete set for
      // caching + export. Only fires when every segment was synthesized, so the
      // cached audio is never a partial read.
      if (!cancelled && produced.length) onAudio?.(produced);
    }
  };

  // Consumer: play buffered segments in order, making room for the producer.
  void (async () => {
    void produce();
    let first = true;
    for (;;) {
      while (buffer.length === 0 && !producerDone && !cancelled) await hasItem.wait();
      if (cancelled) return;
      if (buffer.length === 0) break; // producer done and buffer drained
      const audio = buffer.shift()!;
      hasSpace.fire();
      if (audio === null) {
        if (first) {
          // First-segment failure: voice not warm / unavailable.
          cancelled = true; // stop the producer
          hasSpace.fire();
          if (onUnavailable) {
            onUnavailable();
            return;
          }
          onPlaybackStart?.();
          speak(fallbackText, { onEnd: onDone });
          return;
        }
        break; // mid-stream failure → just end
      }
      if (first) onPlaybackStart?.();
      first = false;
      await playToEnd(audio, playState);
      if (cancelled) return;
    }
    if (!userStopped) onDone();
  })();

  return { stop, pause: pausePlayback, resume: resumePlayback };
}
