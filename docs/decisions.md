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
| D5 | Model delivery       | All models bundled, no model manager in v0.1 — **revised by D9** |
| D6 | Formatting UX        | **Replace + Undo everywhere**, no preview/diff in v0.1           |
| D7 | External insertion   | **No** — note-internal only                                      |
| D8 | Runtime isolation    | AI runtimes run as **isolated processes** (stability, not license)|
| D9 | Model acquisition    | **3-tier** (bundle / download / gated) + in-app **model manager**|
| D10| Multilingual TTS      | **XTTS** + **Zonos** as opt-in sidecars, **backend-selectable** in the UI |
| D11| TTS backend roadmap   | Ranked backend lineup; **Chatterbox Multilingual** adopted as the MIT high-quality slot |
| D12| Notes management & history | Scopes (active/archived/trash), pin group, soft-delete + undo toasts, multi-select, diff history |

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

## D9 — Model acquisition: three tiers + in-app model manager

**Revises D5.** D5 ("all models bundled, no manager") doesn't survive contact with the real
license landscape (below) or with heavy/optional models like XTTS-v2. We separate the **app**
(GPL-3.0, bundled) from the **models** (each with its own license), and acquire each model by
its license class.

**Decision.** Every model/voice is one of three tiers, surfaced in an in-app **model manager**
(install / use / delete per entry, with a license gate for restrictive ones):

- **bundled** — ships in the installer. Only assets that are redistributable **and**
  GPL-3.0-compatible **and** commercial-OK.
- **download** — free + clean, but fetched on demand (keeps the installer slim). Same license
  freedoms as bundled; the only difference is delivery.
- **gated** — restrictive license (e.g. non-commercial). Never bundled; on-demand download
  only, behind an explicit license-acceptance step; hidden entirely in a commercial build via
  a build flag.

**Mechanism.** A shipped manifest (`apps/desktop/src-tauri/models.json`) is the single source
of truth: per entry `id, provider, kind, language, license, commercialOk, tier, files[] (url +
relPath), setup?, notes?`. Backend commands `list_catalog` / `install_model` (streaming
download to a writable models root, `model_progress` events, `.part`→rename) / `delete_model`.
The app-level `license` rule (D1) still governs only runtime **code**; model **weights** are
data with their own terms (the manager enforces them).

**Verified license matrix** (read from canonical LICENSE files / HF MODEL_CARDs, 2026-06-20):

| Asset | License | Commercial | Redistribute | GPL-3.0 bundle | Tier |
|---|---|---|---|---|---|
| Piper runtime (`piper1-gpl`) | GPL-3.0 | yes | yes | yes | **bundle** |
| Voice de_DE-thorsten-high | CC0 | yes | yes | yes | **bundle** |
| Voice en_GB-cori-high | Public Domain (LibriVox) | yes | yes | yes | **bundle** (currently download) |
| Voice en_US-lessac-high | CSTR Blizzard 2013 (research) | **no** | **no** | **no** | **EXCLUDE** |
| Voice en_US-ryan-high | CC BY-NC-SA 4.0 | **no** | yes (NC+SA) | **no** | **gated** |
| whisper.cpp runtime | MIT | yes | yes | yes | **bundle** |
| Whisper ggml-large-v3-turbo | MIT | yes | yes | yes | **bundle** (or download, ~1.6 GB) |
| llama.cpp runtime | MIT | yes | yes | yes | **bundle** |
| Qwen2.5-**1.5B**-Instruct | Apache-2.0 | yes | yes | yes | **bundle** (1.5B only!) |
| Tesseract engine | Apache-2.0 | yes | yes | yes | **bundle** |
| tessdata deu/eng (best/fast) | Apache-2.0 | yes | yes | yes | **bundle** |
| Silero VAD | MIT | yes | yes | yes | **bundle** (verify weights provenance) |
| ONNX Runtime | MIT | yes | yes | yes | **bundle** |
| coqui-tts library (idiap fork) | MPL-2.0 | yes | yes | yes | **bundle** (code only) |
| XTTS-v2 **weights** | CPML 1.0 | **no** | restricted | **no** | **gated** (never bundle) |
| Zonos-v0.1 **weights** | Apache-2.0 | yes | yes | yes | **download** (needs CUDA GPU) |

**Consequences / open items.**
- **Three blockers, all model/data side:** `lessac` (research-only, **exclude entirely** — not
  even download), `ryan` (NC → gated only), XTTS-v2 weights (CPML NC → gated only).
- **The en_US bundle slot needs a clean voice:** both vetted en_US candidates are unusable;
  pick a CC0/Public-Domain/permissive en_US Piper voice before shipping a default English voice.
- **Qwen size trap:** 1.5B is Apache-2.0 (clean); 3B/72B use the restrictive Qwen License — do
  not swap up without re-checking.
- **Attribution:** ship a `THIRD-PARTY-LICENSES` file (MIT notices, Apache-2.0 LICENSE+NOTICE
  for Tesseract/tessdata/Qwen, MPL-2.0 source availability for any modified files).
- **Provenance check:** confirm the Silero `.onnx` came from the official MIT repo, not an old
  CC-BY-NC snapshot.
- XTTS install is special (Python + PyTorch + ~1.75 GB weights, all 58 speakers in one 7.4 MB
  embeddings file) — modeled as a `gated` runtime entry installed via `scripts/setup-xtts.ps1`,
  not a plain file download. A packaged self-contained sidecar is a later improvement.

*Informational, not legal advice — confirm the lessac/ryan exclusions and the CPML gate with
counsel before a commercial release.*

## D10 — Multilingual TTS backends: XTTS + Zonos, backend-selectable

- Status: **accepted** · Date: 2026-06-21 · Extends D2

**Context.** Piper (D2) is the bundled default — the only reliable high-quality German
voice — but it's single-language (espeak phonemes) and weak on mixed DE/EN technical
terms, which are common in these notes. We want optional **multilingual neural** voices
without giving up the on-device principle, and the UI now exposes a **backend picker**
(Piper / XTTS / Zonos) plus voice + speed in the toolbar; the rest of the knobs stay in
the settings overlay.

**Decision.** Offer multilingual TTS as **opt-in local sidecars** behind the existing
`TextToSpeechProvider` interface, each auto-spawned and warmed in the background, each
listing its voices into one merged picker (every voice carries its `provider`, which the
`tts_speak` command routes on). Two are wired:

- **XTTS-v2** — `gated` (CPML, non-commercial weights). All ~58 studio speakers; one voice
  per speaker, language auto-detected per segment (no de/en duplicates). Test-only.
- **Zonos-v0.1** — `download` (**Apache-2.0** weights → commercial-OK, the key advantage
  over XTTS). Voices are **cloned** from reference `.wav` clips in a folder (like Piper
  enumerates a voice folder); language auto-detected per segment. Needs a **CUDA GPU**
  (CPU is unusably slow); depends on eSpeak NG (bundled via `espeakng-loader`); output is
  44.1 kHz. Installed via `scripts/setup-zonos.ps1` + `scripts/zonos-server.py`.

**Candidates evaluated (for "try a better TTS than XTTS").**

| Engine | Weights license | Local? | German | Verdict |
|---|---|---|---|---|
| **Zonos-v0.1** | **Apache-2.0** | yes (GPU) | yes | **Adopted** — only option clean for a commercial GPL build |
| Cartesia Sonic | proprietary, no open weights | cloud API only (on-prem = enterprise) | yes | **Rejected** — breaks the on-device/offline principle |
| Fish Speech / OpenAudio S1 | CC-BY-NC-SA (S2 reportedly MIT, in flux) | yes (GPU) | yes | **Deferred** — same NC problem as XTTS; test-only at best |

**Rationale.** Zonos is the only one of the three whose weights are permissive enough to
ship in a commercial GPL-3.0 build, so it's the strategic successor to XTTS for the
multilingual slot. Cartesia is cloud-only (privacy + offline regression). Fish Speech is
local and good but non-commercial, i.e. no better than XTTS on the blocking constraint.

**Open items.**
- Zonos Python sidecar is **unverified end-to-end** (no CUDA/Zonos test env at integration
  time) — confirm `make_cond_dict`/`generate`/`autoencoder.decode` shapes and the 44.1 kHz
  rate on first real run.
- A packaged self-contained sidecar (no manual venv) is a later improvement, as for XTTS.
- Bundled reference voices: ship a clean, license-clear default clip if Zonos becomes a
  first-class (non-experimental) backend.

## D11 — TTS backend roadmap: ranked lineup + Chatterbox Multilingual

- Status: **accepted** · Date: 2026-06-22 · Extends D2 / D10

**Context.** D2 set Piper as the bundled German default; D10 added XTTS + Zonos as opt-in
multilingual sidecars behind `TextToSpeechProvider`. The neural-TTS field moved fast in early
2026 (Chatterbox Multilingual, Qwen3-TTS, ZONOS2, CosyVoice 3), and several new options are
**permissively licensed** — which matters because a commercial GPL-3.0 build can only *bundle*
weights that are redistributable **and** commercial-OK (D9). This decision records the target
lineup and ranks each backend by role, so future TTS work has one ordered list to pull from.

**Decision.** Adopt the ranked lineup below. The architecture is unchanged: every backend is a
`TextToSpeechProvider`, heavy neural models run as an auto-spawned local sidecar (Python HTTP
server + thin blocking Rust client, like `xtts`/`zonos`), and each lists its voices into the one
merged, backend-routed picker. New backends are added incrementally — **Chatterbox Multilingual
is the next one to wire** (the MIT high-quality slot, the cleanest successor to XTTS/Zonos).

| Rank | Role | Model(s) | License (verified) | Tier | Why / status |
|---|---|---|---|---|---|
| 1 | **Safe default** | **Piper + good German voice** (Thorsten) | GPL-3.0 runtime / CC0 voice | **bundle** | Fast, robust, local, minimal fuss; the proven D2 default. **Wired.** |
| 2 | **Modern fast default / Fast Mode** | **Kokoro-82M** | Apache-2.0 | download | Tiny, permissive, more natural than classic mini-TTS. **Blocker:** no *official* German (community voice only) — must pass a German quality bar before it can be a default (see D2). |
| 3 | **Best optional high-quality backend** | **Chatterbox Multilingual (v3)** | **MIT** | download (GPU) | 23+ languages incl. German, voice cloning, emotion, realistic product integration. MIT → first multilingual option that is *redistributable + commercial-OK*. **Caveat:** embeds a Resemble "Perth" neural watermark in every output by default, no documented opt-out — disclose this for an offline/privacy tool. **Adopted, next to wire.** |
| 4 | **Power backend for GPU users** | **Qwen3-TTS** (0.6B / 1.7B) | Apache-2.0 (open weights, not API-only) | download (GPU) | German among 10 languages, streaming, voice design. Open weights on HF (~2.5 / 4.5 GB); Windows-native path exists. |
| 5 | **Experimental premium GPU backend** | **ZONOS2** | Apache-2.0 (HF model card; "MIT" in some write-ups is wrong) | download (GPU) | Strong cloning, 8B-MoE / 900M active. **Hard caveat:** *Linux-only (x86_64) + CUDA*, ~20× slower than realtime on an 8 GB consumer GPU → only viable via WSL2 / strong GPU; unusable for live read-aloud on typical hardware. Keep behind the existing Zonos slot until a Windows path exists. |
| 6 | **Server / research backend** | **CosyVoice 3** | Apache-2.0 (lineage) | download (GPU) | Strong, multilingual, ~150 ms class, but higher integration/deployment cost; German support unconfirmed. Lower priority. |
| 7 | **Support only, do not bundle** | **F5-TTS, XTTS-v2, Fish Speech 1.5** | NC / custom (XTTS = CPML) | **gated** | Technically relevant but non-commercial/custom licenses → never a clean bundle core. XTTS is already wired as a `gated` test-only sidecar (D10). |

**Rationale.** Chatterbox is the strategic pick for the high-quality multilingual slot: **MIT**
beats both XTTS (CPML, non-commercial → gated) and Zonos (Apache but GPU/Linux-bound), it covers
German, and it slots into the existing sidecar pattern with no new architecture. Qwen3-TTS and
CosyVoice 3 are also permissive and worth supporting for GPU users, but rank below Chatterbox on
maturity/integration cost. ZONOS2 is permissive but platform-blocked today.

**Open items.**
- **Chatterbox watermark:** confirm whether the Perth watermark can be disabled or must be
  disclosed; decide how the UI/About screen surfaces "all generated audio is watermarked."
- **Kokoro German:** validate the community German voice against the German-first quality bar
  before promoting Kokoro to the Fast-Mode default (D2 still says "too risky" until then).
- **GPU realism:** ranks 3–6 all want a GPU; document the CPU-fallback story (Piper stays the
  no-GPU default) and per-backend hardware notes in the model manager.
- **Catalog entries:** add Chatterbox (and later Qwen3-TTS) to `models.json` as `download`
  runtime entries with a `setup` script, mirroring the Zonos entry; never bundle weights until
  installer-size + watermark questions are resolved.

*Informational, not legal advice — re-verify each weight license (and the Chatterbox watermark
terms) against its canonical LICENSE/model card before a commercial release.*

## D12 — Notes management & edit history

- Status: **accepted** · Date: 2026-06-22 · Implements design "Bereich 1 — Notizverwaltung & Historie"

**Context.** The design exploration settled on **Direction B "Local AI Utility"** (the green
on-device accent + dense IBM Plex typography — already the tokens in `theme.css`). Its first
worked-through area, *Bereich 1*, specifies a real note-management layer: scopes, pinning,
soft-delete, multi-select, and a diff-based edit history.

**Decision.** Implement it as designed, on the existing schema where possible:

- **Scopes.** Sidebar tabs **Active / Archived / Trash**; `list_notes(scope, sort)` /
  `search_notes(q, scope)` filter by scope. `active = deleted_at IS NULL AND archived = 0`,
  `archived = archived = 1`, `trash = deleted_at IS NOT NULL`. Sort by modified / created / title;
  pinned always first (the UI renders the pin group only in Active).
- **Pinning.** A pinned group on top; toggle via hover icon or context menu; the pin uses its own
  **amber** colour, distinct from the green action accent.
- **Soft-delete + undo, no modal confirms.** `delete_note` trashes (sets `deleted_at`);
  `restore_note` un-trashes; `hard_delete_note` and `purge_trash(before)` are the permanent ops
  (30-day retention cutoff computed client-side). Every reversible action shows an **undo toast**
  (6 s, also Ctrl/⌘+Z outside the editor); permanent deletes show a plain toast.
- **Multi-select.** Ctrl/⌘- and Shift-click select; a bulk action bar runs pin / archive / export /
  trash as one undoable batch.
- **Edit history (diff).** New `note_versions` table (`content_md`, `content_hash`, `source`
  manual|op, `op`, `provider_id`); snapshots are **deduped by content hash** (no-op saves add
  nothing). Snapshots are written on note-switch (manual baseline) and after format/OCR (op). A
  timeline overlay shows versions with op badges + word deltas and a **word-level diff** (own LCS,
  no npm) of the selected version against the current content; `restore_note_version` writes the
  old content back as a new, non-destructive, undoable version.

**Schema.** `SCHEMA_VERSION` 2 → 3; `note_versions` is created idempotently (`CREATE TABLE IF NOT
EXISTS`), so older DBs pick it up on open with no data migration.

**Verification.** DB/core unit tests cover scope partitioning, sort, restore, hard-delete, purge,
and version dedup/restore; the Tauri crate type-checks; a headless visual pass (chromium, mocked
`invoke`) confirmed every scope + interaction state against the wireframes in both themes
(screenshots under `.workspace/shots/`).

**Open items.** Manual snapshots are currently taken on note-switch only (not on a timed typing
pause); a dictation-stop snapshot isn't wired yet. Bulk export opens one native save dialog per
note (WebView2 can't batch downloads). Remaining design areas (Bereich 2–4: action handling,
settings, …) are not yet in scope.
