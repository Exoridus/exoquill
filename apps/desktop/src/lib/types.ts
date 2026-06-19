// Mirrors the Rust types in exoquill-core (serde camelCase over the IPC bridge).

export type NoteSource = "manual" | "dictation" | "ocr";

export interface Note {
  id: string;
  title: string;
  contentMarkdown: string;
  createdAt: string;
  updatedAt: string;
  pinned: boolean;
  archived: boolean;
  deletedAt: string | null;
  languageMode: string;
  lastCursorPosition: number;
}

export interface NoteUpdate {
  title?: string;
  contentMarkdown?: string;
  pinned?: boolean;
  archived?: boolean;
  languageMode?: string;
  lastCursorPosition?: number;
}

export type JobStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export interface Job {
  id: string;
  jobType: string;
  status: JobStatus;
  noteId: string | null;
  progress: number;
  error: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface TtsResponse {
  samples: number[];
  sampleRate: number;
}

/** A dictation source: a microphone, or an output device captured via loopback. */
export interface CaptureSource {
  name: string;
  loopback: boolean;
}

/** One OCR word with its bounding box in the recognized image's pixel space. */
export interface OcrWord {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  confidence: number;
}

/** Structured OCR result: layout-preserving text + word boxes for the overlay.
 *  `width`/`height` are the pixel space the boxes live in (0 with the mock). */
export interface OcrLayout {
  text: string;
  words: OcrWord[];
  width: number;
  height: number;
}

// Backend event bus payloads (tagged by `type`); see exoquill-core::events.
export type BackendEvent =
  | { type: "job_updated"; job: Job }
  | { type: "job_progress"; id: string; progress: number }
  | { type: "notes_changed" }
  | { type: "error"; message: string };
