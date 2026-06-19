# ExoQuill — v0.1 Implementation Roadmap

This roadmap supersedes `PLAN.md` §26. It folds in the decisions from `docs/decisions.md`,
which **shrink the MVP**: no model manager (D5), no formatting preview (D6), GPL-3.0 so no
license-driven sidecar gymnastics (D1/D8), TipTap editor (D3), bundled models (D5).

Each PR is independently shippable, has explicit acceptance criteria, and ends with a commit.
PRs are ordered by dependency and by risk (the riskiest real-AI work comes after the
architecture is proven with mocks).

## Overview

| PR  | Title                                   | Risk    | Depends on |
|-----|-----------------------------------------|---------|------------|
| 0   | Foundation & Repo Setup                 | low     | —          |
| 1   | App Skeleton + Notes Core               | low     | 0          |
| 2   | Provider Interfaces + Job Queue (mocks) | medium  | 1          |
| 3   | OCR v0.1                                | medium  | 2          |
| 4   | Formatting v0.1                         | medium  | 2          |
| 5   | Dictation v0.1                          | **high**| 2          |
| 6   | Read Aloud v0.1                         | medium  | 2          |
| 7   | Tray + Global Shortcuts                 | low     | 1, 5       |

Key change vs PLAN.md: a new **PR 0** is added, and the model-manager work is **removed**
(replaced by "load bundled models", a small part of PR 2 and each AI PR).

---

## PR 0 — Foundation & Repo Setup *(new)*

**Goal.** A buildable, CI-gated empty monorepo so every later PR lands on green checks.

**Scope.**
- pnpm workspace + repo layout per PLAN.md §20 (`apps/desktop`, `crates/*`, `docs/`, `.workspace/`).
- Tauri v2 app that opens an empty window (React + TypeScript frontend, Rust core).
- Rust workspace with the `exoquill-*` crates stubbed out.
- CI (GitHub Actions): `lint`, `build-test (windows-x64-debug)`, `build-test (windows-x64-release)`
  — the same check names the branch ruleset will require.
- **GPL-3.0 `LICENSE`** (D1), `README.md` skeleton, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`.
- Move `PLAN.md` → `.workspace/specs/exoquill-product-spec.md` (keep `docs/decisions.md` + this file at top level of `docs/`).

**Acceptance.**
- `pnpm install` + Tauri dev build runs and shows a window on Windows.
- CI passes all three checks on a PR.
- After merge: attach the three CI checks as **required status checks** to the `main` branch ruleset
  (the part we deliberately left off at repo creation).

## PR 1 — App Skeleton + Notes Core

**Goal.** Local notes, no AI.

**Scope.**
- SQLite setup + migrations; `notes`, `settings`, `note_events` tables (PLAN.md §16).
- Notes sidebar sorted by `updated_at DESC`; create / rename / edit / delete.
- **TipTap** editor (D3) + `tiptap-markdown`; Markdown is the stored format.
- Auto-create-note resolver (insert with no active note creates one).
- IPC commands: `createNote`, `updateNote`, `deleteNote`, `listNotes`, `searchNotes` (PLAN.md §17.1).

**Out of scope.** All AI (OCR/STT/TTS/formatter).

**Acceptance.** Create/edit/delete works; notes persist across restart; sort by last modified;
resolver can create a note programmatically; Markdown round-trips through the editor (tested on
headings, lists, code fences). Committed.

## PR 2 — Provider Interfaces + Job Queue (mocks)

**Goal.** The AI architecture, proven with mocks and no real models.

**Scope.**
- Provider traits: `SpeechToTextProvider`, `OcrProvider`, `FormatterProvider`,
  `TextToSpeechProvider`, `VadProvider` (PLAN.md §14.1).
- Job queue + `jobs` table + event bus (PLAN.md §16, §17.2, §17.3): async, cancellable,
  progress, single heavy job by default, UI never blocks.
- **Process-isolation harness (D8):** a reusable way to run a provider as a child process with
  health-check / cancel / restart. Mock providers exercise it.
- **Bundled-model loading (replaces the model manager, D5):** resolve model files from bundled
  app resources; surface model + voice **license/size** info read-only in settings.
- Mock STT/OCR/formatter/TTS providers; UI job status + error surfacing.

**Acceptance.** Mock jobs run async without blocking UI; jobs are cancellable and observable;
failed jobs show errors; provider crash → restart works; bundled-model resolver finds resources.
Committed.

## PR 3 — OCR v0.1

**Goal.** Real image-to-text.

**Scope.**
- Sources: file picker, drag & drop, clipboard paste (**no** screen region — D4).
- Tesseract provider (bundled `deu+eng` traineddata), basic auto preprocessing.
- Insert into active note / auto-create note; `ocr_*` note events; result toast `[Format] [Undo]`.

**Acceptance.** OCR from file and from clipboard works; result inserts into note; no active note
creates one; fixture-image tests pass. Committed.

## PR 4 — Formatting v0.1

**Goal.** Local text cleanup.

**Scope.**
- Quick Format + custom instruction, for **selection and whole note**, all via
  **replace + undo** (D6) — no preview/diff.
- Formatter provider over a llama.cpp-compatible interface; bundled **small** Qwen3 model (D5).
- Structured prompt contract (PLAN.md §12.5); custom terms; undo toast.
- Store original in `note_events.raw_text` as the safety net (D6).

**Acceptance.** Selection and whole note can be formatted and undone atomically; technical terms
preserved; prompt-construction + mock-provider snapshot tests pass. Committed.

## PR 5 — Dictation v0.1 *(highest risk)*

**Goal.** Minimal local dictation. This is the hardest PR — real-time audio + VAD + Whisper.

**Scope.**
- Mic selection, auto-gain toggle, manual gain slider, input level meter, start/stop.
- Audio pipeline per PLAN.md §10.5–10.6: capture → gain → ring buffer → Silero VAD → chunk →
  Whisper → transcript. Bundled Whisper + VAD models (D5), Whisper as isolated process (D8).
- German + English-terms default; append to active note / auto-create; no raw audio retention;
  dictation note events.

**Acceptance.** User can dictate into the active note; no active note creates one; UI stays
responsive; level meter works; no unbounded memory growth in a long session. Tests where feasible.
Committed.

**Note.** Budget extra time here. Recommend a spike on the whisper.cpp + Silero VAD process
integration before committing to the full UI.

## PR 6 — Read Aloud v0.1

**Goal.** Local TTS playback.

**Scope.**
- Read selection / whole note / from cursor; voice selector; speed; play/pause/stop.
- Paragraph→sentence splitter feeding a TTS queue (PLAN.md §13.5); playback begins after the
  first generated segment.
- **Piper** provider, bundled, isolated process (D1/D2/D8); behind the `TextToSpeechProvider`
  interface so system-TTS/Kokoro can be added later.
- Markdown read behavior per PLAN.md §13.8 (skip code fences, etc.).

**Acceptance.** Selection and whole note read aloud; pause/stop immediate; long notes segmented;
text-segmentation + queue tests pass. Committed.

## PR 7 — Tray + Global Shortcuts

**Goal.** Make ExoQuill useful in the background.

**Scope.**
- System tray; configurable global shortcuts (PLAN.md §15.9): toggle dictation, quick note,
  (OCR-region shortcut is a placeholder until v0.2); autostart option; shortcut settings UI.

**Acceptance.** App runs in tray; dictation triggerable globally; shortcuts configurable. Committed.

---

## v0.1 Definition of Done

Per PLAN.md §29, adjusted: notes persist; dictation, file OCR, clipboard OCR, quick + custom
selection formatting, whole-note formatting (replace+undo, no preview), read selection, read
whole note all work; no cloud account; no telemetry; no raw audio by default; bundled-model
licenses visible; README explains setup; tests cover notes, jobs, formatting, OCR input handling,
and TTS segmentation.

## Deferred to v0.2+

Screen-region OCR (D4) · full model manager + downloads (D5) · formatting preview/diff (D6) ·
export Markdown · dictation/event history views · improved VAD · better read-aloud queue.
