# ExoQuill — Product & Technical Decisions

This document records the **binding decisions** that resolve the open questions in
`PLAN.md` (Section 28) and the points where we deliberately deviate from the draft
specification for the **v0.1 MVP**. Where this document and `PLAN.md` disagree, this
document wins.

- Status: **accepted**
- Date: 2026-06-19
- Supersedes: relevant parts of `PLAN.md` §14.7, §25.1, §27, §28

## Summary

| #  | Topic                | Decision                                                          |
|----|----------------------|------------------------------------------------------------------|
| D1 | App license          | **GPL-3.0**                                                       |
| D2 | TTS provider         | **Piper**, bundled, behind a provider interface                  |
| D3 | Editor               | **TipTap** + `tiptap-markdown`                                   |
| D4 | Screen-region OCR    | **v0.2** (out of v0.1)                                            |
| D5 | Model delivery       | **All models bundled**, no model manager in v0.1                 |
| D6 | Formatting UX        | **Replace + Undo everywhere**, no preview/diff in v0.1           |
| D7 | External insertion   | **No** — note-internal only                                      |
| D8 | Runtime isolation    | AI runtimes run as **isolated processes** (stability, not license)|

---

## D1 — App license: GPL-3.0

**Context.** The TTS runtime we want (Piper) moved to GPL-3.0: the original MIT repo
`rhasspy/piper` was archived in October 2025; the maintained successor `piper1-gpl`
is GPL-3.0 (Open Home Foundation).

**Decision.** License ExoQuill under **GPL-3.0**.

**Rationale.** GPL-3.0 lets us bundle and integrate `piper1-gpl` directly without any
isolation workaround. The rest of the stack is GPL-3.0-compatible: Tauri (MIT/Apache),
whisper.cpp (MIT), llama.cpp (MIT), Tesseract (Apache-2.0). Strong copyleft also fits an
open, distributed-as-a-whole desktop tool.

**Important clarification.** The app license does **not** govern which model *weights* we
may load — model weights are data loaded at runtime ("mere aggregation") and carry their
own licenses. The app license only matters for bundling/linking runtime *code*.

**Trade-off accepted.** Anyone redistributing or forking ExoQuill must release source;
code parts cannot later be reused in permissive/proprietary projects.

**Deviation from PLAN.md.** §25.1/§27 recommended Apache-2.0 or MIT.

## D2 — TTS provider: Piper, bundled, behind an interface

**Decision.** Ship **Piper** as the default v0.1 TTS provider, bundled with the app, behind
a `TextToSpeechProvider` interface.

**Rationale.** Piper has the only reliable high-quality **German** voice (Thorsten). Kokoro
has no official German (only an experimental community voice) — too risky for a German-first
product. The provider interface keeps the door open for a Windows system-TTS fallback and for
Kokoro later, per PLAN.md §13.6.

**Open item.** Verify the individual license of each bundled voice (`.onnx`) before shipping.

## D3 — Editor: TipTap

**Decision.** Use **TipTap** (MIT core) with the `tiptap-markdown` extension for Markdown I/O.

**Rationale.** TipTap is free for our use case (only Cloud/Collab bundles are paid, which we
don't need), has the largest ecosystem and best docs, and the floating selection toolbar
(PLAN.md §9) is a well-trodden pattern. Markdown remains the storage format.

**Known risk.** TipTap is not Markdown-native internally (ProseMirror JSON); Markdown round-trip
on edge cases (code fences, tables, nested lists) needs test coverage. Milkdown and CodeMirror 6
were the considered alternatives.

## D4 — Screen-region OCR: v0.2

**Decision.** Out of v0.1. v0.1 OCR sources = file picker, drag & drop, clipboard paste only.

**Rationale.** Region-capture overlays are platform-specific and fiddly (multi-monitor, DPI,
Wayland). Keeping them out keeps the OCR PR focused. Matches PLAN.md §6.2.

## D5 — Model delivery: all bundled, no model manager in v0.1

**Decision.** Bundle **all** models as app resources. **No** model manager, download UI, manifest,
or manual path selection in v0.1. The formatter LLM must be a deliberately **small** quantized
model to keep installer size reasonable.

**Rationale.** Eliminates the entire download/manifest subsystem for the MVP. The only real cost
is installer size, driven almost entirely by the LLM (~1–3 GB); everything else combined is < 1 GB.
Choose a small LLM (e.g. Qwen3 ~1.7B Q4 ≈ 1 GB) and accept a ~1.5–2 GB installer/portable build.

**Notes.** GitHub release assets may need splitting for the portable build. The full model manager
(PLAN.md §14.7) moves to v0.2+.

**Deviation from PLAN.md.** §19.2 said "do not bundle large models in the default installer." We
deliberately do, for MVP simplicity.

## D6 — Formatting UX: replace + undo everywhere, no preview

**Decision.** Both selection and whole-note formatting do **direct replace + undo**. No diff/preview
renderer in v0.1.

**Rationale.** PLAN.md principle 6 requires "preview **or** undo"; reliable undo satisfies it and
saves the most complex UI piece. Two guardrails:
1. Whole-note format = a single atomic undo step back to the exact prior state.
2. The original text is stored in `note_events.raw_text` as a safety net.

Preview/diff and a per-scope setting move to v0.2.

**Deviation from PLAN.md.** §12.4/§27 required whole-note preview.

## D7 — External insertion: no

**Decision.** ExoQuill stays note-internal. It will not type into other focused apps in the
foreseeable roadmap.

**Rationale.** UI-automation/accessibility insertion is a large security/permission/platform
burden and conflicts with the note-centric principle. Possible long-term (v1.x) idea only.

## D8 — Runtime isolation (derived)

**Decision.** Run heavy AI runtimes (whisper.cpp, llama.cpp, Piper, Tesseract) as **isolated
processes**, not in-process libraries.

**Rationale.** With GPL-3.0 this is no longer a *license* requirement, but it remains a
**stability** requirement: a crashing inference process must not take down the app
(cf. PLAN.md §24.6 "Provider crashed → Restart provider"). This is an architecture rule for
PR 2 onward.
