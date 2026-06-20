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

/** A recorded note event (format/OCR history + undo safety net). */
export interface NoteEvent {
  id: string;
  noteId: string;
  sourceType: string;
  rawText: string | null;
  processedText: string | null;
  operation: string | null;
  providerId: string | null;
  modelId: string | null;
  modelVersion: string | null;
  createdAt: string;
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

/** A live, in-progress dictation transcript: a frozen `stable` prefix (words the
 *  backend committed via LocalAgreement-2) plus a still-tentative `tail`. */
export interface PartialTranscript {
  stable: string;
  tail: string;
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

/** The frozen full-screen capture for the region-OCR overlay (PNG data URL). */
export interface RegionCapture {
  dataUrl: string;
}

/** A selected screen region: cropped image (PNG data URL) + its OCR layout. */
export interface RegionOcr {
  dataUrl: string;
  layout: OcrLayout;
}

// Backend event bus payloads (tagged by `type`); see exoquill-core::events.
export type BackendEvent =
  | { type: "job_updated"; job: Job }
  | { type: "job_progress"; id: string; progress: number }
  | { type: "notes_changed" }
  | { type: "error"; message: string };
