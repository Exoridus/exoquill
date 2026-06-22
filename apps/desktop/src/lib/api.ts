// Typed wrappers around the Tauri notes commands (see src-tauri/src/notes.rs).
import { invoke } from "@tauri-apps/api/core";

import { emotionVector } from "./tts";
import type {
  CaptureSource,
  CatalogItem,
  Job,
  ModelInfo,
  NewNoteVersion,
  Note,
  NoteEvent,
  NoteScope,
  NoteSort,
  NoteSource,
  NoteUpdate,
  NoteVersion,
  OcrLayout,
  RegionCapture,
  RegionOcr,
  TtsResponse,
  TtsTuning,
  TtsVoice,
} from "./types";

/** List notes in a scope (default "active"), ordered by `sort` (default
 *  "modified"); pinned notes always come first. */
export function listNotes(scope: NoteScope = "active", sort: NoteSort = "modified"): Promise<Note[]> {
  return invoke<Note[]>("list_notes", { scope, sort });
}

export function getNote(id: string): Promise<Note | null> {
  return invoke<Note | null>("get_note", { id });
}

export function createNote(
  contentMarkdown = "",
  source: NoteSource = "manual",
  languageMode: string | null = null,
): Promise<Note> {
  return invoke<Note>("create_note", { contentMarkdown, source, languageMode });
}

export function updateNote(id: string, update: NoteUpdate): Promise<Note | null> {
  return invoke<Note | null>("update_note", { id, update });
}

/** Move a note to the trash (soft-delete). */
export function deleteNote(id: string): Promise<boolean> {
  return invoke<boolean>("delete_note", { id });
}

/** Restore a trashed note back to Active. */
export function restoreNote(id: string): Promise<boolean> {
  return invoke<boolean>("restore_note", { id });
}

/** Archive or un-archive a live note. */
export function setArchived(id: string, archived: boolean): Promise<boolean> {
  return invoke<boolean>("set_archived", { id, archived });
}

/** Permanently delete a note (and its events + versions). No undo. */
export function hardDeleteNote(id: string): Promise<boolean> {
  return invoke<boolean>("hard_delete_note", { id });
}

/** Permanently delete trashed notes deleted before `before` (RFC-3339). Returns
 *  the count removed. The caller computes the cutoff (e.g. now − 30 days). */
export function purgeTrash(before: string): Promise<number> {
  return invoke<number>("purge_trash", { before });
}

export function searchNotes(query: string, scope: NoteScope = "active"): Promise<Note[]> {
  return invoke<Note[]>("search_notes", { query, scope });
}

/** The recorded events for a note (format/OCR history), most recent first. */
export function listNoteEvents(noteId: string): Promise<NoteEvent[]> {
  return invoke<NoteEvent[]>("list_note_events", { noteId });
}

/** Record a content snapshot for the edit history (deduped by content hash).
 *  Resolves to the stored version, or `null` if it was a no-op duplicate. */
export function snapshotNoteVersion(version: NewNoteVersion): Promise<NoteVersion | null> {
  return invoke<NoteVersion | null>("snapshot_note_version", { version });
}

/** A note's edit-history versions (diff timeline), most recent first. */
export function listNoteHistory(noteId: string): Promise<NoteVersion[]> {
  return invoke<NoteVersion[]>("list_note_history", { noteId });
}

/** Restore a stored version's content into the note as a new, undoable change. */
export function restoreNoteVersion(noteId: string, versionId: string): Promise<Note | null> {
  return invoke<Note | null>("restore_note_version", { noteId, versionId });
}

/** Export a note's Markdown via a native save dialog. Resolves to the saved
 *  path, or `null` if the user cancelled. */
export function exportNote(id: string): Promise<string | null> {
  return invoke<string | null>("export_note", { id });
}

/** Quick-format the whole note as an async job; resolves to the job id. */
export function formatNote(noteId: string): Promise<string> {
  return invoke<string>("format_note", { noteId });
}

export function cancelJob(id: string): Promise<void> {
  return invoke("cancel_job", { id });
}

export function listJobs(): Promise<Job[]> {
  return invoke<Job[]>("list_jobs");
}

/** OCR an image (raw bytes) and append the text to the note, as a job. */
export function runOcr(noteId: string, imageBytes: number[]): Promise<string> {
  return invoke<string>("run_ocr", { noteId, imageBytes });
}

/** OCR an image into a structured layout (text + selectable word boxes) for the
 *  overlay. Does not touch any note; the UI decides what to insert. */
export function ocrImage(imageBytes: number[]): Promise<OcrLayout> {
  return invoke<OcrLayout>("ocr_image", { imageBytes });
}

/** Format a short snippet synchronously and return the result. */
export function formatText(text: string, instruction?: string): Promise<string> {
  return invoke<string>("format_text", { text, instruction: instruction ?? null });
}

/** Begin a read-aloud session: installs a fresh cancel token so a later
 *  `cancelRead` can stop the speech-prep generation mid-flight. */
export function beginRead(): Promise<void> {
  return invoke("begin_read");
}

/** Cancel the in-progress read-aloud speech-prep (stops the streaming llama
 *  generation promptly instead of letting the current chunk run to completion). */
export function cancelRead(): Promise<void> {
  return invoke("cancel_read");
}

/** Rewrite one chunk of a note into clean, speakable prose for read-aloud, under
 *  the current read session's cancel token. */
export function prepareSpeech(text: string): Promise<string> {
  return invoke<string>("prepare_speech", { text });
}

/** The frozen screenshot for the region-OCR overlay to display (PNG data URL). */
export function getRegionCapture(): Promise<RegionCapture> {
  return invoke<RegionCapture>("get_region_capture");
}

/** Crop the selected region (logical/CSS px, monitor-relative) and OCR it. */
export function ocrRegion(rect: {
  x: number;
  y: number;
  width: number;
  height: number;
}): Promise<RegionOcr> {
  return invoke<RegionOcr>("ocr_region", rect);
}

/** Discard an in-progress region capture (overlay cancelled). */
export function cancelRegionOcr(): Promise<void> {
  return invoke("cancel_region_ocr");
}

/** Start live dictation into the active note. Capture + transcription run in
 *  the backend, which streams `dictation_*` events (see lib/dictation.ts).
 *  `loopback` captures system audio from an output device instead of a mic. */
export function startDictation(
  device?: string,
  languageMode?: string,
  loopback = false,
  opts: {
    /** `false` disables the adaptive AGC (use `gain` as a fixed multiplier). */
    autoGain?: boolean;
    /** Fixed gain multiplier when `autoGain` is off (1.0 = unchanged). */
    gain?: number;
    /** `false` forces the energy VAD even when the Silero model is available. */
    useSilero?: boolean;
  } = {},
): Promise<void> {
  return invoke("start_dictation", {
    device: device ?? null,
    languageMode: languageMode ?? null,
    loopback,
    autoGain: opts.autoGain ?? null,
    gain: opts.gain ?? null,
    useSilero: opts.useSilero ?? null,
  });
}

/** Stop the current dictation session, flushing any trailing utterance. */
export function stopDictation(): Promise<void> {
  return invoke("stop_dictation");
}

/** The dictation sources: microphones plus output devices (WASAPI loopback). */
export function listCaptureSources(): Promise<CaptureSource[]> {
  return invoke<CaptureSource[]>("list_capture_sources");
}

/** Synthesize speech via the local TTS provider; rejects if none is available.
 *  `voiceId` picks a voice (see `listTtsVoices`); `provider` routes to that
 *  voice's backend (`"piper"` | `"xtts"`); `tuning` overrides the synthesis
 *  knobs. Omitted values fall back to the provider/model defaults. */
export function ttsSpeak(
  text: string,
  voiceId?: string,
  provider?: string,
  tuning: TtsTuning = {},
): Promise<TtsResponse> {
  return invoke<TtsResponse>("tts_speak", {
    text,
    voiceId: voiceId ?? null,
    provider: provider ?? null,
    speed: tuning.speed ?? null,
    expressiveness: tuning.expressiveness ?? null,
    cadence: tuning.cadence ?? null,
    sentenceSilence: tuning.sentenceSilence ?? null,
    intonation: tuning.intonation ?? null,
    brightness: tuning.brightness ?? null,
    emotion: emotionVector(tuning.emotion) ?? null,
  });
}

/** Save the read-aloud audio to a WAV file via a native save dialog. `segments`
 *  are the base64 PCM slices (`TtsResponse.pcm`), joined under one RIFF header at
 *  `sampleRate`. Resolves to the saved path, or `null` if the user cancelled.
 *  WebView2 can't trigger a browser download, so the file is written natively
 *  (like `exportNote`). */
export function exportAudio(
  segments: string[],
  sampleRate: number,
  suggestedName: string,
): Promise<string | null> {
  return invoke<string | null>("export_audio", { segments, sampleRate, suggestedName });
}

/** The read-aloud voices the local TTS provider offers (empty when none). */
export function listTtsVoices(): Promise<TtsVoice[]> {
  return invoke<TtsVoice[]>("list_tts_voices");
}

/** Warm up a TTS backend's sidecar in the background (idempotent). Call when a
 *  backend becomes active so only it loads — never both at launch. Returns at
 *  once; synthesis falls back to Piper until the sidecar is ready. */
export function warmTts(provider: string): Promise<void> {
  return invoke("warm_tts", { provider });
}

/** Resolve once `provider`'s sidecar is warm (model loaded), starting its warm-up
 *  if needed; rejects on an unconfigured backend, warm-up failure, or timeout.
 *  Piper resolves at once. Lets read-aloud wait for a cold voice and then start
 *  automatically, instead of asking the user to click play again. */
export function ensureTtsReady(provider: string): Promise<void> {
  return invoke("ensure_tts_ready", { provider });
}

/** The resolved on-device AI providers with license + status (settings view). */
export function listModelInfo(): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>("list_model_info");
}

/** The installable model/voice catalog with on-disk status (model manager). */
export function listCatalog(): Promise<CatalogItem[]> {
  return invoke<CatalogItem[]>("list_catalog");
}

/** Download + install a catalog entry's files; emits `model_progress` events. */
export function installModel(id: string): Promise<void> {
  return invoke("install_model", { id });
}

/** Delete a downloaded entry's files, freeing disk. */
export function deleteModel(id: string): Promise<void> {
  return invoke("delete_model", { id });
}
