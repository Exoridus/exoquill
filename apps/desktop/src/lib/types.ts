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
