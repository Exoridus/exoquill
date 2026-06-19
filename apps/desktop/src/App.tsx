import { type Editor as TiptapEditor } from "@tiptap/react";
import { listen } from "@tauri-apps/api/event";
import { type ChangeEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ActionBar } from "./components/ActionBar";
import { Editor, insertAtCursor, replaceSelection, selectionText } from "./components/Editor";
import { OcrOverlay } from "./components/OcrOverlay";
import { PlusIcon } from "./components/icons";
import { Sidebar } from "./components/Sidebar";
import { Statusbar } from "./components/Statusbar";
import { Topbar } from "./components/Topbar";
import { useTheme } from "./hooks/useTheme";
import * as api from "./lib/api";
import { playSamples, stopPlayback } from "./lib/audio";
import { startDictation, stopDictation, subscribeDictation } from "./lib/dictation";
import { speak, stopSpeaking } from "./lib/speech";
import type { BackendEvent, CaptureSource, Note, NoteUpdate, OcrLayout } from "./lib/types";
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
  const [micLevel, setMicLevel] = useState(0);
  const [dictationError, setDictationError] = useState<string | null>(null);
  const [sources, setSources] = useState<CaptureSource[]>([]);
  // The chosen dictation source; `null` is the system default microphone.
  const [source, setSource] = useState<CaptureSource | null>(null);
  // The open OCR result overlay (pasted/picked image), and whether OCR is running.
  const [ocr, setOcr] = useState<{ url: string; layout: OcrLayout } | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  // Bumped when a note's content changes out-of-band (format/OCR job) to
  // remount the editor so it picks up the new Markdown.
  const [reloadKey, setReloadKey] = useState(0);

  const editorRef = useRef<TiptapEditor | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Guards against re-entering the async start/stop (e.g. a double-click).
  const dictationBusy = useRef(false);

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
    if (dictationBusy.current) return;
    dictationBusy.current = true;
    try {
      if (dictating) {
        await stopDictation();
      } else {
        setDictationError(null);
        let language = activeNote?.languageMode;
        if (!activeId) {
          const note = await api.createNote("", "dictation");
          setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)]));
          setActiveId(note.id);
          language = note.languageMode;
        }
        await startDictation(source?.name, language, source?.loopback ?? false);
      }
    } catch (err) {
      setDictationError(String(err));
      setDictating(false);
    } finally {
      dictationBusy.current = false;
    }
  }, [dictating, activeId, activeNote, source]);

  // Load the available dictation sources (mics + loopback) once.
  useEffect(() => {
    void api.listCaptureSources().then(setSources).catch(() => setSources([]));
  }, []);

  // Subscribe once to the backend's live dictation events: insert each
  // transcript chunk at the cursor, drive the level meter, surface errors, and
  // track recording state. Stop capture if the app unmounts mid-dictation.
  useEffect(() => {
    const unsub = subscribeDictation({
      onSegment: (text) => {
        const editor = editorRef.current;
        if (editor) insertAtCursor(editor, text);
      },
      onLevel: setMicLevel,
      onError: setDictationError,
      onStarted: () => {
        setDictating(true);
        setDictationError(null);
      },
      onStopped: () => {
        setDictating(false);
        setMicLevel(0);
      },
    });
    return () => {
      void unsub.then((fn) => fn());
      void stopDictation();
    };
  }, []);

  // Auto-dismiss a dictation error after a few seconds.
  useEffect(() => {
    if (!dictationError) return;
    const t = window.setTimeout(() => setDictationError(null), 5000);
    return () => clearTimeout(t);
  }, [dictationError]);

  // Global Quick-Note shortcut (Ctrl+Alt+N) and tray "New Note" → create a note.
  useEffect(() => {
    const unlisten = listen("quick-note", () => void newNote());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [newNote]);

  const triggerOcr = useCallback(() => fileInputRef.current?.click(), []);

  const closeOcr = useCallback(() => {
    setOcr((cur) => {
      if (cur) URL.revokeObjectURL(cur.url);
      return null;
    });
  }, []);

  // Run OCR on an image blob (pasted or picked) and open the result overlay.
  const openOcr = useCallback(async (blob: Blob) => {
    const url = URL.createObjectURL(blob);
    setOcrBusy(true);
    try {
      const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
      const layout = await api.ocrImage(bytes);
      setOcr((cur) => {
        if (cur) URL.revokeObjectURL(cur.url);
        return { url, layout };
      });
    } catch (err) {
      URL.revokeObjectURL(url);
      setDictationError(`Texterkennung fehlgeschlagen: ${String(err)}`);
    } finally {
      setOcrBusy(false);
    }
  }, []);

  // Insert OCR text (a selection or the whole result) into the active note, or
  // create an OCR note when none is active.
  const insertOcrText = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      closeOcr();
      if (!trimmed) return;
      const editor = editorRef.current;
      if (activeId && editor) {
        insertAtCursor(editor, trimmed);
      } else {
        const note = await api.createNote(trimmed, "ocr");
        setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)]));
        setActiveId(note.id);
      }
    },
    [activeId, closeOcr],
  );

  const handleOcrFile = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      e.target.value = ""; // allow re-selecting the same file
      if (file) void openOcr(file);
    },
    [openOcr],
  );

  // Paste an image (Ctrl+V) anywhere → OCR it and open the result overlay.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const image = Array.from(e.clipboardData?.items ?? []).find((item) =>
        item.type.startsWith("image/"),
      );
      const file = image?.getAsFile();
      if (file) {
        e.preventDefault();
        void openOcr(file);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [openOcr]);

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
              {!dictating && sources.length > 0 && (
                <div className="dictation-source">
                  <span className="dictation-source__label">Diktat-Quelle</span>
                  <select
                    className="dictation-source__select"
                    value={source ? String(sources.indexOf(source)) : ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      setSource(v === "" ? null : sources[Number(v)] ?? null);
                    }}
                  >
                    <option value="">Standard-Mikrofon</option>
                    {sources.map((s, i) => (
                      <option key={`${s.loopback ? "out" : "in"}:${s.name}`} value={String(i)}>
                        {(s.loopback ? "🔊 " : "🎙 ") + s.name}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              {dictating && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">Aufnahme läuft…</span>
                  <span className="dictation-bar__meter">
                    <span
                      className="dictation-bar__level"
                      style={{ width: `${Math.round(Math.min(1, micLevel) * 100)}%` }}
                    />
                  </span>
                </div>
              )}
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
      {dictationError && (
        <div className="dictation-error" role="alert">
          {dictationError}
        </div>
      )}
      {ocrBusy && <div className="ocr-busy">Texterkennung läuft…</div>}
      {ocr && (
        <OcrOverlay
          imageUrl={ocr.url}
          layout={ocr.layout}
          onInsert={(text) => void insertOcrText(text)}
          onClose={closeOcr}
        />
      )}
    </div>
  );
}
