// Typed wrappers around the Tauri notes commands (see src-tauri/src/notes.rs).
import { invoke } from "@tauri-apps/api/core";

import type { Job, Note, NoteSource, NoteUpdate } from "./types";

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
