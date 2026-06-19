// Typed wrappers around the Tauri notes commands (see src-tauri/src/notes.rs).
import { invoke } from "@tauri-apps/api/core";

import type {
  CaptureSource,
  Job,
  Note,
  NoteSource,
  NoteUpdate,
  OcrLayout,
  TtsResponse,
} from "./types";

export function listNotes(): Promise<Note[]> {
  return invoke<Note[]>("list_notes");
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

export function deleteNote(id: string): Promise<boolean> {
  return invoke<boolean>("delete_note", { id });
}

export function searchNotes(query: string): Promise<Note[]> {
  return invoke<Note[]>("search_notes", { query });
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

/** Start live dictation into the active note. Capture + transcription run in
 *  the backend, which streams `dictation_*` events (see lib/dictation.ts).
 *  `loopback` captures system audio from an output device instead of a mic. */
export function startDictation(
  device?: string,
  languageMode?: string,
  loopback = false,
): Promise<void> {
  return invoke("start_dictation", {
    device: device ?? null,
    languageMode: languageMode ?? null,
    loopback,
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

/** Synthesize speech via the local TTS provider; rejects if none is available. */
export function ttsSpeak(text: string): Promise<TtsResponse> {
  return invoke<TtsResponse>("tts_speak", { text });
}
