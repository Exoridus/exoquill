import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ActionBar } from "./components/ActionBar";
import { Editor } from "./components/Editor";
import { PlusIcon } from "./components/icons";
import { Sidebar } from "./components/Sidebar";
import { Statusbar } from "./components/Statusbar";
import { Topbar } from "./components/Topbar";
import { useTheme } from "./hooks/useTheme";
import * as api from "./lib/api";
import type { Note, NoteUpdate } from "./lib/types";
import "./styles/app.css";

function sortNotes(notes: Note[]): Note[] {
  return [...notes].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.updatedAt.localeCompare(a.updatedAt);
  });
}

function noteMeta(note: Note): string {
  const text = note.contentMarkdown.trim();
  const words = text ? text.split(/\s+/).length : 0;
  return `${text ? "DRAFT" : "EMPTY"} · ${words} WORDS`;
}

export default function App() {
  const [theme, toggleTheme] = useTheme();
  const [notes, setNotes] = useState<Note[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [saved, setSaved] = useState(true);

  const activeNote = useMemo(
    () => notes.find((n) => n.id === activeId) ?? null,
    [notes, activeId],
  );

  // Load notes — full list, or search results when a query is set.
  const load = useCallback(async (q: string) => {
    const list = q.trim() ? await api.searchNotes(q) : await api.listNotes();
    const sorted = sortNotes(list);
    setNotes(sorted);
    setActiveId((cur) => (cur && sorted.some((n) => n.id === cur) ? cur : sorted[0]?.id ?? null));
  }, []);

  useEffect(() => {
    const t = window.setTimeout(() => void load(query), query ? 200 : 0);
    return () => clearTimeout(t);
  }, [query, load]);

  // Debounced autosave: optimistic local update now, persist after a pause.
  const saveTimer = useRef<number | null>(null);
  const pending = useRef<NoteUpdate>({});

  const scheduleSave = useCallback((id: string, patch: NoteUpdate) => {
    pending.current = { ...pending.current, ...patch };
    setSaved(false);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(async () => {
      const update = pending.current;
      pending.current = {};
      const updated = await api.updateNote(id, update);
      if (updated) {
        setNotes((prev) => sortNotes(prev.map((n) => (n.id === id ? updated : n))));
      }
      setSaved(true);
    }, 450);
  }, []);

  const patchActive = useCallback(
    (patch: NoteUpdate) => {
      if (!activeId) return;
      setNotes((prev) => prev.map((n) => (n.id === activeId ? { ...n, ...patch } : n)));
      scheduleSave(activeId, patch);
    },
    [activeId, scheduleSave],
  );

  const newNote = useCallback(async () => {
    const note = await api.createNote("");
    setQuery("");
    setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)]));
    setActiveId(note.id);
  }, []);

  const deleteActive = useCallback(async () => {
    if (!activeNote) return;
    const id = activeNote.id;
    const idx = notes.findIndex((n) => n.id === id);
    const fallback = notes[idx + 1] ?? notes[idx - 1] ?? null;
    await api.deleteNote(id);
    setNotes((prev) => prev.filter((n) => n.id !== id));
    setActiveId(fallback?.id ?? null);
  }, [activeNote, notes]);

  return (
    <div className="app">
      <Topbar theme={theme} onToggleTheme={toggleTheme} />
      <div className="body">
        <Sidebar
          notes={notes}
          activeId={activeId}
          query={query}
          onQueryChange={setQuery}
          onSelect={setActiveId}
          onNewNote={() => void newNote()}
        />
        <main className="editor-pane">
          {activeNote ? (
            <>
              <ActionBar onDelete={() => void deleteActive()} />
              <div className="editor-scroll">
                <input
                  className="editor-title"
                  value={activeNote.title}
                  placeholder="Untitled note"
                  onChange={(e) => patchActive({ title: e.target.value })}
                />
                <div className="editor-meta">{noteMeta(activeNote)}</div>
                <Editor
                  key={activeNote.id}
                  initialMarkdown={activeNote.contentMarkdown}
                  onChange={(md) => patchActive({ contentMarkdown: md })}
                />
              </div>
              <Statusbar note={activeNote} saved={saved} />
            </>
          ) : (
            <div className="empty-state">
              <div className="empty-state__title">No note selected</div>
              <div>Start with a note, dictate something, or paste a screenshot.</div>
              <button
                className="btn-primary"
                style={{ width: "auto", padding: "8px 16px" }}
                onClick={() => void newNote()}
              >
                <PlusIcon size={14} />
                New note
              </button>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
