# Contributing to ExoQuill

Thanks for your interest. ExoQuill is a local-first, privacy-first desktop app — contributions
must keep those guarantees intact.

## Ground rules

- **Focused PRs.** One concern per pull request.
- **Tests for core changes.** Notes, jobs, formatting, OCR input handling and TTS segmentation
  must stay covered.
- **No telemetry**, ever, by default.
- **No cloud dependency** for core features. Local-first is non-negotiable.
- **Provider additions must document** their license, model size, and download/source — see the
  provider interface and `docs/decisions.md`.
- **Commit completed changes.** Don't leave the tree half-migrated.

## Branch & tag rules

- `main` is protected: no force-push, no deletion, and (once CI exists) required status checks.
- Version tags `v*` are protected: no create/update/delete except by maintainers.
- Work on feature branches and open a PR against `main`.

## Development

See the [README](README.md#development-setup) for prerequisites and setup. The architecture and
PR breakdown are in [`docs/roadmap.md`](docs/roadmap.md); binding decisions in
[`docs/decisions.md`](docs/decisions.md).

## Architecture in one line

Tauri v2: a **Rust** core (notes, jobs, AI providers, data) plus a **TypeScript/React** frontend
in a WebView. Heavy AI runtimes run as **isolated processes** (`docs/decisions.md`, D8).

## Commit style

Conventional, imperative subject lines (e.g. `feat(notes): add auto-create resolver`). Keep
commits scoped and buildable.

## Code of conduct

Be respectful and constructive. Assume good intent.
