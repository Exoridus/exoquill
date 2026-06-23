// Mirrors the Rust types in exoquill-core (serde camelCase over the IPC bridge).

export type NoteSource = "manual" | "dictation" | "ocr";

export interface Note {
  id: string;
  title: string;
  /** True while the title auto-follows the content (the user hasn't named it). */
  titleAuto: boolean;
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

/** Which slice of notes a listing returns (the sidebar's scope tabs). */
export type NoteScope = "active" | "archived" | "trash";

/** Sort order for a note listing, applied within the pinned/un-pinned split. */
export type NoteSort = "modified" | "created" | "title";

/** A stored content snapshot for the edit-history diff timeline. `source` is
 *  "manual" (a typing-pause snapshot) or "op" (written by an operation); `op`
 *  names that operation (e.g. "format", "ocr", "dictation", "restore"). */
export interface NoteVersion {
  id: string;
  noteId: string;
  createdAt: string;
  contentMarkdown: string;
  contentHash: string;
  source: string;
  op: string | null;
  providerId: string | null;
}

/** Input for recording a NoteVersion (id/createdAt/hash filled in by the DB). */
export interface NewNoteVersion {
  noteId: string;
  contentMarkdown: string;
  /** "manual" | "op"; defaults to "manual" when omitted. */
  source?: string;
  op?: string;
  providerId?: string;
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

/** An installable model/voice in the catalog, with its on-disk status. */
export interface CatalogItem {
  id: string;
  provider: string;
  kind: string;
  displayName: string;
  language: string;
  license: string;
  commercialOk: boolean;
  /** "bundled" | "download" | "gated". */
  tier: string;
  /** Setup-script path for runtimes that aren't a plain file download (XTTS). */
  setup: string | null;
  notes: string | null;
  installed: boolean;
  installedBytes: number;
}

/** Download progress for a model file (the `model_progress` backend event). */
export interface ModelProgress {
  id: string;
  file: string;
  downloaded: number;
  total: number;
}

/** Read-only summary of the provider behind an AI capability (settings/about). */
export interface ModelInfo {
  feature: string;
  providerId: string;
  displayName: string;
  version: string;
  status: string;
  runtimeLicense: string;
  source: string | null;
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
  /** Base64 of 16-bit little-endian mono PCM (decoded in audio.ts). */
  pcm: string;
  sampleRate: number;
}

/** A selectable read-aloud voice offered by the local TTS provider. */
export interface TtsVoice {
  id: string;
  displayName: string;
  language: string;
  quality: string;
  /** Synthesis backend this voice belongs to (`"piper"` | `"xtts"`); passed
   *  back to `ttsSpeak` so the request is routed to the right provider. */
  provider: string;
}

/** Read-aloud synthesis knobs. `speed` applies to every backend; the rest are
 *  backend-specific (a provider applies only the ones it understands). Omitted
 *  fields fall back to model defaults. */
export interface TtsTuning {
  /** Speaking rate; 1.0 = normal, >1 faster. */
  speed?: number;
  /** Piper expressiveness / timbre variation (noise_scale). */
  expressiveness?: number;
  /** Piper cadence variability (noise_w). */
  cadence?: number;
  /** Seconds of silence after each sentence (Piper). */
  sentenceSilence?: number;
  /** Zonos intonation liveliness (pitch_std); low monotone, high lively. */
  intonation?: number;
  /** Zonos synthesis frequency ceiling in Hz (fmax); lower warmer, higher brighter. */
  brightness?: number;
  /** Zonos emotion preset key (see `lib/tts.ts`); resolved to an 8-value vector
   *  before synthesis. `"neutral"`/omitted leaves Zonos' own default. */
  emotion?: string;
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

/** Dictation capture tuning (persisted UI settings; flow into `startDictation`). */
export interface DictationOpts {
  autoGain: boolean;
  gain: number;
  useSilero: boolean;
}

/** Editor display preferences (persisted UI settings; applied as CSS variables). */
export interface EditorPrefs {
  /** Multiplier on the editor font size (1.0 = default). */
  fontScale: number;
  /** Max content column width in px. */
  contentWidth: number;
}
