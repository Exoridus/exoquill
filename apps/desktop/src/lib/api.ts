// Typed wrappers around the Tauri notes commands (see src-tauri/src/notes.rs).
import { invoke } from "@tauri-apps/api/core";

import type { Note, NoteSource, NoteUpdate } from "./types";

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
