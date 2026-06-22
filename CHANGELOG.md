# Changelog

All notable changes to ExoQuill are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-22

### Added

- **OCR** (Tesseract): file picker, drag & drop, clipboard paste, a selectable
  result overlay, and desktop region capture (Ctrl+Alt+O snipping tool).
- **Formatting**: deterministic cleanup with a before/after preview (D6) and an
  optional local-LLM "prepare for speech" pass.
- **Dictation**: cpal capture + VAD with live, word-by-word streaming through a
  persistent whisper-server (ghost text, multiple insertion modes).
- **Read-aloud**: bundled Piper TTS with a streaming sentence queue + prefetch;
  optional multilingual sidecars **XTTS-v2** and **Zonos-v0.1**, backend-selectable,
  with per-voice tuning and WAV export (D2/D10).
- **Model manager**: a three-tier catalog (bundled / download / gated) with
  install / delete and license info (D9); read-only on-device provider info (D5).
- **Notes management** (D12): scopes (Active / Archived / Trash), a pinned group,
  soft-delete with undo toasts, multi-select bulk actions, and sort.
- **Edit history** (D12): content-hash-deduped snapshots (`note_versions`) and a
  diff-timeline overlay with operation badges and non-destructive version restore.
- Bilingual **DE/EN** UI via `lib/i18n.ts`, and the Direction B "Local AI Utility"
  skin in light + dark.
- TTS backend roadmap, incl. Chatterbox Multilingual as the MIT high-quality slot (D11).

### Changed

- Consolidated into a single toolbar (the former top bar was removed); branding
  moved into the sidebar.

[Unreleased]: https://github.com/Exoridus/exoquill/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Exoridus/exoquill/releases/tag/v0.2.0
