import { type Editor as TiptapEditor } from "@tiptap/react";
import { listen } from "@tauri-apps/api/event";
import {
  type ChangeEvent,
  type MouseEvent as ReactMouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { ActionBar } from "./components/ActionBar";
import {
  Editor,
  editorMarkdown,
  insertAtCursor,
  replaceRange,
  replaceSelection,
  selectionText,
  separatorBefore,
} from "./components/Editor";
import { HistoryOverlay } from "./components/HistoryOverlay";
import { ModelManager } from "./components/ModelManager";
import { OcrOverlay } from "./components/OcrOverlay";
import { ReadAloudSettings, TTS_DEFAULTS } from "./components/ReadAloudSettings";
import { ArchiveIcon, PlusIcon, RestoreIcon, TrashIcon } from "./components/icons";
import { Sidebar } from "./components/Sidebar";
import { Statusbar } from "./components/Statusbar";
import { ToastStack, useToasts } from "./components/Toasts";
import { useTheme } from "./hooks/useTheme";
import * as api from "./lib/api";
import { getLang, translate, useI18n } from "./lib/i18n";
import type { I18n, TranslationKey } from "./lib/i18n";
import { decodePcm, playSamples, stopPlayback } from "./lib/audio";
import { chunkMarkdown, cleanDictation } from "./lib/format";
import { startDictation, stopDictation, subscribeDictation } from "./lib/dictation";
import { plainSource, preparedSource, readAloud, type ReadAloudHandle } from "./lib/readaloud";
import { stopSpeaking } from "./lib/speech";
import { ZONOS_EMOTIONS } from "./lib/tts";
import type {
  BackendEvent,
  CaptureSource,
  CatalogItem,
  ModelInfo,
  ModelProgress,
  Note,
  NoteScope,
  NoteSort,
  NoteUpdate,
  NoteVersion,
  OcrLayout,
  RegionOcr,
  TtsTuning,
  TtsResponse,
  TtsVoice,
} from "./lib/types";
import "./styles/app.css";

/** Sort a note list to match the backend ordering: pinned first, then by the
 *  chosen key (so optimistic local updates keep the same order as a reload). */
function sortNotes(notes: Note[], sort: NoteSort): Note[] {
  return [...notes].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    if (sort === "title") return a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
    if (sort === "created") return b.createdAt.localeCompare(a.createdAt);
    return b.updatedAt.localeCompare(a.updatedAt);
  });
}

function noteMeta(note: Note, t: I18n["t"]): string {
  const text = note.contentMarkdown.trim();
  const words = text ? text.split(/\s+/).length : 0;
  return `${text ? t("meta.draft") : t("meta.empty")} · ${t("meta.words", { count: words })}`;
}

// Display labels for the TTS backends behind the toolbar's backend picker.
const BACKEND_LABELS: Record<string, string> = { piper: "Piper", xtts: "XTTS", zonos: "Zonos" };
const backendLabel = (provider: string) => BACKEND_LABELS[provider] ?? provider;

export default function App() {
  const { t, lang, setLang } = useI18n();
  const [theme, toggleTheme] = useTheme();
  const [notes, setNotes] = useState<Note[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  // The sidebar scope (Active / Archived / Trash) and sort order (persisted).
  const [scope, setScope] = useState<NoteScope>("active");
  const [sort, setSort] = useState<NoteSort>(
    () => (localStorage.getItem("notes-sort") as NoteSort) || "modified",
  );
  // Multi-selected note ids (non-empty → the sidebar is in selection mode).
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  // Undo-toast queue for reversible/destructive note actions.
  const toasts = useToasts();
  // Set when a rename was requested, to focus + select the title input once.
  const [pendingTitleFocus, setPendingTitleFocus] = useState(false);
  const [saved, setSaved] = useState(true);
  const [formatting, setFormatting] = useState(false);
  const [reading, setReading] = useState(false);
  // Whether audio has actually started (vs. still preparing/buffering). Drives
  // the toolbar: cancel-only while preparing, pause/stop once speaking.
  const [speaking, setSpeaking] = useState(false);
  // Whether read-aloud is currently paused (suspended AudioContext).
  const [paused, setPaused] = useState(false);
  const [dictating, setDictating] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [dictationError, setDictationError] = useState<string | null>(null);
  const [sources, setSources] = useState<CaptureSource[]>([]);
  // The chosen dictation source; `null` is the system default microphone.
  const [source, setSource] = useState<CaptureSource | null>(null);
  // The read-aloud voices the local TTS offers, and the chosen one (persisted).
  // Empty `voices` means no local TTS — read-aloud uses system speech instead.
  const [voices, setVoices] = useState<TtsVoice[]>([]);
  const [voiceId, setVoiceId] = useState<string>(() => localStorage.getItem("tts-voice") ?? "");
  // Read-aloud synthesis knobs (persisted), and whether the settings dialog is open.
  const [tuning, setTuning] = useState<TtsTuning>(() => {
    try {
      return { ...TTS_DEFAULTS, ...JSON.parse(localStorage.getItem("tts-tuning") ?? "{}") };
    } catch {
      return { ...TTS_DEFAULTS };
    }
  });
  const [showVoiceSettings, setShowVoiceSettings] = useState(false);
  // Chunked-format progress (null = idle), and whether to run an LLM "prepare for
  // speech" pass before read-aloud (persisted).
  const [formatProgress, setFormatProgress] = useState<{ done: number; total: number } | null>(
    null,
  );
  // Progress of the Zonos prebuffer synthesis (whole-text-then-play), null = idle.
  const [synthProgress, setSynthProgress] = useState<{ done: number; total: number } | null>(null);
  // Shows a brief "voice still loading" hint when a read starts before the chosen
  // sidecar voice is warm (instead of playing a wrong/robot voice).
  const [voiceLoading, setVoiceLoading] = useState(false);
  // Whether the settings dialog's one-sentence preview is currently rendering.
  const [previewing, setPreviewing] = useState(false);
  const [speechPrep, setSpeechPrep] = useState<boolean>(
    () => localStorage.getItem("tts-speech-prep") === "1",
  );
  // The open OCR result overlay (pasted/picked image), and whether OCR is running.
  const [ocr, setOcr] = useState<{ url: string; layout: OcrLayout } | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  // The open edit-history overlay's versions (null = closed).
  const [history, setHistory] = useState<NoteVersion[] | null>(null);
  // The on-device provider summaries (shown inside the model manager).
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  // The model manager window: open flag, catalog, live download progress, and
  // the entry currently installing.
  const [showModels, setShowModels] = useState(false);
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
  const [modelProgress, setModelProgress] = useState<Record<string, ModelProgress>>({});
  const [modelBusy, setModelBusy] = useState<string | null>(null);
  // The open formatting preview (original vs formatted), with its apply action.
  const [preview, setPreview] = useState<{
    original: string;
    formatted: string;
    onApply: () => void | Promise<void>;
  } | null>(null);
  // Bumped when a note's content changes out-of-band (format/OCR job) to
  // remount the editor so it picks up the new Markdown.
  const [reloadKey, setReloadKey] = useState(0);

  const editorRef = useRef<TiptapEditor | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);
  // The note id last clicked in the sidebar (anchor for Shift-range selection).
  const lastClickedId = useRef<string | null>(null);
  // Guards against re-entering the async start/stop (e.g. a double-click).
  const dictationBusy = useRef(false);
  // Where dictated text lands, decided when dictation starts: "replace" the
  // initial selection (first segment) then continue at the cursor, or "cursor"
  // (insert at the caret — also covers append-to-end when nothing was focused).
  const dictationMode = useRef<"replace" | "cursor">("cursor");
  // The live-committed region of the current utterance: its document anchor, the
  // separating-space prefix, and the text written so far. While speaking, the
  // stabilized prefix is committed into the note (real text) and this region is
  // overwritten as it grows; the final segment overwrites it authoritatively.
  // `null` between utterances.
  const utterance = useRef<{ from: number; prefix: string; committed: string } | null>(null);
  // The running read-aloud queue, if any (so it can be stopped).
  const readHandle = useRef<ReadAloudHandle | null>(null);
  // Cache of speech-prep results, keyed by raw chunk text. The LLM rewrite is
  // backend-independent, so switching voice/backend (or re-reading the same note)
  // reuses already-prepared chunks instead of running the LLM again.
  const prepCache = useRef<Map<string, string>>(new Map());
  // Cache of synthesized audio, keyed by voice+tuning+text. Same voice & text →
  // replay the stored audio instead of re-synthesizing. `lastAudioKey` is the most
  // recently rendered key, which the Export-Audio button saves to a WAV.
  const audioCache = useRef<Map<string, TtsResponse[]>>(new Map());
  const [lastAudioKey, setLastAudioKey] = useState<string | null>(null);

  // The open note stays available even when the sidebar shows another scope
  // (e.g. after archiving it, or while viewing Trash): keep the last-seen copy.
  const activeNoteCache = useRef<Note | null>(null);
  const activeNote = useMemo(() => {
    const found = notes.find((n) => n.id === activeId) ?? null;
    if (found) activeNoteCache.current = found;
    if (found) return found;
    return activeNoteCache.current?.id === activeId ? activeNoteCache.current : null;
  }, [notes, activeId]);

  const load = useCallback(async (q: string, sc: NoteScope, so: NoteSort) => {
    const list = q.trim() ? await api.searchNotes(q, sc) : await api.listNotes(sc, so);
    setNotes(sortNotes(list, so));
    // Open the first note only on the very first Active view with nothing open;
    // otherwise keep the current selection (it may now live in another scope).
    setActiveId((cur) => cur ?? (sc === "active" ? list[0]?.id ?? null : null));
  }, []);

  // Refs so stable action callbacks can reload with the latest view params.
  const queryRef = useRef(query);
  queryRef.current = query;
  const scopeRef = useRef(scope);
  scopeRef.current = scope;
  const sortRef = useRef(sort);
  sortRef.current = sort;
  const notesRef = useRef(notes);
  notesRef.current = notes;
  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;

  const reload = useCallback(
    () => load(queryRef.current, scopeRef.current, sortRef.current),
    [load],
  );

  useEffect(() => {
    const handle = window.setTimeout(() => void load(query, scope, sort), query ? 200 : 0);
    return () => clearTimeout(handle);
  }, [query, scope, sort, load]);

  // Persist the sort choice.
  useEffect(() => {
    localStorage.setItem("notes-sort", sort);
  }, [sort]);

  // Switching scope clears any multi-selection (it's scope-specific).
  const changeScope = useCallback((next: NoteScope) => {
    setScope(next);
    setSelected(new Set());
  }, []);

  // Focus + select the title input once after a "rename" request.
  useEffect(() => {
    if (pendingTitleFocus && titleInputRef.current) {
      titleInputRef.current.focus();
      titleInputRef.current.select();
      setPendingTitleFocus(false);
    }
  }, [pendingTitleFocus, activeNote]);

  // Stop any read-aloud when switching notes.
  useEffect(() => {
    readHandle.current?.stop();
    readHandle.current = null;
    stopSpeaking();
    stopPlayback();
    setReading(false);
  }, [activeId]);

  // On leaving a note, snapshot its current content as a manual history baseline
  // (deduped in the backend, so an unchanged note adds nothing).
  useEffect(() => {
    const leaving = activeId;
    return () => {
      if (!leaving) return;
      const note = notesRef.current.find((n) => n.id === leaving);
      if (note && note.contentMarkdown.trim()) {
        void api.snapshotNoteVersion({ noteId: leaving, contentMarkdown: note.contentMarkdown });
      }
    };
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
        setNotes((prev) => sortNotes(prev.map((n) => (n.id === id ? updated : n)), sortRef.current));
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
          void reload().then(() => setReloadKey((k) => k + 1));
        }
      } else if (payload.type === "notes_changed") {
        void reload();
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [reload]);

  const handleEditorReady = useCallback((editor: TiptapEditor) => {
    editorRef.current = editor;
  }, []);

  const newNote = useCallback(async () => {
    const note = await api.createNote("");
    setQuery("");
    setScope("active"); // a new note belongs to the Active view
    setSelected(new Set());
    setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)], sortRef.current));
    setActiveId(note.id);
  }, []);

  // Drop a note from the visible list (it left the current scope) and, if it was
  // the open one, pick a neighbour so the editor doesn't go blank unexpectedly.
  const removeFromList = useCallback((id: string) => {
    setNotes((prev) => {
      if (activeIdRef.current === id) {
        const idx = prev.findIndex((n) => n.id === id);
        const fallback = prev[idx + 1] ?? prev[idx - 1] ?? null;
        setActiveId(fallback?.id ?? null);
      }
      return prev.filter((n) => n.id !== id);
    });
  }, []);

  // --- Sidebar selection (click = open; Ctrl/⌘ or Shift = multi-select) ---
  const toggleSelect = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    lastClickedId.current = id;
  }, []);

  const handleSelect = useCallback(
    (id: string, e: ReactMouseEvent) => {
      if (e.shiftKey && lastClickedId.current) {
        const ids = notesRef.current.map((n) => n.id);
        const a = ids.indexOf(lastClickedId.current);
        const b = ids.indexOf(id);
        if (a !== -1 && b !== -1) {
          const [lo, hi] = a < b ? [a, b] : [b, a];
          setSelected((prev) => {
            const next = new Set(prev);
            for (let i = lo; i <= hi; i++) next.add(ids[i]);
            return next;
          });
        }
        return;
      }
      if (e.ctrlKey || e.metaKey || selected.size > 0) {
        toggleSelect(id);
        return;
      }
      setActiveId(id);
      lastClickedId.current = id;
    },
    [selected.size, toggleSelect],
  );

  // --- Per-note actions (optimistic, with undo toasts) ---
  const handlePin = useCallback(
    async (note: Note) => {
      const pinned = !note.pinned;
      setNotes((prev) =>
        sortNotes(
          prev.map((n) => (n.id === note.id ? { ...n, pinned } : n)),
          sortRef.current,
        ),
      );
      await api.updateNote(note.id, { pinned });
      void reload();
    },
    [reload],
  );

  const handleArchive = useCallback(
    async (note: Note) => {
      removeFromList(note.id);
      await api.setArchived(note.id, true);
      void reload();
      toasts.push(t("toast.archived"), {
        icon: <ArchiveIcon size={15} />,
        actionLabel: t("toast.undo"),
        onAction: async () => {
          await api.setArchived(note.id, false);
          void reload();
        },
      });
    },
    [removeFromList, reload, toasts, t],
  );

  const handleTrash = useCallback(
    async (note: Note) => {
      removeFromList(note.id);
      await api.deleteNote(note.id);
      void reload();
      toasts.push(t("toast.trashed"), {
        icon: <TrashIcon size={15} />,
        actionLabel: t("toast.undo"),
        onAction: async () => {
          await api.restoreNote(note.id);
          void reload();
        },
      });
    },
    [removeFromList, reload, toasts, t],
  );

  // Restore from Archive (un-archive) or Trash (un-delete), with undo.
  const handleRestore = useCallback(
    async (note: Note) => {
      removeFromList(note.id);
      const fromTrash = !!note.deletedAt;
      if (fromTrash) await api.restoreNote(note.id);
      else await api.setArchived(note.id, false);
      void reload();
      toasts.push(t("toast.restored"), {
        icon: <RestoreIcon size={15} />,
        actionLabel: t("toast.undo"),
        onAction: async () => {
          if (fromTrash) await api.deleteNote(note.id);
          else await api.setArchived(note.id, true);
          void reload();
        },
      });
    },
    [removeFromList, reload, toasts, t],
  );

  const handleDeleteForever = useCallback(
    async (note: Note) => {
      removeFromList(note.id);
      await api.hardDeleteNote(note.id);
      void reload();
      toasts.push(t("toast.deletedForever"));
    },
    [removeFromList, reload, toasts, t],
  );

  const handleEmptyTrash = useCallback(async () => {
    setNotes([]);
    // Everything currently trashed was deleted before "now".
    await api.purgeTrash(new Date().toISOString());
    void reload();
    toasts.push(t("toast.deletedForever"));
  }, [reload, toasts, t]);

  const handleDuplicate = useCallback(
    async (note: Note) => {
      const copy = await api.createNote(note.contentMarkdown);
      void reload();
      setActiveId(copy.id);
    },
    [reload],
  );

  const handleRename = useCallback((note: Note) => {
    setActiveId(note.id);
    setPendingTitleFocus(true);
  }, []);

  const handleExportNote = useCallback(
    (note: Note) => {
      void flushSave().then(() => api.exportNote(note.id));
    },
    [flushSave],
  );

  // Delete the open note (action-bar trash) → move to Trash with undo.
  const deleteActive = useCallback(() => {
    if (activeNote) void handleTrash(activeNote);
  }, [activeNote, handleTrash]);

  // --- Bulk actions over the current selection ---
  const handleBulkPin = useCallback(async () => {
    const ids = [...selected];
    setSelected(new Set());
    await Promise.all(ids.map((id) => api.updateNote(id, { pinned: true })));
    void reload();
  }, [selected, reload]);

  const handleBulkArchive = useCallback(async () => {
    const ids = [...selected];
    setSelected(new Set());
    setNotes((prev) => prev.filter((n) => !ids.includes(n.id)));
    await Promise.all(ids.map((id) => api.setArchived(id, true)));
    void reload();
    toasts.push(t("toast.archivedMany", { count: ids.length }), {
      icon: <ArchiveIcon size={15} />,
      actionLabel: t("toast.undo"),
      onAction: async () => {
        await Promise.all(ids.map((id) => api.setArchived(id, false)));
        void reload();
      },
    });
  }, [selected, reload, toasts, t]);

  const handleBulkTrash = useCallback(async () => {
    const ids = [...selected];
    setSelected(new Set());
    setNotes((prev) => prev.filter((n) => !ids.includes(n.id)));
    if (activeIdRef.current && ids.includes(activeIdRef.current)) setActiveId(null);
    await Promise.all(ids.map((id) => api.deleteNote(id)));
    void reload();
    toasts.push(t("toast.trashedMany", { count: ids.length }), {
      icon: <TrashIcon size={15} />,
      actionLabel: t("toast.undo"),
      onAction: async () => {
        await Promise.all(ids.map((id) => api.restoreNote(id)));
        void reload();
      },
    });
  }, [selected, reload, toasts, t]);

  const handleBulkExport = useCallback(async () => {
    const ids = [...selected];
    setSelected(new Set());
    await flushSave();
    // Sequential native save dialogs (WebView2 can't batch downloads).
    for (const id of ids) await api.exportNote(id);
  }, [selected, flushSave]);

  // --- Edit history ---
  const openHistory = useCallback(() => {
    if (activeIdRef.current) void api.listNoteHistory(activeIdRef.current).then(setHistory);
  }, []);

  const restoreVersion = useCallback(
    async (versionId: string) => {
      const id = activeIdRef.current;
      if (!id) return;
      await api.restoreNoteVersion(id, versionId);
      setHistory(null);
      void reload();
      setReloadKey((k) => k + 1);
      toasts.push(t("toast.versionRestored"));
    },
    [reload, toasts, t],
  );

  // Format the selection, or the whole note when nothing is selected, then open
  // a preview (original vs formatted) so the change is applied only on confirm
  // (D6 — replace + undo, now with a look-before-you-leap step).
  const formatActive = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) return;
    const selection = selectionText(editor);
    const isSelection = !!selection.trim();
    const original = isSelection ? selection : editorMarkdown(editor);
    if (!original.trim()) return;
    // Deterministic, instant cleanup — no LLM, so no garbage output and no hang.
    const formatted = cleanDictation(original);
    setPreview({
      original,
      formatted,
      onApply: async () => {
        if (isSelection) {
          replaceSelection(editor, formatted);
          if (activeId) {
            void api.snapshotNoteVersion({
              noteId: activeId,
              contentMarkdown: editorMarkdown(editor),
              source: "op",
              op: "format",
            });
          }
          return;
        }
        if (!activeId) return;
        await api.updateNote(activeId, { contentMarkdown: formatted });
        void api.snapshotNoteVersion({
          noteId: activeId,
          contentMarkdown: formatted,
          source: "op",
          op: "format",
        });
        await reload();
        setReloadKey((k) => k + 1);
      },
    });
  }, [activeId, reload]);

  // Read the selection, or the whole note, aloud — toggling stop. Streams via the
  // read-aloud queue (sentence chunks, local Piper TTS with prefetch, system
  // speech fallback) so playback starts on the first segment, not the whole note.
  const readActive = useCallback(async (retry = false) => {
    // `retry` is the automatic restart after a cold sidecar warmed up — it skips
    // the stop toggle (the previous attempt already tore itself down) and only
    // ever fires once, so a still-failing voice can't loop.
    if (!retry && reading) {
      readHandle.current?.stop();
      readHandle.current = null;
      void api.cancelRead(); // stop any in-flight speech-prep generation
      setReading(false);
      setSpeaking(false);
      setPaused(false);
      setFormatProgress(null);
      setSynthProgress(null);
      return;
    }
    const selection = selectionText(editorRef.current);
    const raw = selection.trim() ? selection : activeNote?.contentMarkdown ?? "";
    if (!raw.trim()) return;
    setReading(true);
    setSpeaking(false);
    setPaused(false);
    setVoiceLoading(false);
    // Route synthesis to the chosen voice's backend (Piper / XTTS).
    const provider = voices.find((v) => v.id === voiceId)?.provider;
    const speak = (chunk: string) => api.ttsSpeak(chunk, voiceId || undefined, provider, tuning);
    // With speech-prep on, rewrite the note for speech chunk-by-chunk and feed
    // each prepared chunk straight into synthesis — playback starts on the first
    // chunk instead of waiting for the whole note (pipelined). `beginRead` arms a
    // cancel token so the Abbrechen button can stop generation mid-chunk.
    // Otherwise speak the raw text directly.
    // Audio cache: same voice + FULL tuning + text (+ speech-prep flag) → replay
    // the stored audio instead of re-synthesizing. The key must include every
    // tuning knob (speed, Piper's expressiveness/cadence/sentence-silence, Zonos'
    // intonation/brightness/emotion) — changing any of them has to invalidate the
    // cache. Survives backend switches.
    const tuneKey = `${tuning.speed ?? ""}/${tuning.expressiveness ?? ""}/${tuning.cadence ?? ""}/${tuning.sentenceSilence ?? ""}/${tuning.intonation ?? ""}/${tuning.brightness ?? ""}/${tuning.emotion ?? ""}`;
    const cacheKey = `${provider ?? ""}|${voiceId}|${tuneKey}|${speechPrep ? 1 : 0}|${raw}`;
    const cachedAudios = audioCache.current.get(cacheKey);
    if (cachedAudios) setLastAudioKey(cacheKey);

    const source = cachedAudios
      ? plainSource("") // unused on a cache hit
      : speechPrep
        ? (() => {
            const chunks = chunkMarkdown(raw);
            setFormatProgress({ done: 0, total: chunks.length });
            void api.beginRead();
            return preparedSource(
              chunks,
              async (chunk) => {
                const cached = prepCache.current.get(chunk);
                if (cached !== undefined) return cached;
                const formatted = await api.prepareSpeech(chunk);
                prepCache.current.set(chunk, formatted); // only on success (cancel throws)
                return formatted;
              },
              (done, total) => setFormatProgress(done >= total ? null : { done, total }),
            );
          })()
        : plainSource(raw);
    // Zonos is slower than real time, so streaming stutters between sentences.
    // For it, synthesize the whole text up front (with progress) and play gapless.
    const prebuffer = provider === "zonos";
    readHandle.current = readAloud(
      source,
      speak,
      () => {
        readHandle.current = null;
        setReading(false);
        setSpeaking(false);
        setPaused(false);
        setFormatProgress(null);
        setSynthProgress(null);
      },
      raw,
      {
        onPlaybackStart: () => setSpeaking(true),
        prebuffer,
        onPrepare: (done, total) =>
          setSynthProgress(done >= total ? null : { done, total }),
        cachedAudios,
        onAudio: (audios) => {
          if (audios.length) {
            audioCache.current.set(cacheKey, audios);
            setLastAudioKey(cacheKey);
          }
        },
        onUnavailable: () => {
          // Chosen sidecar voice isn't warm yet. Keep the "loading" hint up, wait
          // for the warm-up to finish, then restart the read automatically — no
          // second click. The `retry` guard means a still-cold voice gives up
          // after one auto-retry instead of looping.
          readHandle.current = null;
          setReading(false);
          setSpeaking(false);
          setFormatProgress(null);
          setSynthProgress(null);
          if (!provider || retry) {
            setVoiceLoading(false);
            if (provider) void api.warmTts(provider); // best-effort for next time
            return;
          }
          setVoiceLoading(true);
          void (async () => {
            let ready = false;
            try {
              await api.ensureTtsReady(provider);
              ready = true;
            } catch {
              ready = false; // warm-up failed/timed out — drop the hint, stay put
            }
            setVoiceLoading(false);
            if (ready) void readActiveRef.current(true); // warm now → auto-start
          })();
        },
      },
    );
  }, [reading, activeNote, voiceId, voices, tuning, speechPrep]);

  // Always points at the latest `readActive`, so the warm-up handler inside it can
  // restart the read once the cold sidecar is ready (it can't reference itself).
  const readActiveRef = useRef(readActive);
  readActiveRef.current = readActive;

  // Export the most recently rendered read-aloud audio to a WAV file via a native
  // save dialog (WebView2 can't trigger a browser download).
  const exportAudio = useCallback(() => {
    const audios = lastAudioKey ? audioCache.current.get(lastAudioKey) : null;
    if (!audios || !audios.length) return;
    const name = `${(activeNote?.title || translate(getLang(), "readaloud.exportName")).replace(/[^\w.-]+/g, "_")}.wav`;
    void api.exportAudio(
      audios.map((a) => a.pcm),
      audios[0].sampleRate,
      name,
    );
  }, [lastAudioKey, activeNote]);

  // Pause / resume the running read-aloud (suspends the audio; the queue waits).
  const togglePauseRead = useCallback(() => {
    const handle = readHandle.current;
    if (!handle) return;
    if (paused) {
      handle.resume();
      setPaused(false);
    } else {
      handle.pause();
      setPaused(true);
    }
  }, [paused]);

  // One-sentence preview of the current voice + tuning for the settings dialog —
  // a single synthesis (one slice), played at once, for quick A/B tuning.
  const previewVoice = useCallback(async () => {
    if (previewing) return;
    const provider = voices.find((v) => v.id === voiceId)?.provider;
    setPreviewing(true);
    try {
      const audio = await api.ttsSpeak(
        translate(getLang(), "readaloud.sample"),
        voiceId || undefined,
        provider,
        tuning,
      );
      stopPlayback();
      playSamples(decodePcm(audio.pcm), audio.sampleRate);
    } catch (err) {
      console.error("preview failed:", err);
    } finally {
      setPreviewing(false);
    }
  }, [previewing, voiceId, voices, tuning]);

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
        // Decide where dictation lands (applied as segments arrive): a selection
        // is replaced; a focused caret inserts in place; otherwise append at the
        // end (move the caret there and scroll to it).
        const editor = editorRef.current;
        if (editor && activeId && selectionText(editor).trim()) {
          dictationMode.current = "replace";
        } else if (editor && activeId && editor.isFocused) {
          dictationMode.current = "cursor";
        } else {
          dictationMode.current = "cursor";
          if (editor && activeId) editor.chain().focus("end").scrollIntoView().run();
        }
        let language = activeNote?.languageMode;
        if (!activeId) {
          const note = await api.createNote("", "dictation");
          setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)], sortRef.current));
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

  // Load the read-aloud voices once; if nothing is stored (or the stored voice
  // is gone), fall back to the first available voice.
  useEffect(() => {
    void api
      .listTtsVoices()
      .then((vs) => {
        setVoices(vs);
        // Keep a valid stored voice; otherwise default to XTTS (fast on GPU,
        // fluent streaming) and fall back to the first voice.
        const stored = localStorage.getItem("tts-voice") ?? "";
        const active = vs.some((v) => v.id === stored)
          ? stored
          : (vs.find((v) => v.provider === "xtts") ?? vs[0])?.id ?? "";
        setVoiceId(active);
        // Warm only the active backend's sidecar — never both at launch.
        const provider = vs.find((v) => v.id === active)?.provider;
        if (provider) void api.warmTts(provider);
      })
      .catch(() => setVoices([]));
  }, []);

  // Persist the read-aloud tuning knobs.
  useEffect(() => {
    localStorage.setItem("tts-tuning", JSON.stringify(tuning));
  }, [tuning]);

  // Live model-download progress for the manager.
  useEffect(() => {
    const unlisten = listen<ModelProgress>("model_progress", (e) => {
      setModelProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
    });
    return () => void unlisten.then((f) => f());
  }, []);

  // Open the model manager: load the catalog + active-provider summaries.
  const openModels = useCallback(() => {
    setShowModels(true);
    void api.listCatalog().then(setCatalog).catch(() => setCatalog([]));
    void api.listModelInfo().then(setModels).catch(() => setModels([]));
  }, []);

  // Install a catalog entry (license-gate restrictive ones first), then refresh.
  const handleInstallModel = useCallback(async (item: CatalogItem) => {
    if (
      !item.commercialOk &&
      !window.confirm(
        translate(getLang(), "models.confirmNonCommercial", {
          name: item.displayName,
          license: item.license,
        }),
      )
    ) {
      return;
    }
    setModelBusy(item.id);
    try {
      await api.installModel(item.id);
      setCatalog(await api.listCatalog());
    } catch (err) {
      console.error("install failed:", err);
      window.alert(translate(getLang(), "models.installFailed", { error: String(err) }));
    } finally {
      setModelBusy(null);
      setModelProgress((p) => {
        const rest = { ...p };
        delete rest[item.id];
        return rest;
      });
    }
  }, []);

  const handleDeleteModel = useCallback(async (item: CatalogItem) => {
    if (!window.confirm(translate(getLang(), "models.confirmDelete", { name: item.displayName })))
      return;
    try {
      await api.deleteModel(item.id);
      setCatalog(await api.listCatalog());
    } catch (err) {
      console.error("delete failed:", err);
    }
  }, []);

  // Subscribe once to the backend's live dictation events: insert each
  // transcript chunk at the cursor, drive the level meter, surface errors, and
  // track recording state. Stop capture if the app unmounts mid-dictation.
  useEffect(() => {
    const unsub = subscribeDictation({
      onSegment: (text) => {
        const editor = editorRef.current;
        if (!editor) return;
        editor.commands.clearDictationGhost();
        const u = utterance.current;
        if (u) {
          // Overwrite the live-committed region with the authoritative final
          // transcript (one undoable step), then continue after it.
          replaceRange(editor, u.from, u.committed.length, u.prefix + text);
          utterance.current = null;
        } else if (dictationMode.current === "replace") {
          replaceSelection(editor, text);
          dictationMode.current = "cursor"; // further segments append at the caret
        } else {
          insertAtCursor(editor, text);
        }
        editor.commands.scrollIntoView();
      },
      onPartial: (partial) => {
        const editor = editorRef.current;
        if (!editor) return;
        let u = utterance.current;
        if (!u) {
          // Don't anchor an utterance until there's a stabilized word to commit.
          if (!partial.stable) {
            editor.commands.setDictationGhost("", partial.tail);
            return;
          }
          if (dictationMode.current === "replace") {
            const { from, to } = editor.state.selection;
            replaceRange(editor, from, to - from, partial.stable);
            u = { from, prefix: "", committed: partial.stable };
            dictationMode.current = "cursor";
          } else {
            const from = editor.state.selection.to;
            const prefix = separatorBefore(editor, from);
            replaceRange(editor, from, 0, prefix + partial.stable);
            u = { from, prefix, committed: prefix + partial.stable };
          }
          utterance.current = u;
        } else {
          // Grow / revise the committed prefix in place.
          const next = u.prefix + partial.stable;
          if (next !== u.committed) {
            replaceRange(editor, u.from, u.committed.length, next);
            u.committed = next;
          }
        }
        // Only the still-tentative tail stays as ghost, after the committed text.
        editor.commands.setDictationGhost("", partial.tail);
      },
      onLevel: setMicLevel,
      onError: setDictationError,
      onStarted: () => {
        utterance.current = null;
        setDictating(true);
        setDictationError(null);
      },
      onStopped: () => {
        utterance.current = null;
        setDictating(false);
        setMicLevel(0);
        editorRef.current?.commands.clearDictationGhost();
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

  // Region OCR (Ctrl+Alt+O snipping tool): the overlay window forwards its
  // cropped image + recognized layout here; open the selectable OCR overlay.
  useEffect(() => {
    const result = listen<RegionOcr>("region-ocr-result", ({ payload }) => {
      setOcr((cur) => {
        if (cur) URL.revokeObjectURL(cur.url);
        return { url: payload.dataUrl, layout: payload.layout };
      });
    });
    const error = listen<string>("region-ocr-error", ({ payload }) =>
      setDictationError(translate(getLang(), "ocr.regionFailed", { error: payload })),
    );
    return () => {
      void result.then((fn) => fn());
      void error.then((fn) => fn());
    };
  }, []);

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
      setDictationError(translate(getLang(), "ocr.failed", { error: String(err) }));
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
        void api.snapshotNoteVersion({
          noteId: activeId,
          contentMarkdown: editorMarkdown(editor),
          source: "op",
          op: "ocr",
        });
      } else {
        const note = await api.createNote(trimmed, "ocr");
        setNotes((prev) => sortNotes([note, ...prev.filter((n) => n.id !== note.id)], sortRef.current));
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
      <div className="body">
        <Sidebar
          notes={notes}
          scope={scope}
          onScopeChange={changeScope}
          sort={sort}
          onSortChange={setSort}
          activeId={activeId}
          query={query}
          onQueryChange={setQuery}
          onSelect={handleSelect}
          onNewNote={() => void newNote()}
          selected={selected}
          onClearSelection={() => setSelected(new Set())}
          onPin={(n) => void handlePin(n)}
          onRename={handleRename}
          onDuplicate={(n) => void handleDuplicate(n)}
          onArchive={(n) => void handleArchive(n)}
          onExport={handleExportNote}
          onTrash={(n) => void handleTrash(n)}
          onRestore={(n) => void handleRestore(n)}
          onDeleteForever={(n) => void handleDeleteForever(n)}
          onBulkPin={() => void handleBulkPin()}
          onBulkArchive={() => void handleBulkArchive()}
          onBulkExport={() => void handleBulkExport()}
          onBulkTrash={() => void handleBulkTrash()}
          onEmptyTrash={() => void handleEmptyTrash()}
        />
        <main className="editor-pane">
          <ActionBar
            onDictate={() => void toggleDictation()}
            onOcr={triggerOcr}
            onFormat={() => void formatActive()}
            onRead={() => void readActive()}
            onExport={() => activeId && void flushSave().then(() => api.exportNote(activeId))}
            onHistory={openHistory}
            onDelete={() => void deleteActive()}
            dictating={dictating}
            formatting={formatting}
            reading={reading}
            hasNote={!!activeNote}
            theme={theme}
            onToggleTheme={toggleTheme}
            onShowModels={openModels}
            lang={lang}
            onToggleLang={() => setLang(lang === "de" ? "en" : "de")}
          />
          {activeNote ? (
            <>
              {!dictating && sources.length > 0 && (
                <div className="dictation-source">
                  <span className="dictation-source__label">{t("dictation.source")}</span>
                  <select
                    className="dictation-source__select"
                    value={source ? String(sources.indexOf(source)) : ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      setSource(v === "" ? null : sources[Number(v)] ?? null);
                    }}
                  >
                    <option value="">{t("dictation.defaultMic")}</option>
                    {sources.map((s, i) => (
                      <option key={`${s.loopback ? "out" : "in"}:${s.name}`} value={String(i)}>
                        {(s.loopback ? "🔊 " : "🎙 ") + s.name}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              {!dictating &&
                voices.length > 0 &&
                (() => {
                  const backends = [...new Set(voices.map((v) => v.provider))];
                  const backend = voices.find((v) => v.id === voiceId)?.provider ?? backends[0] ?? "";
                  const backendVoices = voices.filter((v) => v.provider === backend);
                  return (
                    <div className="dictation-source">
                      <span className="dictation-source__label">{t("readaloud.label")}</span>
                      {backends.length > 1 && (
                        <select
                          className="dictation-source__select dictation-source__select--backend"
                          value={backend}
                          aria-label={t("readaloud.backend.aria")}
                          title={t("readaloud.backend.title")}
                          onChange={(e) => {
                            const next = voices.find((v) => v.provider === e.target.value);
                            if (next) {
                              setVoiceId(next.id);
                              localStorage.setItem("tts-voice", next.id);
                              void api.warmTts(e.target.value); // warm the chosen backend
                            }
                          }}
                        >
                          {backends.map((p) => (
                            <option key={p} value={p}>
                              {backendLabel(p)}
                            </option>
                          ))}
                        </select>
                      )}
                      <select
                        className="dictation-source__select"
                        value={voiceId}
                        aria-label={t("readaloud.voice.aria")}
                        onChange={(e) => {
                          const v = e.target.value;
                          setVoiceId(v);
                          localStorage.setItem("tts-voice", v);
                        }}
                      >
                        {backendVoices.map((v) => (
                          <option key={v.id} value={v.id}>
                            {`🗣 ${v.displayName}`}
                          </option>
                        ))}
                      </select>
                      {backend === "zonos" && (
                        <select
                          className="dictation-source__select dictation-source__select--emotion"
                          value={tuning.emotion ?? "neutral"}
                          aria-label={t("readaloud.emotion.aria")}
                          title={t("readaloud.emotion.title")}
                          onChange={(e) =>
                            setTuning((prev) => ({ ...prev, emotion: e.target.value }))
                          }
                        >
                          {ZONOS_EMOTIONS.map((em) => (
                            <option key={em.key} value={em.key}>
                              {`${em.icon} ${t(`emotion.${em.key}` as TranslationKey)}`}
                            </option>
                          ))}
                        </select>
                      )}
                      {lastAudioKey && (
                        <button
                          className="icon-btn"
                          onClick={exportAudio}
                          aria-label={t("readaloud.exportAudio.aria")}
                          title={t("readaloud.exportAudio.title")}
                        >
                          ⬇
                        </button>
                      )}
                      <button
                        className="icon-btn"
                        onClick={() => setShowVoiceSettings(true)}
                        aria-label={t("readaloud.settings.aria")}
                        title={t("readaloud.settings.aria")}
                      >
                        ⚙
                      </button>
                    </div>
                  );
                })()}
              {dictating && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">{t("dictation.recording")}</span>
                  <span className="dictation-bar__meter">
                    <span
                      className="dictation-bar__level"
                      style={{ width: `${Math.round(Math.min(1, micLevel) * 100)}%` }}
                    />
                  </span>
                </div>
              )}
              {formatProgress && !reading && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">
                    {t("format.progress", {
                      done: formatProgress.done,
                      total: formatProgress.total,
                    })}
                  </span>
                </div>
              )}
              {reading && !speaking && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">
                    {synthProgress
                      ? t("readaloud.prepAudio", {
                          done: synthProgress.done,
                          total: synthProgress.total,
                        })
                      : formatProgress
                        ? t("readaloud.prepText", {
                            done: formatProgress.done,
                            total: formatProgress.total,
                          })
                        : t("readaloud.preparing")}
                  </span>
                  <button className="tts-reset" onClick={() => void readActive()}>
                    {t("readaloud.cancel")}
                  </button>
                </div>
              )}
              {reading && speaking && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">
                    {paused ? t("readaloud.paused") : t("readaloud.reading")}
                    {formatProgress
                      ? t("readaloud.prepSuffix", {
                          done: formatProgress.done,
                          total: formatProgress.total,
                        })
                      : ""}
                  </span>
                  <button className="tts-reset" onClick={togglePauseRead}>
                    {paused ? t("readaloud.resume") : t("readaloud.pause")}
                  </button>
                  <button className="tts-reset" onClick={() => void readActive()}>
                    {t("readaloud.stop")}
                  </button>
                </div>
              )}
              {voiceLoading && !reading && (
                <div className="dictation-bar" role="status" aria-live="polite">
                  <span className="dictation-bar__dot" />
                  <span className="dictation-bar__label">{t("readaloud.voiceLoading")}</span>
                </div>
              )}
              <div className="editor-scroll">
                <input
                  ref={titleInputRef}
                  className="editor-title"
                  value={activeNote.title}
                  placeholder={t("editor.titlePlaceholder")}
                  onChange={(e) => patchActive({ title: e.target.value })}
                />
                <div className="editor-meta">{noteMeta(activeNote, t)}</div>
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
              <div className="empty-state__title">{t("empty.title")}</div>
              <div>{t("empty.desc")}</div>
              <button
                className="btn-primary"
                style={{ width: "auto", padding: "8px 16px" }}
                onClick={() => void newNote()}
              >
                <PlusIcon size={14} />
                {t("sidebar.newNote")}
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
      {ocrBusy && <div className="ocr-busy">{t("ocr.busy")}</div>}
      {ocr && (
        <OcrOverlay
          key={ocr.url}
          imageUrl={ocr.url}
          layout={ocr.layout}
          onInsert={(text) => void insertOcrText(text)}
          onClose={closeOcr}
        />
      )}
      {history && (
        <HistoryOverlay
          versions={history}
          currentContent={activeNote?.contentMarkdown ?? ""}
          noteTitle={activeNote?.title ?? ""}
          onRestore={(versionId) => void restoreVersion(versionId)}
          onClose={() => setHistory(null)}
        />
      )}
      {showModels && (
        <ModelManager
          items={catalog}
          providers={models ?? []}
          progress={modelProgress}
          busyId={modelBusy}
          onInstall={handleInstallModel}
          onDelete={handleDeleteModel}
          onClose={() => setShowModels(false)}
        />
      )}
      {showVoiceSettings &&
        (() => {
          const activeProvider = voices.find((v) => v.id === voiceId)?.provider ?? "piper";
          return (
            <ReadAloudSettings
              backendLabel={backendLabel(activeProvider)}
              isPiper={activeProvider === "piper"}
              isZonos={activeProvider === "zonos"}
              tuning={tuning}
              onChange={setTuning}
              speechPrep={speechPrep}
              onSpeechPrepChange={(v) => {
                setSpeechPrep(v);
                localStorage.setItem("tts-speech-prep", v ? "1" : "0");
              }}
              onPreview={() => void previewVoice()}
              previewing={previewing}
              onClose={() => setShowVoiceSettings(false)}
            />
          );
        })()}
      {preview && (
        <div className="history-backdrop" onClick={() => setPreview(null)}>
          <div
            className="preview-panel"
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-label={t("preview.aria")}
          >
            <header className="history-panel__head">
              <span>{t("preview.title")}</span>
              <button
                className="icon-btn"
                onClick={() => setPreview(null)}
                aria-label={t("common.close")}
              >
                ×
              </button>
            </header>
            <div className="preview-cols">
              <div className="preview-col">
                <span className="preview-col__label">{t("preview.original")}</span>
                <pre className="preview-col__text">{preview.original}</pre>
              </div>
              <div className="preview-col">
                <span className="preview-col__label">{t("preview.formatted")}</span>
                <pre className="preview-col__text">{preview.formatted}</pre>
              </div>
            </div>
            <footer className="preview-actions">
              <button className="action-btn" onClick={() => setPreview(null)}>
                {t("common.cancel")}
              </button>
              <button
                className="action-btn action-btn--primary"
                onClick={async () => {
                  const apply = preview.onApply;
                  setPreview(null);
                  await apply();
                }}
              >
                {t("common.apply")}
              </button>
            </footer>
          </div>
        </div>
      )}
      <ToastStack toasts={toasts.toasts} onAction={toasts.runAction} onDismiss={toasts.dismiss} />
    </div>
  );
}
