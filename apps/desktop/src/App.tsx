import { type Editor as TiptapEditor } from "@tiptap/react";
import { listen } from "@tauri-apps/api/event";
import { type ChangeEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ActionBar } from "./components/ActionBar";
import { Editor, insertAtCursor, replaceSelection, selectionText } from "./components/Editor";
import { PlusIcon } from "./components/icons";
import { Sidebar } from "./components/Sidebar";
import { Statusbar } from "./components/Statusbar";
import { Topbar } from "./components/Topbar";
import { useTheme } from "./hooks/useTheme";
import * as api from "./lib/api";
import { playSamples, stopPlayback } from "./lib/audio";
import { type DictationSession, startDictation } from "./lib/dictation";
import { speak, stopSpeaking } from "./lib/speech";
import type { BackendEvent, Note, NoteUpdate } from "./lib/types";
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
  const [formatting, setFormatting] = useState(false);
  const [reading, setReading] = useState(false);
  const [dictating, setDictating] = useState(false);
  // Bumped when a note's content changes out-of-band (format/OCR job) to
  // remount the editor so it picks up the new Markdown.
  const [reloadKey, setReloadKey] = useState(0);

  const editorRef = useRef<TiptapEditor | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dictationRef = useRef<DictationSession | null>(null);

  const activeNote = useMemo(
    () => notes.find((n) => n.id === activeId) ?? null,
    [notes, activeId],
  );

  const load = useCallback(async (q: string) => {
    const list = q.trim() ? await api.searchNotes(q) : await api.listNotes();
    const sorted = sortNotes(list);
    setNotes(sorted);
    setActiveId((cur) => (cur && sorted.some((n) => n.id === cur) ? cur : sorted[0]?.id ?? null));
  }, []);

  const queryRef = useRef(query);
  queryRef.current = query;

  useEffect(() => {
    const t = window.setTimeout(() => void load(query), query ? 200 : 0);
    return () => clearTimeout(t);
  }, [query, load]);

  // Stop any read-aloud when switching notes.
  useEffect(() => {
    stopSpeaking();
    stopPlayback();
    setReading(false);
  }, [activeId]);

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

  const flushSave = useCallback(async () => {
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = null;
    }
    if (activeId && Object.keys(pending.current).length > 0) {
      const update = pending.current;
      pending.current = {};
      await api.updateNote(activeId, update);
      setSaved(true);
    }
  }, [activeId]);

  const patchActive = useCallback(
    (patch: NoteUpdate) => {
      if (!activeId) return;
      setNotes((prev) => prev.map((n) => (n.id === activeId ? { ...n, ...patch } : n)));
      scheduleSave(activeId, patch);
    },
    [activeId, scheduleSave],
  );

  // Backend job/event-bus messages.
  useEffect(() => {
    const unlisten = listen<BackendEvent>("backend-event", ({ payload }) => {
      if (payload.type === "job_updated") {
        const { job } = payload;
        const terminal =
          job.status === "completed" || job.status === "failed" || job.status === "cancelled";
        if (terminal) {
          if (job.jobType === "format") setFormatting(false);
          if (job.status === "failed" && job.error) console.error("Job failed:", job.error);
          void load(queryRef.current).then(() => setReloadKey((k) => k + 1));
        }
      } else if (payload.type === "notes_changed") {
        void load(queryRef.current);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  const handleEditorReady = useCallback((editor: TiptapEditor) => {
    editorRef.current = editor;
  }, []);

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

  // Format the selection (direct replace + undo), or the whole note via the
  // job queue when nothing is selected (decisions D6).
  const formatActive = useCallback(async () => {
    const editor = editorRef.current;
    const selection = selectionText(editor);
    if (editor && selection.trim()) {
      try {
        const formatted = await api.formatText(selection);
        replaceSelection(editor, formatted);
      } catch (err) {
        console.error("format selection failed:", err);
      }
      return;
    }
    if (!activeId) return;
    setFormatting(true);
    try {
      await flushSave();
      await api.formatNote(activeId);
    } catch (err) {
      setFormatting(false);
      console.error("format failed:", err);
    }
  }, [activeId, flushSave]);

  // Read the selection, or the whole note, aloud — toggling stop. Uses the
  // local Piper TTS when available, else the webview's system speech.
  const readActive = useCallback(async () => {
    if (reading) {
      stopSpeaking();
      stopPlayback();
      setReading(false);
      return;
    }
    const selection = selectionText(editorRef.current);
    const text = selection.trim() ? selection : activeNote?.contentMarkdown ?? "";
    if (!text.trim()) return;
    setReading(true);
    try {
      const audio = await api.ttsSpeak(text);
      playSamples(audio.samples, audio.sampleRate, () => setReading(false));
    } catch {
      speak(text, { onEnd: () => setReading(false) });
    }
  }, [reading, activeNote]);

  // Toggle microphone dictation. Captured audio is segmented in the webview and
  // transcribed locally by Whisper; each finalized segment is inserted at the
  // cursor. Starting without an active note auto-creates a dictation note.
  const toggleDictation = useCallback(async () => {
    if (dictating) {
      const session = dictationRef.current;
      dictationRef.current = null;
      setDictating(false);
      await session?.stop();
      return;
    }
    if (!activeId) {
      const note = await api.createNote("", "dictation");
      setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)]));
      setActiveId(note.id);
    }
    try {
      const session = await startDictation({
        languageMode: activeNote?.languageMode,
        onText: (text) => {
          const editor = editorRef.current;
          if (editor) insertAtCursor(editor, text);
        },
        onError: (message) => console.error("dictation:", message),
      });
      dictationRef.current = session;
      setDictating(true);
    } catch (err) {
      console.error("could not start dictation:", err);
      setDictating(false);
    }
  }, [dictating, activeId, activeNote]);

  // Release the microphone if the app unmounts mid-dictation.
  useEffect(() => {
    return () => {
      void dictationRef.current?.stop();
    };
  }, []);

  // Global Quick-Note shortcut (Ctrl+Alt+N) and tray "New Note" → create a note.
  useEffect(() => {
    const unlisten = listen("quick-note", () => void newNote());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [newNote]);

  const triggerOcr = useCallback(() => fileInputRef.current?.click(), []);

  const handleOcrFile = useCallback(
    async (e: ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      e.target.value = ""; // allow re-selecting the same file
      if (!file || !activeId) return;
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      try {
        await flushSave();
        await api.runOcr(activeId, bytes);
      } catch (err) {
        console.error("ocr failed:", err);
      }
    },
    [activeId, flushSave],
  );

  return (
    <div className="app">
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        onChange={handleOcrFile}
        style={{ display: "none" }}
      />
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
              <ActionBar
                onDictate={() => void toggleDictation()}
                onOcr={triggerOcr}
                onFormat={() => void formatActive()}
                onRead={() => void readActive()}
                onDelete={() => void deleteActive()}
                dictating={dictating}
                formatting={formatting}
                reading={reading}
              />
              <div className="editor-scroll">
                <input
                  className="editor-title"
                  value={activeNote.title}
                  placeholder="Untitled note"
                  onChange={(e) => patchActive({ title: e.target.value })}
                />
                <div className="editor-meta">{noteMeta(activeNote)}</div>
                <Editor
                  key={`${activeNote.id}:${reloadKey}`}
                  initialMarkdown={activeNote.contentMarkdown}
                  onChange={(md) => patchActive({ contentMarkdown: md })}
                  onReady={handleEditorReady}
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
