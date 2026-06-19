# ExoQuill

> Speak it, capture it, clean it, read it — locally.

**ExoQuill** is a lightweight, local-first desktop app for capturing, cleaning, reading
and organizing text from multiple sources. It centers on a single global note list, and
every AI tool writes into notes:

- 🎙️ **Dictate** — live local speech-to-text
- 🖼️ **OCR** — text from files, drag & drop, and clipboard images
- ✨ **Format** — AI-assisted cleanup of rough text into readable Markdown
- 🔊 **Read** — text-to-speech read-aloud with a calm local voice

Optimized for **German and English**, runs **offline by default**, deliberately minimal.

## What ExoQuill is *not*

Not a chatbot, not an Obsidian/OneNote clone, not a cloud AI wrapper, not an audio editor,
not a transcription suite, not a translation tool. It is a focused local writing/capture utility.

## Privacy

- No cloud calls, no telemetry, no automatic uploads.
- Raw audio is **not** stored by default.
- All models run locally; you can delete all local data at any time.

## Supported languages

Official support is limited to **German** and **English**, including mixed German speech
with English technical terms as a first-class use case.

## Status

Pre-alpha — building toward **v0.1**. See the [roadmap](docs/roadmap.md).

## Installation

Windows installer / portable build — _coming with the first v0.1 release._

## Models

ExoQuill v0.1 **bundles** all required local models (STT, OCR, formatter, TTS). There is no
separate download step for the MVP. See [decisions](docs/decisions.md) (D5).

## Development setup

Prerequisites:

- [Rust](https://rustup.rs/) (stable, `x86_64-pc-windows-msvc`) + MSVC C++ build tools
- [Node.js](https://nodejs.org/) 20+ and [pnpm](https://pnpm.io/)
- WebView2 (preinstalled on Windows 11)

```bash
pnpm install
pnpm tauri dev   # once the Tauri app skeleton lands (PR 0/1)
```

## Roadmap

The full plan lives in [`docs/roadmap.md`](docs/roadmap.md); product/technical decisions in
[`docs/decisions.md`](docs/decisions.md); the original product spec in
[`.workspace/specs/exoquill-product-spec.md`](.workspace/specs/exoquill-product-spec.md).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). In short: focused PRs, tests for core changes,
no telemetry, no cloud dependency for core features.

## License

[GPL-3.0](LICENSE). ExoQuill bundles GPL components (e.g. the Piper TTS runtime); the project
license is GPL-3.0 to stay compatible. See [decisions](docs/decisions.md) (D1).
