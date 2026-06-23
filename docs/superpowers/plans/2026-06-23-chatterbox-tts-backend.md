# Chatterbox Multilingual TTS Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Chatterbox Multilingual as a third Python-HTTP-sidecar TTS backend, wired exactly like Zonos in every Rust file and mirrored in scripts/models.json.

**Architecture:** Copy the Zonos pattern at every layer — `ChatterboxServer` (owns the child process), `ChatterboxTts` (blocking HTTP client), three new AppState fields, `resolve_chatterbox_paths`, `warm_backend`/`ensure_tts_ready`/`list_tts_voices`/`tts_for` arms, a `models.rs` env-var branch, a catalog entry, and two script files. Chatterbox outputs 24 kHz mono PCM via `POST /tts`, identical wire format to Zonos.

**Tech Stack:** Rust (exoquill-ai crate, Tauri commands), Python (chatterbox-tts pip package), reqwest::blocking, std::process::Child, Tauri AppState/Mutex/AtomicBool.

## Global Constraints

- Mirror Zonos EXACTLY — same types, same Mutex/AtomicBool usage, same threading via std::thread::spawn inside warm_backend
- Provider id: `"tts.chatterbox"`, backend selector string: `"chatterbox"`, env vars: `EXOQUILL_CHATTERBOX_PYTHON` / `EXOQUILL_CHATTERBOX_SCRIPT` / `EXOQUILL_CHATTERBOX_VOICES`
- SAMPLE_RATE = 24_000 (Chatterbox native output)
- Do NOT modify any frontend files
- `cargo check --workspace` must be green — no `cargo build` or `cargo test`
- No new patterns; every decision defers to the Zonos precedent

---

## File Map

| File | Action | What changes |
|------|--------|--------------|
| `crates/exoquill-ai/src/chatterbox.rs` | **Create** | ChatterboxServer + ChatterboxTts (copy of zonos.rs, renamed) |
| `crates/exoquill-ai/src/lib.rs` | **Modify** | add `mod chatterbox; pub use chatterbox::{…}` |
| `apps/desktop/src-tauri/src/notes.rs` | **Modify** | add 3 chatterbox_* AppState fields |
| `apps/desktop/src-tauri/src/lib.rs` | **Modify** | add resolve_chatterbox_paths + populate AppState + shutdown hook |
| `apps/desktop/src-tauri/src/jobs.rs` | **Modify** | add "chatterbox" arms to tts_for / warm_backend / ensure_tts_ready / list_tts_voices |
| `apps/desktop/src-tauri/src/models.rs` | **Modify** | add "chatterbox" branch in entry_status |
| `apps/desktop/src-tauri/models.json` | **Modify** | add tts-chatterbox catalog entry |
| `scripts/setup-chatterbox.ps1` | **Create** | mirror setup-zonos.ps1 for chatterbox-tts pip package |
| `scripts/chatterbox-server.py` | **Create** | mirror zonos-server.py using chatterbox.tts ChatterboxMultilingualTTS |

---

### Task 1: Create `crates/exoquill-ai/src/chatterbox.rs`

**Files:**
- Create: `crates/exoquill-ai/src/chatterbox.rs`

**Interfaces:**
- Produces: `ChatterboxServer::start(python, script, voices_dir) -> ProviderResult<Self>`, `ChatterboxServer::client() -> Option<ChatterboxTts>`, `ChatterboxTts::connect(base_url) -> Option<Self>`, `ChatterboxTts::voices_in_dir(dir: &Path) -> Vec<TtsVoice>`, impl Provider for ChatterboxTts (id="tts.chatterbox"), impl TextToSpeechProvider for ChatterboxTts

- [ ] **Step 1: Create the file** — copy zonos.rs exactly, then replace all identifiers:
  - `ZonosServer` → `ChatterboxServer`
  - `ZonosTts` → `ChatterboxTts`
  - `"tts.zonos"` → `"tts.chatterbox"`
  - `"Zonos-v0.1"` → `"Chatterbox Multilingual"`
  - `"zonos"` (in TtsVoice.provider, quality field, error strings) → `"chatterbox"`
  - `44_100` → `24_000`
  - `"zonos sidecar"` error strings → `"chatterbox sidecar"`
  - `"Apache-2.0"` in license_info → `"MIT"`
  - `"Zyphra/Zonos-v0.1"` in license_info.source → `Some("resemble-ai/chatterbox".into())`
  - ModelRequirement model_id `"tts.zonos_v0_1"` → `"tts.chatterbox_v3"`
  - doc comment at top (update to describe Chatterbox)

The full file content:

```rust
//! Chatterbox Multilingual text-to-speech provider (Resemble AI), via a Python sidecar.
//!
//! Like Zonos, Chatterbox is multilingual (incl. German) and clones a voice from a
//! reference `.wav` clip. It outputs 24 kHz mono PCM. A small Python HTTP server
//! (`scripts/chatterbox-server.py`) loads the model once and synthesizes on
//! `POST /tts`; this is a thin blocking client, mirroring [`crate::zonos`].
//!
//! The weights are MIT-licensed (commercial ok). Enable by pointing
//! `EXOQUILL_CHATTERBOX_*` at the venv/script/voice folder; otherwise the other
//! TTS providers are used. Requires a CUDA GPU for practical speed.
//! Note: Chatterbox embeds a Resemble "Perth" watermark in every output.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{detect_language, TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Chatterbox output is fixed at 24 kHz mono (its native sample rate).
const SAMPLE_RATE: u32 = 24_000;

/// A running Chatterbox Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::zonos::ZonosServer`].
pub struct ChatterboxServer {
    child: Child,
    base_url: String,
}

impl ChatterboxServer {
    /// Spawn `python script --port P --voices DIR` and wait until the model is
    /// loaded (the sidecar only answers `GET /` once the model is ready).
    pub fn start(
        python: impl Into<PathBuf>,
        script: impl Into<PathBuf>,
        voices_dir: impl Into<PathBuf>,
    ) -> ProviderResult<Self> {
        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(python.into());
        command
            .arg(script.into())
            .arg("--port")
            .arg(port.to_string())
            .arg("--voices")
            .arg(voices_dir.into())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        below_normal_priority(&mut command);
        let child = command
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn chatterbox sidecar: {e}")))?;
        let server = Self { child, base_url };
        server.wait_ready(Duration::from_secs(600))?;
        Ok(server)
    }

    fn wait_ready(&self, timeout: Duration) -> ProviderResult<()> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(&self.base_url).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(ProviderError::Runtime(
            "chatterbox sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar.
    pub fn client(&self) -> Option<ChatterboxTts> {
        ChatterboxTts::connect(self.base_url.clone())
    }
}

impl Drop for ChatterboxServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reserve a free localhost TCP port by binding to :0 and reading it back.
fn free_port() -> ProviderResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| ProviderError::Runtime(format!("reserve port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| ProviderError::Runtime(format!("read port: {e}")))?
        .port();
    Ok(port)
}

/// Thin client for a running Chatterbox sidecar.
pub struct ChatterboxTts {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TtsBody<'a> {
    text: &'a str,
    language: &'a str,
    speaker: &'a str,
    speed: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pitch: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fmax: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<&'a [f32]>,
}

impl ChatterboxTts {
    /// Connect to a sidecar at `base_url`; `None` if it isn't reachable.
    pub fn connect(base_url: impl Into<String>) -> Option<Self> {
        let base_url = base_url.into();
        let probe = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()?;
        probe.get(&base_url).send().ok()?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .ok()?;
        Some(Self { base_url, client })
    }

    /// The selectable voices — one per `.wav` reference clip in `dir`.
    pub fn voices_in_dir(dir: &Path) -> Vec<TtsVoice> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut voices: Vec<TtsVoice> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let is_wav = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("wav"));
                if !is_wav {
                    return None;
                }
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some(TtsVoice {
                    id: stem.clone(),
                    display_name: stem.replace(['_', '-'], " "),
                    language: "auto".into(),
                    quality: "chatterbox".into(),
                    provider: "chatterbox".into(),
                })
            })
            .collect();
        voices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        voices
    }

    fn parse_speaker(voice_id: &str) -> &str {
        voice_id.trim()
    }
}

impl Provider for ChatterboxTts {
    fn id(&self) -> &str {
        "tts.chatterbox"
    }
    fn display_name(&self) -> &str {
        "Chatterbox Multilingual"
    }
    fn version(&self) -> &str {
        "3"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.chatterbox_v3".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "MIT".into(),
            source: Some("resemble-ai/chatterbox".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("chatterbox sidecar unreachable: {e}"),
            },
        }
    }
}

impl TextToSpeechProvider for ChatterboxTts {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: SAMPLE_RATE,
            });
        }
        let speaker = Self::parse_speaker(&request.voice_id);
        let language = detect_language(text);
        let body = TtsBody {
            text,
            language,
            speaker,
            speed: request.speed,
            pitch: request.intonation,
            fmax: request.brightness,
            emotion: request.emotion.as_deref(),
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("chatterbox request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "chatterbox sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("chatterbox read: {e}")))?;

        // Raw 16-bit little-endian mono PCM → normalized f32 (same as Piper/XTTS/Zonos).
        let samples = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        Ok(TtsResponse {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }

    fn voices(&self) -> Vec<TtsVoice> {
        Vec::new()
    }

    fn default_voice(&self) -> Option<String> {
        None
    }
}
```

- [ ] **Step 2: Verify file was written** — check that `crates/exoquill-ai/src/chatterbox.rs` exists and is non-empty

---

### Task 2: Update `crates/exoquill-ai/src/lib.rs`

**Files:**
- Modify: `crates/exoquill-ai/src/lib.rs`

**Interfaces:**
- Consumes: `chatterbox::ChatterboxServer`, `chatterbox::ChatterboxTts` (from Task 1)
- Produces: `pub use chatterbox::{ChatterboxServer, ChatterboxTts}` for downstream crates

- [ ] **Step 1: Add module declaration and pub use** after the existing `zonos` lines:

Add after line `pub mod zonos;`:
```rust
pub mod chatterbox;
```

Add after line `pub use zonos::{ZonosServer, ZonosTts};`:
```rust
pub use chatterbox::{ChatterboxServer, ChatterboxTts};
```

---

### Task 3: Update `apps/desktop/src-tauri/src/notes.rs`

**Files:**
- Modify: `apps/desktop/src-tauri/src/notes.rs` (the `AppState` struct, lines 21–83)

**Interfaces:**
- Consumes: `exoquill_ai::ChatterboxServer` (from Task 2)
- Produces: `AppState.chatterbox_paths: Option<(PathBuf, PathBuf, PathBuf)>`, `AppState.chatterbox_server: Mutex<Option<exoquill_ai::ChatterboxServer>>`, `AppState.chatterbox_warming: std::sync::atomic::AtomicBool`

- [ ] **Step 1: Add three fields to AppState** after the `zonos_warming` field (line 67):

```rust
    /// `(python, chatterbox-server.py, voices_dir)` to spawn the Chatterbox sidecar, or
    /// `None` when not configured. `voices_dir` holds the reference `.wav` clips
    /// (one per voice). MIT-licensed weights, but needs a CUDA GPU.
    pub chatterbox_paths: Option<(PathBuf, PathBuf, PathBuf)>,
    /// The Chatterbox sidecar, warmed up on demand (when the UI selects Chatterbox) and
    /// kept alive. Dropping it kills the Python process.
    pub chatterbox_server: Mutex<Option<exoquill_ai::ChatterboxServer>>,
    /// Guards against starting two Chatterbox sidecars concurrently (see above).
    pub chatterbox_warming: std::sync::atomic::AtomicBool,
```

---

### Task 4: Update `apps/desktop/src-tauri/src/lib.rs`

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AppState.chatterbox_paths/server/warming` (from Task 3)
- Produces: `resolve_chatterbox_paths(app: &App) -> Option<(PathBuf, PathBuf, PathBuf)>`; AppState constructed with chatterbox fields; chatterbox_server dropped on Exit

- [ ] **Step 1: Add `resolve_chatterbox_paths` function** — place it directly after `resolve_zonos_paths` (around line 270):

```rust
/// Python + `chatterbox-server.py` + a reference-voice folder, from
/// `EXOQUILL_CHATTERBOX_PYTHON` / `EXOQUILL_CHATTERBOX_SCRIPT` / `EXOQUILL_CHATTERBOX_VOICES`
/// (set by dev.ps1). `None` when not configured. Chatterbox weights are MIT-licensed,
/// but it needs a CUDA GPU, so it's opt-in via the env vars.
fn resolve_chatterbox_paths(_app: &App) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let python = std::env::var("EXOQUILL_CHATTERBOX_PYTHON")
        .map(PathBuf::from)
        .ok()?;
    let script = std::env::var("EXOQUILL_CHATTERBOX_SCRIPT")
        .map(PathBuf::from)
        .ok()?;
    let voices = std::env::var("EXOQUILL_CHATTERBOX_VOICES")
        .map(PathBuf::from)
        .ok()?;
    (python.exists() && script.exists() && voices.exists()).then_some((python, script, voices))
}
```

- [ ] **Step 2: Call `resolve_chatterbox_paths` in `run()`** — add right after `let zonos_paths = resolve_zonos_paths(app);`:

```rust
            let chatterbox_paths = resolve_chatterbox_paths(app);
```

- [ ] **Step 3: Populate AppState** — add three fields right after `zonos_warming: std::sync::atomic::AtomicBool::new(false),`:

```rust
                chatterbox_paths,
                chatterbox_server: Mutex::new(None),
                chatterbox_warming: std::sync::atomic::AtomicBool::new(false),
```

- [ ] **Step 4: Add shutdown cleanup** — add right after `state.zonos_server.lock()` block in the `Exit` handler:

```rust
                    if let Ok(mut server) = state.chatterbox_server.lock() {
                        let _ = server.take();
                    }
```

---

### Task 5: Update `apps/desktop/src-tauri/src/jobs.rs`

**Files:**
- Modify: `apps/desktop/src-tauri/src/jobs.rs`

**Interfaces:**
- Consumes: `AppState.chatterbox_paths/server/warming`, `exoquill_ai::ChatterboxTts::voices_in_dir`, `exoquill_ai::ChatterboxServer::start`

- [ ] **Step 1: Add "chatterbox" arm to `tts_for`** — add after the `Some("zonos")` arm (after line 96):

```rust
        Some("chatterbox") => state
            .chatterbox_server
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
            .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>),
```

- [ ] **Step 2: Add "chatterbox" arm to `warm_backend`** — add after the closing `}` of the `"zonos"` arm (after line 452):

```rust
        "chatterbox" => {
            let Some((python, script, voices)) = state.chatterbox_paths.clone() else {
                return;
            };
            if state
                .chatterbox_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return;
            }
            if state.chatterbox_warming.swap(true, Ordering::SeqCst) {
                return;
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::ChatterboxServer::start(python, script, voices).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) =
                        (server, state.chatterbox_server.lock())
                    {
                        *slot = Some(server);
                    }
                    state.chatterbox_warming.store(false, Ordering::SeqCst);
                }
            });
        }
```

- [ ] **Step 3: Add "chatterbox" branches to `ensure_tts_ready`** — in the `warm` closure, after the `"zonos"` arm; in the `warming` closure, after the `"zonos"` arm; in the `configured` match, after the `"zonos"` arm:

In `warm` closure:
```rust
        "chatterbox" => st.chatterbox_server.lock().map(|s| s.is_some()).unwrap_or(false),
```

In `warming` closure:
```rust
        "chatterbox" => st.chatterbox_warming.load(Ordering::SeqCst),
```

In `configured` match:
```rust
        "chatterbox" => st.chatterbox_paths.is_some(),
```

- [ ] **Step 4: Add Chatterbox voices to `list_tts_voices`** — add after the `zonos_paths` block:

```rust
    if let Some((_, _, voices_dir)) = &state.chatterbox_paths {
        voices.extend(exoquill_ai::ChatterboxTts::voices_in_dir(voices_dir));
    }
```

---

### Task 6: Update `apps/desktop/src-tauri/src/models.rs`

**Files:**
- Modify: `apps/desktop/src-tauri/src/models.rs` (the `entry_status` function, lines 100–120)

- [ ] **Step 1: Add "chatterbox" branch** — add an `else if` for chatterbox right after the `xtts` branch inside `entry_status`:

Replace:
```rust
        if entry.provider == "xtts" {
            let ok = std::env::var("EXOQUILL_XTTS_PYTHON")
                .map(|p| PathBuf::from(p).exists())
                .unwrap_or(false);
            return (ok, 0);
        }
        return (false, 0);
```

With:
```rust
        if entry.provider == "xtts" {
            let ok = std::env::var("EXOQUILL_XTTS_PYTHON")
                .map(|p| PathBuf::from(p).exists())
                .unwrap_or(false);
            return (ok, 0);
        }
        if entry.provider == "chatterbox" {
            let ok = std::env::var("EXOQUILL_CHATTERBOX_PYTHON")
                .map(|p| PathBuf::from(p).exists())
                .unwrap_or(false);
            return (ok, 0);
        }
        return (false, 0);
```

---

### Task 7: Update `apps/desktop/src-tauri/models.json`

**Files:**
- Modify: `apps/desktop/src-tauri/models.json`

- [ ] **Step 1: Add chatterbox catalog entry** — add after the closing `}` of the `tts-zonos-v0_1` entry (before the final `]`):

```json
    {
      "id": "tts-chatterbox",
      "provider": "chatterbox",
      "kind": "runtime",
      "displayName": "Chatterbox Multilingual (v3)",
      "language": "multi",
      "license": "MIT",
      "commercialOk": true,
      "tier": "download",
      "setup": "scripts/setup-chatterbox.ps1",
      "notes": "MIT-Lizenz (kommerziell ok), 23+ Sprachen inkl. Deutsch, Voice-Cloning aus Referenz-WAVs (chatterbox-voices/). Benötigt eine CUDA-GPU. Bettet ein Resemble-„Perth\"-Wasserzeichen in jede Ausgabe ein.",
      "files": []
    }
```

---

### Task 8: Create `scripts/setup-chatterbox.ps1`

**Files:**
- Create: `scripts/setup-chatterbox.ps1`

- [ ] **Step 1: Write the setup script** (mirror setup-zonos.ps1, swap package to `chatterbox-tts`):

```powershell
# Sets up a local Python venv with Resemble AI Chatterbox Multilingual for the
# EXPERIMENTAL Chatterbox TTS sidecar (scripts/chatterbox-server.py). The weights
# are MIT-licensed (commercial ok), but the model needs a CUDA GPU to be usable.
#
#   pwsh scripts/setup-chatterbox.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-chatterbox.ps1 -Cuda cu124     # RTX 30xx/40xx
#
# Pick -Cuda to match your GPU: cu128 (default) covers Blackwell (RTX 50xx,
# sm_120) and is backward-compatible down to sm_70; cu124 for RTX 30xx/40xx.
# Requires Python 3.12. Then create a voices folder with one or more 10-30 s
# reference .wav clips (each file = one voice), start the sidecar, and point
# ExoQuill at it via dev.ps1 (EXOQUILL_CHATTERBOX_*).

param(
    [string]$Cuda = "cu128"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$venv = Join-Path $root ".venv-chatterbox"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $root "chatterbox-voices"

if (-not (Test-Path $py)) {
    Write-Host "Creating venv at $venv (Python 3.12) ..."
    if (Get-Command py -ErrorAction SilentlyContinue) {
        py -3.12 -m venv $venv
    } else {
        python -m venv $venv
    }
}

& $py -m pip install --upgrade pip wheel

# PyTorch from the index matching your GPU.
& $py -m pip install "torch>=2.7,<2.9" "torchaudio>=2.7,<2.9" --index-url "https://download.pytorch.org/whl/$Cuda"

# Chatterbox Multilingual pip package.
& $py -m pip install chatterbox-tts

# A default voices folder so the sidecar has something to offer on first run.
if (-not (Test-Path $voices)) {
    New-Item -ItemType Directory -Path $voices | Out-Null
}

Write-Host ""
Write-Host "Done. Add one or more 10-30s reference .wav clips to:"
Write-Host "  $voices"
Write-Host "Then start the sidecar with:"
Write-Host "  $py scripts\chatterbox-server.py --port 8022 --voices $voices"
Write-Host "Or let ExoQuill auto-start it: the EXOQUILL_CHATTERBOX_* lines in scripts\dev.ps1."
```

---

### Task 9: Create `scripts/chatterbox-server.py`

**Files:**
- Create: `scripts/chatterbox-server.py`

- [ ] **Step 1: Write the Python sidecar** (mirror zonos-server.py, use chatterbox-tts API):

```python
#!/usr/bin/env python3
r"""Minimal Chatterbox Multilingual HTTP sidecar for ExoQuill (EXPERIMENTAL).

Loads Resemble AI Chatterbox Multilingual once and serves synthesis over localhost
HTTP, mirroring the Zonos sidecar. Like Zonos, Chatterbox clones a voice from a
reference clip, so each `.wav` in --voices is one selectable voice (its file stem
is the voice id).

Endpoints:
  GET  /          -> 200 "ok"                (health check, only once ready)
  GET  /voices    -> JSON list of voice ids  (reference clip stems)
  POST /tts       -> raw int16 mono PCM @ 24 kHz
                     body: {"text": str, "language": "de"|"en"|...,
                            "speaker": str (voice id), "speed": float (optional),
                            "pitch": float (optional),
                            "fmax": float (optional),
                            "emotion": [float]*8 (optional)}

Weights are MIT-licensed (commercial ok). Requires a CUDA GPU.
Note: Chatterbox embeds a Resemble "Perth" watermark in every output.

Setup:  pwsh scripts/setup-chatterbox.ps1
Run:    .\.venv-chatterbox\Scripts\python.exe scripts\chatterbox-server.py --port 8022 --voices .\chatterbox-voices
"""

import argparse
import glob
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

SAMPLE_RATE = 24000


def normalize_loudness(wav, target_rms=0.12, peak_ceiling=0.99):
    """Scale a clip to a target RMS so per-sentence volume stays consistent."""
    if wav.size == 0:
        return wav
    rms = float(np.sqrt(np.mean(np.square(wav))))
    if rms < 1e-5:
        return wav
    gain = target_rms / rms
    peak = float(np.max(np.abs(wav)))
    if peak * gain > peak_ceiling:
        gain = peak_ceiling / max(peak, 1e-5)
    return wav * gain


def load_model(voices_dir):
    import torch
    import torchaudio
    from chatterbox.tts import ChatterboxMultilingualTTS

    device = "cuda" if torch.cuda.is_available() else "cpu"
    if device != "cuda":
        print("[chatterbox] WARNING: no CUDA GPU found — Chatterbox on CPU is far too slow.", flush=True)
    print(f"[chatterbox] loading ChatterboxMultilingualTTS on {device} ...", flush=True)
    model = ChatterboxMultilingualTTS.from_pretrained(device=device)

    # Pre-load every reference clip from the voices folder.
    speakers = {}
    for path in sorted(glob.glob(os.path.join(voices_dir, "*.wav"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        speakers[stem] = path  # store path; clip is passed per-request
    names = ", ".join(speakers) or "(none — add .wav clips to the voices folder)"
    print(f"[chatterbox] ready. {len(speakers)} voices: {names}", flush=True)
    return model, speakers


def make_handler(model, speakers):
    import torch

    default_speaker = next(iter(speakers), None)
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def _send(self, code, body, ctype):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if body:
                self.wfile.write(body)

        def do_GET(self):
            if self.path.startswith("/voices"):
                self._send(200, json.dumps(list(speakers)).encode(), "application/json")
            else:
                self._send(200, b"ok", "text/plain")

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
                text = (req.get("text") or "").strip()
                speaker_id = req.get("speaker") or default_speaker
                speed = float(req.get("speed") or 1.0)
                if not text or speaker_id not in speakers:
                    self._send(200, b"", "application/octet-stream")
                    return
                reference_wav = speakers[speaker_id]
                with lock:
                    # generate() returns a tensor of float32 samples at 24 kHz
                    audio = model.generate(
                        text,
                        audio_prompt_path=reference_wav,
                        exaggeration=min(2.0, max(0.0, speed)),
                    )
                    if hasattr(audio, "cpu"):
                        audio = audio.cpu()
                wav = np.asarray(audio, dtype=np.float32).reshape(-1)
                wav = normalize_loudness(wav)
                pcm = np.clip(wav, -1.0, 1.0)
                pcm = (pcm * 32767.0).astype("<i2").tobytes()
                self._send(200, pcm, "application/octet-stream")
            except Exception as e:
                self._send(500, f"chatterbox error: {e}".encode(), "text/plain")

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8022)
    ap.add_argument("--voices", default="chatterbox-voices", help="folder of reference .wav clips")
    args = ap.parse_args()

    model, speakers = load_model(args.voices)
    server = ThreadingHTTPServer((args.host, args.port), make_handler(model, speakers))
    print(f"[chatterbox] serving on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
```

---

### Task 10: Run `cargo check --workspace` and iterate

- [ ] **Step 1: Run check**

```bash
cargo check --workspace 2>&1
```

Expected: `Finished` with no errors.

- [ ] **Step 2: Fix any type/name mismatches** — if errors appear, fix the exact line(s) reported and re-run.

- [ ] **Step 3: Run check once more to confirm green**

```bash
cargo check --workspace 2>&1
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in Xs`
