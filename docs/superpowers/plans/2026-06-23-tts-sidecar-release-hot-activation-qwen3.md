# Sidecar-Release-Pfad, Hot-Aktivierung & Qwen3-TTS — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Python-TTS-Sidecars im Release in einen schreibbaren Pfad installieren, neu installierte/heruntergeladene TTS-Backends ohne App-Neustart aktivieren, und Qwen3-TTS als neues Sidecar-Backend (eingebaute Sprecher + Voice-Cloning) ergänzen.

**Architecture:** Drei Teile auf der bestehenden Sidecar-Architektur (Python-HTTP-Server + dünner blockierender Rust-Client hinter `TextToSpeechProvider`). (A) Eine schreibbare Sidecar-Basis (`app_data_dir()/sidecars`) für venv + Voices im Release; Setup-Skripte per `-Root` parametrisiert. (B) TTS-State in `AppState` hinter `Mutex`, den `run_setup`/`install_model` nach Erfolg neu auflösen und zurückschreiben. (C) Qwen3-TTS als viertes Sidecar nach dem Chatterbox-Muster.

**Tech Stack:** Rust (Tauri v2, `reqwest::blocking`, `serde`), Python 3.12 (`qwen-tts`, `torch`/`torchaudio`, `http.server`), PowerShell-Setup-Skripte.

## Global Constraints

- Rust muss `cargo clippy --all-targets -- -D warnings` und `cargo fmt --check` bestehen (vgl. Commit `f079f3c`).
- Sidecar-Ausgabe ist überall **16-bit little-endian mono PCM**; der Rust-Client normalisiert auf `f32`. Qwen3 fixiert `SAMPLE_RATE = 24_000` (Server resampled intern).
- Provider-id-Schema: `tts.<name>` (z. B. `tts.qwen3`). `TtsVoice`-Felder: `id, display_name, language, quality, provider` (snake→camel via serde). `provider` der Qwen3-Stimmen = `"qwen3"`.
- Qwen3-Default-Modell: `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice`. Eingebaute Sprecher: `Vivian, Serena, Uncle_Fu, Dylan, Eric, Ryan, Aiden, Ono_Anna, Sohee`. Sprache an den Server immer `"Auto"`.
- Cloning-Voice = `<name>.wav` **und** `<name>.txt` (Transkript) im Voices-Ordner; ohne `.txt` wird der Clip übersprungen.
- Dev-Verhalten unverändert: Setup-Skripte ohne `-Root` legen venv/Voices weiter im Repo-Root ab; `EXOQUILL_*`-Env-Vars behalten Vorrang vor `conventional_sidecar`.
- Sprache der nutzergerichteten Strings: Deutsch mit korrekten Umlauten.

---

### Task 1: Qwen3-TTS Rust-Provider (`qwen3tts.rs`)

Spiegelt `crates/exoquill-ai/src/chatterbox.rs`. Reines Modul ohne AppState-Bezug → über Unit-Tests abgesichert.

**Files:**
- Create: `crates/exoquill-ai/src/qwen3tts.rs`
- Modify: `crates/exoquill-ai/src/lib.rs` (Modul + Re-Exporte)

**Interfaces:**
- Produces:
  - `pub struct Qwen3Server` mit `pub fn start(python: impl Into<PathBuf>, script: impl Into<PathBuf>, voices_dir: impl Into<PathBuf>) -> ProviderResult<Self>` und `pub fn client(&self) -> Option<Qwen3Tts>`.
  - `pub struct Qwen3Tts` mit `pub fn connect(base_url: impl Into<String>) -> Option<Self>`, `pub fn predefined_voices() -> Vec<TtsVoice>`, `pub fn voices_in_dir(dir: &Path) -> Vec<TtsVoice>`. Implementiert `Provider` (id `tts.qwen3`) + `TextToSpeechProvider`.

- [ ] **Step 1: Modul-Datei mit Server, Client, Tests anlegen**

Create `crates/exoquill-ai/src/qwen3tts.rs`:

```rust
//! Qwen3-TTS text-to-speech provider (Alibaba Qwen team), via a Python sidecar.
//!
//! Qwen3-TTS is multilingual (10 languages incl. German) with nine built-in
//! speakers AND voice cloning from a reference clip. A small Python HTTP server
//! (`scripts/qwen3tts-server.py`) loads the model once and synthesizes on
//! `POST /tts`; this is a thin blocking client, mirroring [`crate::chatterbox`].
//!
//! The weights are Apache-2.0 (commercial ok). Enable by pointing
//! `EXOQUILL_QWEN3_*` at the venv/script/voice folder, or install in-app via the
//! model manager. Requires a CUDA GPU for practical speed. Output is resampled to
//! 24 kHz mono by the sidecar.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// The sidecar resamples every output to 24 kHz mono (matches the Rust client).
const SAMPLE_RATE: u32 = 24_000;

/// Qwen3-TTS CustomVoice built-in speakers (no reference clip needed).
const PREDEFINED_SPEAKERS: [&str; 9] = [
    "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ryan", "Aiden", "Ono_Anna", "Sohee",
];

/// A running Qwen3 Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::chatterbox::ChatterboxServer`].
pub struct Qwen3Server {
    child: Child,
    base_url: String,
}

impl Qwen3Server {
    /// Spawn `python script --port P --voices DIR` and wait until the model is
    /// loaded (the sidecar only answers `GET /` once ready).
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
            .map_err(|e| ProviderError::Runtime(format!("spawn qwen3 sidecar: {e}")))?;
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
            "qwen3 sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar.
    pub fn client(&self) -> Option<Qwen3Tts> {
        Qwen3Tts::connect(self.base_url.clone())
    }
}

impl Drop for Qwen3Server {
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

/// Thin client for a running Qwen3 sidecar.
pub struct Qwen3Tts {
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
}

impl Qwen3Tts {
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

    /// The nine built-in CustomVoice speakers, always available (no clip needed).
    pub fn predefined_voices() -> Vec<TtsVoice> {
        PREDEFINED_SPEAKERS
            .iter()
            .map(|name| TtsVoice {
                id: (*name).to_string(),
                display_name: name.replace('_', " "),
                language: "auto".into(),
                quality: "qwen3".into(),
                provider: "qwen3".into(),
            })
            .collect()
    }

    /// Cloning voices: one per `<name>.wav` that also has a `<name>.txt` transcript
    /// (Qwen3 cloning needs the reference text). Clips without a `.txt` are skipped.
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
                if !path.with_extension("txt").exists() {
                    return None; // no transcript → not usable for Qwen3 cloning
                }
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some(TtsVoice {
                    id: stem.clone(),
                    display_name: stem.replace(['_', '-'], " "),
                    language: "auto".into(),
                    quality: "qwen3-clone".into(),
                    provider: "qwen3".into(),
                })
            })
            .collect();
        voices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        voices
    }
}

impl Provider for Qwen3Tts {
    fn id(&self) -> &str {
        "tts.qwen3"
    }
    fn display_name(&self) -> &str {
        "Qwen3-TTS"
    }
    fn version(&self) -> &str {
        "3"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.qwen3".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "Apache-2.0".into(),
            source: Some("QwenLM/Qwen3-TTS".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("qwen3 sidecar unreachable: {e}"),
            },
        }
    }
}

impl TextToSpeechProvider for Qwen3Tts {
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
        let body = TtsBody {
            text,
            language: "Auto",
            speaker: request.voice_id.trim(),
            speed: request.speed,
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("qwen3 request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "qwen3 sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("qwen3 read: {e}")))?;

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
        Some(PREDEFINED_SPEAKERS[0].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_voices_has_nine_known_speakers() {
        let voices = Qwen3Tts::predefined_voices();
        assert_eq!(voices.len(), 9);
        assert!(voices.iter().all(|v| v.provider == "qwen3"));
        assert!(voices.iter().any(|v| v.id == "Vivian"));
        assert!(voices.iter().any(|v| v.id == "Ono_Anna"));
        // display name de-underscores
        let anna = voices.iter().find(|v| v.id == "Ono_Anna").unwrap();
        assert_eq!(anna.display_name, "Ono Anna");
    }

    #[test]
    fn voices_in_dir_requires_a_transcript() {
        let dir = std::env::temp_dir().join(format!("qwen3_voices_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // clip WITH transcript → counts
        std::fs::write(dir.join("dima.wav"), b"RIFFxxxx").unwrap();
        std::fs::write(dir.join("dima.txt"), "hallo welt").unwrap();
        // clip WITHOUT transcript → skipped
        std::fs::write(dir.join("nope.wav"), b"RIFFxxxx").unwrap();
        // non-wav → ignored
        std::fs::write(dir.join("readme.md"), b"x").unwrap();

        let voices = Qwen3Tts::voices_in_dir(&dir);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "dima");
        assert_eq!(voices[0].provider, "qwen3");
        assert_eq!(voices[0].quality, "qwen3-clone");
    }
}
```

- [ ] **Step 2: Modul registrieren + re-exportieren**

In `crates/exoquill-ai/src/lib.rs` neben der Chatterbox-Zeile ergänzen. Finde:

```rust
mod chatterbox;
```
und füge danach `mod qwen3tts;` ein. Finde den Chatterbox-Re-Export (z. B. `pub use chatterbox::{ChatterboxServer, ChatterboxTts};`) und ergänze danach:

```rust
pub use qwen3tts::{Qwen3Server, Qwen3Tts};
```

> Falls die exakten Zeilen abweichen: `grep -n "chatterbox" crates/exoquill-ai/src/lib.rs` zeigt das Muster; spiegle es 1:1 für `qwen3tts`.

- [ ] **Step 3: Tests laufen lassen (müssen zunächst nicht fehlschlagen — reine neue Logik)**

Run: `cargo test -p exoquill-ai qwen3tts -- --nocapture`
Expected: PASS (`predefined_voices_has_nine_known_speakers`, `voices_in_dir_requires_a_transcript`).

- [ ] **Step 4: Lint + Format**

Run: `cargo clippy -p exoquill-ai --all-targets -- -D warnings && cargo fmt --check`
Expected: kein Fehler.

- [ ] **Step 5: Commit**

```bash
git add crates/exoquill-ai/src/qwen3tts.rs crates/exoquill-ai/src/lib.rs
git commit -m "feat(tts): Qwen3-TTS sidecar Rust client (predefined speakers + cloning)"
```

---

### Task 2: Qwen3 Python-Sidecar + Setup-Skript

Nicht laufzeit-verifizierbar (kein GPU/Modell hier); Absicherung über `py_compile` (Syntax). Inferenz gegen die verifizierte `qwen-tts`-API.

**Files:**
- Create: `scripts/qwen3tts-server.py`
- Create: `scripts/setup-qwen3tts.ps1`

**Interfaces:**
- Produces: HTTP-Sidecar mit `GET /` (Health), `GET /voices` (JSON-Liste), `POST /tts` (body `{text, language, speaker, speed?}` → raw int16 mono PCM @ 24 kHz). CLI `--host --port --voices --model`.

- [ ] **Step 1: Sidecar-Server schreiben**

Create `scripts/qwen3tts-server.py`:

```python
#!/usr/bin/env python3
r"""Minimal Qwen3-TTS HTTP sidecar for ExoQuill (EXPERIMENTAL).

Loads Alibaba Qwen3-TTS once and serves synthesis over localhost HTTP, mirroring
the Chatterbox sidecar. Qwen3 has nine built-in speakers AND voice cloning. Each
predefined speaker is a voice id; each `<name>.wav` in --voices that also has a
`<name>.txt` transcript is a cloning voice (Qwen3 cloning needs the reference text).

Endpoints:
  GET  /          -> 200 "ok"                (health check, only once ready)
  GET  /voices    -> JSON list of voice ids  (predefined + cloning)
  POST /tts       -> raw int16 mono PCM @ 24 kHz
                     body: {"text": str, "language": "Auto"|"German"|...,
                            "speaker": str (voice id), "speed": float (optional)}

Weights are Apache-2.0 (commercial ok). Requires a CUDA GPU.

Setup:  pwsh scripts/setup-qwen3tts.ps1
Run:    .\.venv-qwen3\Scripts\python.exe scripts\qwen3tts-server.py --port 8023 --voices .\qwen3-voices
"""

import argparse
import glob
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

SAMPLE_RATE = 24000
PREDEFINED = ["Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ryan", "Aiden", "Ono_Anna", "Sohee"]


def load_model(model_id, voices_dir):
    import torch
    from qwen_tts import Qwen3TTSModel

    has_cuda = torch.cuda.is_available()
    if not has_cuda:
        print("[qwen3] WARNING: no CUDA GPU found — Qwen3-TTS on CPU is far too slow.", flush=True)
    device = "cuda:0" if has_cuda else "cpu"
    dtype = torch.bfloat16 if has_cuda else torch.float32

    # flash-attn is fragile on Windows; fall back to sdpa, then eager.
    model = None
    for attn in ("flash_attention_2", "sdpa", "eager"):
        try:
            print(f"[qwen3] loading {model_id} on {device} (attn={attn}) ...", flush=True)
            model = Qwen3TTSModel.from_pretrained(
                model_id, device_map=device, dtype=dtype, attn_implementation=attn
            )
            print(f"[qwen3] loaded with attn_implementation={attn}", flush=True)
            break
        except Exception as e:  # noqa: BLE001 — try the next attn backend
            print(f"[qwen3] attn={attn} failed: {e}", flush=True)
    if model is None:
        raise RuntimeError("could not load Qwen3TTSModel with any attn implementation")

    # Index cloning clips: each <name>.wav with a sibling <name>.txt transcript.
    clones = {}
    for wav in sorted(glob.glob(os.path.join(voices_dir, "*.wav"))):
        txt = os.path.splitext(wav)[0] + ".txt"
        if not os.path.exists(txt):
            continue
        with open(txt, "r", encoding="utf-8") as f:
            ref_text = f.read().strip()
        if ref_text:
            stem = os.path.splitext(os.path.basename(wav))[0]
            clones[stem] = (wav, ref_text)
    print(f"[qwen3] ready. {len(PREDEFINED)} speakers, clones: {list(clones) or '(none)'}", flush=True)
    return model, clones


def to_pcm16(wav, sr):
    """Resample to 24 kHz mono and pack as int16 little-endian PCM bytes."""
    import torch
    import torchaudio

    t = torch.as_tensor(np.asarray(wav, dtype=np.float32)).reshape(-1)
    if sr != SAMPLE_RATE:
        t = torchaudio.functional.resample(t, int(sr), SAMPLE_RATE)
    a = np.clip(t.cpu().numpy(), -1.0, 1.0)
    return (a * 32767.0).astype("<i2").tobytes()


def make_handler(model, clones):
    default_speaker = PREDEFINED[0]
    lock = threading.Lock()  # one GPU model, serialize generate()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):  # keep the console quiet
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
                self._send(200, json.dumps(PREDEFINED + list(clones)).encode(), "application/json")
            else:
                self._send(200, b"ok", "text/plain")

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
                text = (req.get("text") or "").strip()
                speaker = req.get("speaker") or default_speaker
                language = req.get("language") or "Auto"
                if not text:
                    self._send(200, b"", "application/octet-stream")
                    return
                with lock:
                    if speaker in clones:
                        ref_audio, ref_text = clones[speaker]
                        wavs, sr = model.generate_voice_clone(
                            text=text, language=language, ref_audio=ref_audio, ref_text=ref_text
                        )
                    else:
                        spk = speaker if speaker in PREDEFINED else default_speaker
                        wavs, sr = model.generate_custom_voice(text=text, language=language, speaker=spk)
                wav = wavs[0]
                if hasattr(wav, "cpu"):
                    wav = wav.cpu().numpy()
                self._send(200, to_pcm16(wav, sr), "application/octet-stream")
            except Exception as e:  # surface the error to the Rust client
                self._send(500, f"qwen3 error: {e}".encode(), "text/plain")

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8023)
    ap.add_argument("--voices", default="qwen3-voices", help="folder of <name>.wav + <name>.txt clones")
    ap.add_argument("--model", default="Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice")
    args = ap.parse_args()

    model, clones = load_model(args.model, args.voices)
    server = ThreadingHTTPServer((args.host, args.port), make_handler(model, clones))
    print(f"[qwen3] serving on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Setup-Skript schreiben**

Create `scripts/setup-qwen3tts.ps1`:

```powershell
# Sets up a local Python venv with Alibaba Qwen3-TTS for the EXPERIMENTAL Qwen3
# TTS sidecar (scripts/qwen3tts-server.py). Weights are Apache-2.0 (commercial
# ok), but the model needs a CUDA GPU to be usable.
#
#   pwsh scripts/setup-qwen3tts.ps1                 # CUDA wheels (default cu128)
#   pwsh scripts/setup-qwen3tts.ps1 -Cuda cu124     # RTX 30xx/40xx
#
# -Root  : where to create .venv-qwen3 + qwen3-voices (release passes the writable
#          app-data sidecars dir; defaults to the repo root for dev).
# -Model : HF model id (default 1.7B CustomVoice; predefined speakers + cloning).
# Requires Python 3.12.

param(
    [string]$Cuda = "cu128",
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [string]$Model = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"
)

$ErrorActionPreference = "Stop"
$venv = Join-Path $Root ".venv-qwen3"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $Root "qwen3-voices"

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

# Qwen3-TTS pip package.
& $py -m pip install -U qwen-tts

# flash-attn is optional — the server falls back to sdpa/eager when it's missing.
& $py -m pip install flash-attn --no-build-isolation
if ($LASTEXITCODE -ne 0) {
    Write-Host "flash-attn übersprungen (optional; der Server nutzt dann sdpa/eager)."
    $global:LASTEXITCODE = 0
}

# A default voices folder so cloning has somewhere to look.
if (-not (Test-Path $voices)) {
    New-Item -ItemType Directory -Path $voices | Out-Null
}

Write-Host ""
Write-Host "Done. Built-in speakers work out of the box (Vivian, Serena, ...)."
Write-Host "For voice cloning, add <name>.wav AND <name>.txt (its transcript) to:"
Write-Host "  $voices"
Write-Host "Then start the sidecar with:"
Write-Host "  $py scripts\qwen3tts-server.py --port 8023 --voices $voices --model $Model"
Write-Host "Or let ExoQuill auto-start it (EXOQUILL_QWEN3_* in scripts\dev.ps1)."
```

- [ ] **Step 3: Python-Syntax verifizieren**

Run: `python -m py_compile scripts/qwen3tts-server.py && echo OK`
Expected: `OK` (kein Traceback). Falls `python` fehlt, `py -3.12 -m py_compile ...`.

- [ ] **Step 4: PowerShell-Syntax verifizieren**

Run: `pwsh -NoProfile -Command "$null = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path scripts/setup-qwen3tts.ps1), [ref]$null, [ref]$null); 'OK'"`
Expected: `OK` (keine Parse-Fehler).

- [ ] **Step 5: Commit**

```bash
git add scripts/qwen3tts-server.py scripts/setup-qwen3tts.ps1
git commit -m "feat(tts): Qwen3-TTS python sidecar + setup script"
```

---

### Task 3: Qwen3 `models.json`-Katalogeintrag

**Files:**
- Modify: `apps/desktop/src-tauri/models.json`
- Modify: `apps/desktop/src-tauri/src/models.rs` (Test ergänzen)

**Interfaces:**
- Consumes: das `ModelEntry`-Schema (`id, provider, kind, displayName, language, license, commercialOk, tier, setup?, notes?, files[]`).
- Produces: Katalogeintrag `tts-qwen3` (provider `qwen3`, tier `download`, setup `scripts/setup-qwen3tts.ps1`, leere `files`).

- [ ] **Step 1: Failing test ergänzen**

In `apps/desktop/src-tauri/src/models.rs`, im `mod tests`-Block, in `embedded_catalog_parses` nach den bestehenden Kokoro-Assertions ergänzen:

```rust
        // Qwen3-TTS sidecar runtime entry (download tier, setup script, no files).
        let qwen3 = cat
            .models
            .iter()
            .find(|m| m.id == "tts-qwen3")
            .expect("tts-qwen3 entry present");
        assert_eq!(qwen3.provider, "qwen3");
        assert_eq!(qwen3.tier, "download");
        assert!(qwen3.files.is_empty());
        assert_eq!(qwen3.setup.as_deref(), Some("scripts/setup-qwen3tts.ps1"));
```

- [ ] **Step 2: Test ausführen — muss fehlschlagen**

Run: `cargo test -p exoquill-desktop embedded_catalog_parses`
Expected: FAIL (`tts-qwen3 entry present` panics — Eintrag fehlt noch).

> Crate-Name unbekannt? `grep -n "^name" apps/desktop/src-tauri/Cargo.toml` zeigt ihn; nutze `-p <name>`.

- [ ] **Step 3: Katalogeintrag einfügen**

In `apps/desktop/src-tauri/models.json`, im `"models"`-Array nach dem `tts-chatterbox`-Objekt (vor `tts-kokoro`) einfügen:

```json
    {
      "id": "tts-qwen3",
      "provider": "qwen3",
      "kind": "runtime",
      "displayName": "Qwen3-TTS — multilingual (experimentell)",
      "language": "multi",
      "license": "Apache-2.0",
      "commercialOk": true,
      "tier": "download",
      "setup": "scripts/setup-qwen3tts.ps1",
      "notes": "Apache-2.0 (kommerziell ok), 10 Sprachen inkl. Deutsch, ~4,5 GB (1.7B). Benötigt eine CUDA-GPU. Neun eingebaute Sprecher plus Voice-Cloning aus Referenz-WAVs (qwen3-voices/, je <name>.wav + <name>.txt-Transkript).",
      "files": []
    },
```

> Achte auf das abschließende Komma nach `}` (es folgt `tts-kokoro`). Kein gerades `"` innerhalb der deutschen `notes` verwenden.

- [ ] **Step 4: Test ausführen — muss bestehen**

Run: `cargo test -p exoquill-desktop embedded_catalog_parses`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/models.json apps/desktop/src-tauri/src/models.rs
git commit -m "feat(models): catalog entry for Qwen3-TTS sidecar"
```

---

### Task 4: Schreibbarer venv-Pfad im Release (Teil A)

Sidecar-venv + Voices in eine schreibbare Basis legen; Setup-Skripte per `-Root` parametrisieren; `conventional_sidecar` beidseitig suchen.

**Files:**
- Modify: `apps/desktop/src-tauri/src/models.rs` (`sidecar_data_root`, `conventional_sidecar` Signatur, `run_setup` übergibt `-Root`)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (Aufrufer von `conventional_sidecar` an neue Signatur anpassen)
- Modify: `scripts/setup-xtts.ps1`, `scripts/setup-zonos.ps1`, `scripts/setup-chatterbox.ps1` (`-Root`-Param)

**Interfaces:**
- Consumes: `tauri::AppHandle`, `app.path().app_data_dir()`.
- Produces:
  - `pub(crate) fn sidecar_data_root(app: &AppHandle) -> Option<PathBuf>` — die schreibbare Basis (`app_data_dir()/sidecars`).
  - geänderte Signatur `pub(crate) fn conventional_sidecar(name: &str, app: &AppHandle) -> Option<(PathBuf, PathBuf, PathBuf)>` — prüft schreibbare Basis **und** Dev-Walk-up.

- [ ] **Step 1: `setup-*.ps1` parametrisieren (xtts, zonos, chatterbox)**

Für **jedes** der drei Skripte den Header `param(...)` und die `$root`-Zeile angleichen. Beispiel `scripts/setup-chatterbox.ps1` — ersetze:

```powershell
param(
    [string]$Cuda = "cu128"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$venv = Join-Path $root ".venv-chatterbox"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $root "chatterbox-voices"
```
durch:
```powershell
param(
    [string]$Cuda = "cu128",
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = "Stop"
$venv = Join-Path $Root ".venv-chatterbox"
$py = Join-Path $venv "Scripts\python.exe"
$voices = Join-Path $Root "chatterbox-voices"
```

Analog für `setup-zonos.ps1` (`.venv-zonos`, `zonos-voices`) und `setup-xtts.ps1` (`.venv-xtts`; XTTS hat ggf. keinen Voices-Ordner — nur `$venv`/`$py` über `$Root` führen, vorhandene Param-Liste um `[string]$Root = (Split-Path $PSScriptRoot -Parent)` erweitern und jedes `Split-Path $PSScriptRoot -Parent` im Skriptkörper durch `$Root` ersetzen).

> `grep -n "PSScriptRoot" scripts/setup-*.ps1` listet alle Stellen; jede im Körper → `$Root`.

- [ ] **Step 2: `sidecar_data_root` + beidseitige `conventional_sidecar` schreiben**

In `apps/desktop/src-tauri/src/models.rs`. Ersetze die bestehende `conventional_sidecar`-Funktion (und ergänze `sidecar_data_root`) — neue Fassung:

```rust
/// The writable base that holds installed sidecars in release: each setup script
/// puts its `.venv-<name>` + `<name>-voices` here. `app_data_dir()/sidecars`.
pub(crate) fn sidecar_data_root(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("sidecars"))
}

/// A sidecar installed by `run_setup`, if its venv python exists, as
/// `(python, server script, voices dir)`. Checks the writable app-data base
/// first (release), then the dev repo-root walk-up. The server script is resolved
/// from the repo/resource tree; venv + voices come from whichever base has the venv.
pub(crate) fn conventional_sidecar(
    name: &str,
    app: &AppHandle,
) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let script = resolve_repo_path(app, &format!("scripts/{name}-server.py"))?;
    // 1) writable app-data base (release / in-app install)
    if let Some(base) = sidecar_data_root(app) {
        let python = base.join(format!(".venv-{name}/Scripts/python.exe"));
        if python.exists() {
            return Some((python, script.clone(), base.join(format!("{name}-voices"))));
        }
    }
    // 2) dev repo-root walk-up (existing .venv-* next to scripts/<name>-server.py)
    let root = sidecar_root(name)?;
    let python = root.join(format!(".venv-{name}/Scripts/python.exe"));
    if python.exists() {
        return Some((python, script, root.join(format!("{name}-voices"))));
    }
    None
}
```

> `resolve_repo_path` und `sidecar_root` existieren bereits in dieser Datei. `conventional_sidecar` gibt nun **immer** ein Voices-Verzeichnis zurück (auch für xtts, das es ignoriert — Aufrufer in lib.rs nehmen für xtts nur `(python, script)`).

- [ ] **Step 3: `entry_status` an neue Signatur anpassen**

In `apps/desktop/src-tauri/src/models.rs`, in `entry_status`, finde:

```rust
        let via_setup = conventional_sidecar(&entry.provider).is_some();
```
ersetze durch:
```rust
        let via_setup = conventional_sidecar(&entry.provider, app).is_some();
```

- [ ] **Step 4: `run_setup` übergibt `-Root`**

In `apps/desktop/src-tauri/src/models.rs`, in `run_setup`, finde den PowerShell-Aufruf:

```rust
            &format!("& '{}' *>&1", script.display()),
```
ersetze durch (übergibt die schreibbare Basis als `-Root`, legt sie vorher an):

```rust
            &{
                match sidecar_data_root(&app) {
                    Some(root) => {
                        let _ = std::fs::create_dir_all(&root);
                        format!("& '{}' -Root '{}' *>&1", script.display(), root.display())
                    }
                    None => format!("& '{}' *>&1", script.display()),
                }
            },
```

Außerdem in `run_setup` die Erfolgsmeldung anpassen — finde:

```rust
        emit("✓ Einrichtung abgeschlossen — Backend nach Neustart aktiv.".to_string());
```
ersetze durch:
```rust
        emit("✓ Einrichtung abgeschlossen — Backend aktiv.".to_string());
```

> Das eigentliche Re-Resolve folgt in Task 6; diese Meldung wird dort sinnvoll.

- [ ] **Step 5: Aufrufer in lib.rs anpassen**

In `apps/desktop/src-tauri/src/lib.rs` rufen `resolve_zonos_paths` und `resolve_chatterbox_paths` `conventional_sidecar` ohne `app` auf. Da diese Resolver in Task 5 ohnehin auf `&AppHandle` umgestellt werden, hier nur sicherstellen, dass der Code kompiliert: vorübergehend `conventional_sidecar("zonos", app)` bzw. `("chatterbox", app)` einsetzen — d. h. die `_app`-Parameter dieser beiden Funktionen in `app: &App` umbenennen und durchreichen. Finde:

```rust
fn resolve_zonos_paths(_app: &App) -> Option<(PathBuf, PathBuf, PathBuf)> {
    env_sidecar(
        "EXOQUILL_ZONOS_PYTHON",
        "EXOQUILL_ZONOS_SCRIPT",
        "EXOQUILL_ZONOS_VOICES",
    )
    // Fall back to a sidecar installed in-app via `run_setup` (conventional layout).
    .or_else(|| crate::models::conventional_sidecar("zonos"))
}
```
ersetze durch:
```rust
fn resolve_zonos_paths(app: &AppHandle) -> Option<(PathBuf, PathBuf, PathBuf)> {
    env_sidecar(
        "EXOQUILL_ZONOS_PYTHON",
        "EXOQUILL_ZONOS_SCRIPT",
        "EXOQUILL_ZONOS_VOICES",
    )
    // Fall back to a sidecar installed in-app via `run_setup` (conventional layout).
    .or_else(|| crate::models::conventional_sidecar("zonos", app))
}
```
und analog `resolve_chatterbox_paths(_app: &App)` → `(app: &AppHandle)` mit `conventional_sidecar("chatterbox", app)`. Die Aufrufstellen in `setup()` (`resolve_zonos_paths(app)` / `resolve_chatterbox_paths(app)`) übergeben `app` — `&App` coerced zu `&AppHandle`? Nein: ändere die Aufrufe zu `resolve_zonos_paths(&app.handle().clone())` ist unnötig — stattdessen die Aufrufe in `setup()` auf `app.handle()` umstellen. Konkret in `setup()` finde `let zonos_paths = resolve_zonos_paths(app);` → `let zonos_paths = resolve_zonos_paths(&app.handle().clone());` und ebenso für chatterbox.

> `App` derefs nicht zu `AppHandle`; `app.handle()` liefert `&AppHandle`. `&app.handle().clone()` ist robust. (Task 5 vereinheitlicht alle Resolver auf `&AppHandle`.)

- [ ] **Step 6: Build + Lint**

Run: `cargo build -p exoquill-desktop && cargo clippy -p exoquill-desktop --all-targets -- -D warnings && cargo fmt --check`
Expected: grün.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/models.rs apps/desktop/src-tauri/src/lib.rs scripts/setup-xtts.ps1 scripts/setup-zonos.ps1 scripts/setup-chatterbox.ps1
git commit -m "feat(models): writable app-data venv path for sidecars in release"
```

---

### Task 5: `AppState`-TTS-State hinter Mutex (Teil B — Refactor)

Mechanischer Refactor: Sidecar-Pfade + native Provider hinter `Mutex`. Verhalten zunächst unverändert (noch kein Re-Resolve). Abgesichert durch `cargo build` + bestehende Tests.

**Files:**
- Modify: `apps/desktop/src-tauri/src/notes.rs` (AppState-Feldtypen)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (Resolver-Signaturen `&AppHandle`, Init, Exit)
- Modify: `apps/desktop/src-tauri/src/jobs.rs` (Lesestellen + Helper)

**Interfaces:**
- Produces (AppState-Felder, neue Typen):
  - `tts: Mutex<Option<Arc<dyn TextToSpeechProvider>>>`
  - `kokoro: Mutex<Option<Arc<dyn TextToSpeechProvider>>>` (weiterhin `#[cfg(feature = "kokoro")]`)
  - `xtts_paths: Mutex<Option<(PathBuf, PathBuf)>>`
  - `zonos_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>`
  - `chatterbox_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>`
  - Helper in jobs.rs: `fn slot_provider(slot: &Mutex<Option<Arc<dyn TextToSpeechProvider>>>) -> Option<Arc<dyn TextToSpeechProvider>>`.

- [ ] **Step 1: AppState-Feldtypen ändern**

In `apps/desktop/src-tauri/src/notes.rs`, in `pub struct AppState`, die Felder umtypen (Doc-Kommentare beibehalten/anpassen):

- `pub tts: Option<Arc<dyn TextToSpeechProvider>>,` → `pub tts: Mutex<Option<Arc<dyn TextToSpeechProvider>>>,`
- `pub xtts_paths: Option<(PathBuf, PathBuf)>,` → `pub xtts_paths: Mutex<Option<(PathBuf, PathBuf)>>,`
- `pub zonos_paths: Option<(PathBuf, PathBuf, PathBuf)>,` → `pub zonos_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>,`
- `pub chatterbox_paths: Option<(PathBuf, PathBuf, PathBuf)>,` → `pub chatterbox_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>,`
- `pub kokoro: Option<Arc<dyn TextToSpeechProvider>>,` → `pub kokoro: Mutex<Option<Arc<dyn TextToSpeechProvider>>>,` (unter dem bestehenden `#[cfg(feature = "kokoro")]`)

> `Mutex` ist in notes.rs bereits importiert (andere Felder nutzen es).

- [ ] **Step 2: Resolver auf `&AppHandle` vereinheitlichen**

In `apps/desktop/src-tauri/src/lib.rs` die Signaturen umstellen, damit sie zur Laufzeit (aus models.rs) aufrufbar sind:

- `fn resolve_tts_provider(app: &App)` → `pub(crate) fn resolve_tts_provider(app: &AppHandle)`
- `#[cfg(feature = "kokoro")] fn resolve_kokoro_native(app: &App)` → `#[cfg(feature = "kokoro")] pub(crate) fn resolve_kokoro_native(app: &AppHandle)`
- `fn resolve_xtts_paths(_app: &App)` → `pub(crate) fn resolve_xtts_paths(app: &AppHandle)` (Body: `_app` wird nicht gebraucht; benenne den Parameter `_app` und lasse ihn ungenutzt, ABER mit `&AppHandle`-Typ — `pub(crate) fn resolve_xtts_paths(_app: &AppHandle)`)
- `resolve_zonos_paths` / `resolve_chatterbox_paths`: bereits in Task 4 auf `&AppHandle` gebracht → zusätzlich `pub(crate)` machen.

Innerhalb `resolve_tts_provider`/`resolve_kokoro_native` funktionieren `app.path().resource_dir()` und `app.path().app_data_dir()` unverändert mit `&AppHandle`.

- [ ] **Step 3: `setup()`-Initialisierung anpassen**

In `apps/desktop/src-tauri/src/lib.rs`, im `.setup(|app| { ... })`, die Resolver-Aufrufe auf `app.handle()` umstellen und die `app.manage(AppState { ... })`-Felder in `Mutex::new(...)` wickeln. Finde den Block:

```rust
            let tts = resolve_tts_provider(app);
            let xtts_paths = resolve_xtts_paths(app);
            let zonos_paths = resolve_zonos_paths(app);
            let chatterbox_paths = resolve_chatterbox_paths(app);
```
ersetze durch:
```rust
            let handle = app.handle().clone();
            let tts = resolve_tts_provider(&handle);
            let xtts_paths = resolve_xtts_paths(&handle);
            let zonos_paths = resolve_zonos_paths(&handle);
            let chatterbox_paths = resolve_chatterbox_paths(&handle);
```

Dann im `AppState { ... }`-Literal die betroffenen Felder umstellen:

```rust
                tts: Mutex::new(tts),
                xtts_paths: Mutex::new(xtts_paths),
                ...
                zonos_paths: Mutex::new(zonos_paths),
                ...
                chatterbox_paths: Mutex::new(chatterbox_paths),
                ...
                #[cfg(feature = "kokoro")]
                kokoro: Mutex::new(resolve_kokoro_native(&handle)),
```
(Die `*_server: Mutex::new(None)` und `*_warming`-Felder bleiben unverändert.)

- [ ] **Step 4: Exit-Handler — kein Provider-Take nötig**

Der Exit-Handler in `.run(|app_handle, event| { ... })` nimmt nur die `*_server`-Slots (`Mutex<Option<Server>>`) — die sind unverändert. **Keine Änderung** an `tts`/`kokoro` nötig (native, kein Kindprozess). Nur prüfen, dass nichts auf das alte `Option`-`tts` zugreift.

- [ ] **Step 5: jobs.rs — Helper + Lesestellen umstellen**

In `apps/desktop/src-tauri/src/jobs.rs` zuoberst (nach den `use`) den Helper ergänzen:

```rust
/// Clone the provider currently in a mutex-guarded slot, if any (poisoned → None).
fn slot_provider(
    slot: &std::sync::Mutex<Option<Arc<dyn TextToSpeechProvider>>>,
) -> Option<Arc<dyn TextToSpeechProvider>> {
    slot.lock().ok().and_then(|g| g.clone())
}
```

Dann in `tts_for` die Provider-Zugriffe umstellen:
- `Some("piper") => state.tts.clone(),` → `Some("piper") => slot_provider(&state.tts),`
- `#[cfg(feature = "kokoro")] Some("kokoro") => state.kokoro.clone(),` → `... => slot_provider(&state.kokoro),`
- Im `Some("xtts")`-Arm: `state.xtts_paths.as_ref()?;` → `state.xtts_paths.lock().ok().and_then(|g| g.clone())?;`
- Im Auto-Arm `_ =>`: `if state.xtts_paths.is_some() {` → `if state.xtts_paths.lock().map(|g| g.is_some()).unwrap_or(false) {` und das abschließende `state.tts.clone()` → `slot_provider(&state.tts)`.

In `warm_backend`:
- `let Some((python, script)) = state.xtts_paths.clone() else { return; };` → `let Some((python, script)) = state.xtts_paths.lock().ok().and_then(|g| g.clone()) else { return; };`
- `let Some((python, script, voices)) = state.zonos_paths.clone() else { ... };` → `... = state.zonos_paths.lock().ok().and_then(|g| g.clone()) else { ... };`
- analog chatterbox.

In `ensure_tts_ready`, im `configured`-Closure:
- `"xtts" => st.xtts_paths.is_some(),` → `"xtts" => st.xtts_paths.lock().map(|g| g.is_some()).unwrap_or(false),`
- analog `zonos`, `chatterbox`.

In `list_tts_voices`:
- `let mut voices = state.tts.as_ref().map(|tts| tts.voices()).unwrap_or_default();` → 
  ```rust
  let mut voices = slot_provider(&state.tts).map(|tts| tts.voices()).unwrap_or_default();
  ```
- `if state.xtts_paths.is_some() {` → `if state.xtts_paths.lock().map(|g| g.is_some()).unwrap_or(false) {`
- `if let Some((_, _, voices_dir)) = &state.zonos_paths {` → 
  ```rust
  if let Some((_, _, voices_dir)) = state.zonos_paths.lock().ok().and_then(|g| g.clone()) {
      voices.extend(exoquill_ai::ZonosTts::voices_in_dir(&voices_dir));
  }
  ```
  (analog chatterbox; beachte `&voices_dir` statt `voices_dir`).
- Kokoro-Block: `if let Some(kokoro) = state.kokoro.as_ref() {` → `if let Some(kokoro) = slot_provider(&state.kokoro) {`.

In `list_model_info`:
- `match state.tts.as_ref() {` → `match slot_provider(&state.tts) {`
- innerhalb: `Some(tts) => out.push(describe("tts", tts.as_ref())),` bleibt (`tts` ist jetzt ein `Arc`, `tts.as_ref()` liefert `&dyn`).

- [ ] **Step 6: Build + Lint + bestehende Tests**

Run: `cargo build -p exoquill-desktop && cargo clippy -p exoquill-desktop --all-targets -- -D warnings && cargo fmt --check && cargo test -p exoquill-desktop`
Expected: grün. (Verhalten unverändert — nur Locking eingezogen.)

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/notes.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/jobs.rs
git commit -m "refactor(state): TTS providers + sidecar paths behind Mutex"
```

---

### Task 6: Hot-Aktivierung — Re-Resolve nach Setup/Download (Teil B)

`run_setup` (Sidecars) und `install_model` (native Downloads) lösen nach Erfolg neu auf, schreiben in den Mutex und emittieren `tts_changed`. Kein Neustart mehr nötig.

**Files:**
- Modify: `apps/desktop/src-tauri/src/models.rs` (`run_setup`, `install_model` Re-Resolve + Event)

**Interfaces:**
- Consumes: `crate::resolve_xtts_paths`, `resolve_zonos_paths`, `resolve_chatterbox_paths`, `resolve_tts_provider`, `resolve_kokoro_native` (alle `pub(crate)`, `&AppHandle`, aus Task 5); `AppState`-Mutex-Felder.
- Produces: Tauri-Event `tts_changed` (kein Payload nötig) nach erfolgreicher (De-)Installation.

- [ ] **Step 1: Re-Resolve-Helper in models.rs**

In `apps/desktop/src-tauri/src/models.rs` (oben, nach den `use`) ergänzen. `AppState` importieren falls nötig (`use crate::notes::AppState;`):

```rust
/// After a sidecar/model install, re-resolve the affected provider and store it
/// into the live `AppState` so the backend is usable without an app restart.
/// `provider` is the catalog entry's `provider` field. Emits `tts_changed` so the
/// UI re-fetches the voice list. Best-effort: a missing state/handle is a no-op.
fn reactivate_provider(app: &AppHandle, provider: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match provider {
        "xtts" => {
            let resolved = crate::resolve_xtts_paths(app);
            if let Ok(mut slot) = state.xtts_paths.lock() {
                *slot = resolved;
            }
        }
        "zonos" => {
            let resolved = crate::resolve_zonos_paths(app);
            if let Ok(mut slot) = state.zonos_paths.lock() {
                *slot = resolved;
            }
        }
        "chatterbox" => {
            let resolved = crate::resolve_chatterbox_paths(app);
            if let Ok(mut slot) = state.chatterbox_paths.lock() {
                *slot = resolved;
            }
        }
        "piper" => {
            let resolved = crate::resolve_tts_provider(app);
            if let Ok(mut slot) = state.tts.lock() {
                *slot = resolved;
            }
        }
        #[cfg(feature = "kokoro")]
        "kokoro" => {
            let resolved = crate::resolve_kokoro_native(app);
            if let Ok(mut slot) = state.kokoro.lock() {
                *slot = resolved;
            }
        }
        _ => {}
    }
    let _ = app.emit("tts_changed", ());
}
```

> `app.try_state` und `app.emit` brauchen `Manager`/`Emitter` (in models.rs bereits via `use tauri::{AppHandle, Emitter, Manager};` importiert).

- [ ] **Step 2: `run_setup` ruft Re-Resolve auf Erfolg**

In `apps/desktop/src-tauri/src/models.rs`, in `run_setup`, am Erfolgszweig — finde:

```rust
    if status.success() {
        emit("✓ Einrichtung abgeschlossen — Backend aktiv.".to_string());
        Ok(())
    } else {
```
ersetze durch:
```rust
    if status.success() {
        reactivate_provider(&app, &entry.provider);
        emit("✓ Einrichtung abgeschlossen — Backend aktiv (kein Neustart nötig).".to_string());
        Ok(())
    } else {
```

> `entry` wird in `run_setup` früh per `.find(...)` ermittelt; `entry.provider` ist verfügbar. Falls `entry` durch `entry.setup`-`ok_or` teil-gemoved wurde: ziehe `let provider = entry.provider.clone();` vor das `let setup = entry.setup...`-Statement und nutze `&provider`.

- [ ] **Step 3: `install_model` ruft Re-Resolve auf Erfolg (native Downloads)**

In `apps/desktop/src-tauri/src/models.rs`, in `install_model`, am Ende vor `Ok(())` — finde den Schluss der Funktion:

```rust
        fs::rename(&tmp, &dest).map_err(|e| format!("Abschließen: {e}"))?;
    }
    Ok(())
}
```
ersetze durch:
```rust
        fs::rename(&tmp, &dest).map_err(|e| format!("Abschließen: {e}"))?;
    }
    // Newly downloaded Piper voices / Kokoro assets activate without a restart.
    reactivate_provider(&app, &entry.provider);
    Ok(())
}
```

> `entry` ist in `install_model` verfügbar (per `.find`). `entry.provider` ist ein `String`.

- [ ] **Step 4: Build + Lint**

Run: `cargo build -p exoquill-desktop && cargo clippy -p exoquill-desktop --all-targets -- -D warnings && cargo fmt --check`
Expected: grün.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/models.rs
git commit -m "feat(models): activate sidecars + downloaded voices without restart"
```

---

### Task 7: Qwen3 vollständig verdrahten (Teil C)

AppState-Felder, Resolver, jobs.rs-Arme, Exit-Cleanup, Re-Resolve-Arm, dev.ps1.

**Files:**
- Modify: `apps/desktop/src-tauri/src/notes.rs` (qwen3 AppState-Felder)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (`resolve_qwen3_paths`, Init, Exit)
- Modify: `apps/desktop/src-tauri/src/jobs.rs` (`tts_for`, `warm_backend`, `ensure_tts_ready`, `list_tts_voices`)
- Modify: `apps/desktop/src-tauri/src/models.rs` (`reactivate_provider` qwen3-Arm)
- Modify: `scripts/dev.ps1` (`EXOQUILL_QWEN3_*`)

**Interfaces:**
- Consumes: `exoquill_ai::{Qwen3Server, Qwen3Tts}` (Task 1); `crate::models::conventional_sidecar` (Task 4).
- Produces: AppState-Felder `qwen3_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>`, `qwen3_server: Mutex<Option<exoquill_ai::Qwen3Server>>`, `qwen3_warming: AtomicBool`; `pub(crate) fn resolve_qwen3_paths(app: &AppHandle)`.

- [ ] **Step 1: AppState-Felder ergänzen**

In `apps/desktop/src-tauri/src/notes.rs`, in `pub struct AppState`, nach dem `chatterbox_warming`-Feld einfügen:

```rust
    /// `(python, qwen3tts-server.py, voices_dir)` to spawn the Qwen3-TTS sidecar,
    /// or `None` when not configured. Apache-2.0 weights, needs a CUDA GPU. Built-in
    /// speakers plus cloning from `<name>.wav` + `<name>.txt` in `voices_dir`.
    pub qwen3_paths: Mutex<Option<(PathBuf, PathBuf, PathBuf)>>,
    /// The Qwen3 sidecar, warmed up on demand (when the UI selects Qwen3) and kept
    /// alive. Dropping it kills the Python process.
    pub qwen3_server: Mutex<Option<exoquill_ai::Qwen3Server>>,
    /// Guards against starting two Qwen3 sidecars concurrently.
    pub qwen3_warming: std::sync::atomic::AtomicBool,
```

- [ ] **Step 2: `resolve_qwen3_paths` + Init + Exit in lib.rs**

In `apps/desktop/src-tauri/src/lib.rs` nach `resolve_chatterbox_paths` ergänzen:

```rust
/// Python + `qwen3tts-server.py` + a reference-voice folder, from
/// `EXOQUILL_QWEN3_PYTHON` / `EXOQUILL_QWEN3_SCRIPT` / `EXOQUILL_QWEN3_VOICES`
/// (set by dev.ps1), else a sidecar installed in-app via `run_setup`. Apache-2.0
/// weights; needs a CUDA GPU, so it's opt-in.
pub(crate) fn resolve_qwen3_paths(app: &AppHandle) -> Option<(PathBuf, PathBuf, PathBuf)> {
    env_sidecar(
        "EXOQUILL_QWEN3_PYTHON",
        "EXOQUILL_QWEN3_SCRIPT",
        "EXOQUILL_QWEN3_VOICES",
    )
    .or_else(|| crate::models::conventional_sidecar("qwen3", app))
}
```

In `setup()` neben den anderen Resolver-Aufrufen: `let qwen3_paths = resolve_qwen3_paths(&handle);`. Im `AppState { ... }`-Literal (nach den chatterbox-Feldern, vor `#[cfg(feature = "kokoro")] kokoro`):

```rust
                qwen3_paths: Mutex::new(qwen3_paths),
                qwen3_server: Mutex::new(None),
                qwen3_warming: std::sync::atomic::AtomicBool::new(false),
```

Im Exit-Handler (`.run(|app_handle, event| { ... }`), nach dem chatterbox-`server.take()`-Block:

```rust
                    if let Ok(mut server) = state.qwen3_server.lock() {
                        let _ = server.take();
                    }
```

- [ ] **Step 3: jobs.rs — qwen3-Arme**

In `apps/desktop/src-tauri/src/jobs.rs`:

In `tts_for`, nach dem `Some("chatterbox") => ...`-Arm:

```rust
        Some("qwen3") => state
            .qwen3_server
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
            .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>),
```

In `warm_backend`, nach dem `"chatterbox" => { ... }`-Arm:

```rust
        "qwen3" => {
            let Some((python, script, voices)) =
                state.qwen3_paths.lock().ok().and_then(|g| g.clone())
            else {
                return;
            };
            if state
                .qwen3_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return;
            }
            if state.qwen3_warming.swap(true, Ordering::SeqCst) {
                return;
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::Qwen3Server::start(python, script, voices).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) = (server, state.qwen3_server.lock()) {
                        *slot = Some(server);
                    }
                    state.qwen3_warming.store(false, Ordering::SeqCst);
                }
            });
        }
```

In `ensure_tts_ready`, in den drei Closures je einen qwen3-Zweig ergänzen:
- `warm`: `"qwen3" => st.qwen3_server.lock().map(|s| s.is_some()).unwrap_or(false),`
- `warming`: `"qwen3" => st.qwen3_warming.load(Ordering::SeqCst),`
- `configured`: `"qwen3" => st.qwen3_paths.lock().map(|g| g.is_some()).unwrap_or(false),`

In `list_tts_voices`, nach dem chatterbox-Block (vor dem Kokoro-Block):

```rust
    if let Some((_, _, voices_dir)) = state.qwen3_paths.lock().ok().and_then(|g| g.clone()) {
        voices.extend(exoquill_ai::Qwen3Tts::predefined_voices());
        voices.extend(exoquill_ai::Qwen3Tts::voices_in_dir(&voices_dir));
    }
```

- [ ] **Step 4: `reactivate_provider` qwen3-Arm**

In `apps/desktop/src-tauri/src/models.rs`, in `reactivate_provider`, nach dem `"chatterbox" => { ... }`-Arm:

```rust
        "qwen3" => {
            let resolved = crate::resolve_qwen3_paths(app);
            if let Ok(mut slot) = state.qwen3_paths.lock() {
                *slot = resolved;
            }
        }
```

- [ ] **Step 5: dev.ps1 — Env-Vars (Cloning reuses `dima.wav`)**

In `scripts/dev.ps1`, nach den `EXOQUILL_ZONOS_*`-Zeilen einfügen:

```powershell
# Auto-start the experimental Qwen3-TTS sidecar (Apache-2.0 weights, CUDA GPU).
# Built-in speakers plus voice cloning. Reuses the existing zonos-voices/ folder
# so dima.wav can be a clone — add dima.txt (its transcript) for cloning to work.
$env:EXOQUILL_QWEN3_PYTHON = Join-Path $root ".venv-qwen3\Scripts\python.exe"
$env:EXOQUILL_QWEN3_SCRIPT = Join-Path $root "scripts\qwen3tts-server.py"
$env:EXOQUILL_QWEN3_VOICES = Join-Path $root "zonos-voices"
```

- [ ] **Step 6: Build + Lint + Tests**

Run: `cargo build -p exoquill-desktop && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && cargo test --workspace`
Expected: grün. (Qwen3-Unit-Tests aus Task 1 laufen mit.)

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/notes.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/jobs.rs apps/desktop/src-tauri/src/models.rs scripts/dev.ps1
git commit -m "feat(tts): wire Qwen3-TTS backend (state, warm-up, voices, dev env)"
```

---

### Task 8: Doku + manuelle Verifikation

**Files:**
- Modify: `docs/decisions.md` (D11 Open Item „next to wire" aktualisieren)

- [ ] **Step 1: D11-Status pflegen**

In `docs/decisions.md`, in der D11-Tabelle die Qwen3-Zeile (Rang 4) am Ende ergänzen: `**Wired** (experimentell, GPU; eingebaute Sprecher + Cloning).` und im D11-Open-Items-Abschnitt den Catalog-Hinweis auf Qwen3 als erledigt markieren.

- [ ] **Step 2: Commit**

```bash
git add docs/decisions.md
git commit -m "docs(decisions): mark Qwen3-TTS as wired (D11)"
```

- [ ] **Step 3: Manuelle Verifikation auf der Nutzer-GPU (kein automatischer Test möglich)**

Checkliste (durch den Nutzer auszuführen — kein GPU/Modell in dieser Umgebung):
1. Im Model-Manager „Qwen3-TTS" per Setup installieren → venv landet unter `app_data_dir()/sidecars/.venv-qwen3` (Release) bzw. Repo-Root (Dev).
2. **Ohne Neustart**: Backend „Qwen3" in der TTS-Auswahl wählbar, Stimmenliste enthält die 9 Sprecher.
3. Einen eingebauten Sprecher vorlesen → deutsche Ausgabe hörbar.
4. `dima.txt` (Transkript) neben `zonos-voices/dima.wav` anlegen → Voice „dima" erscheint, Clone hörbar.
5. Eine Piper-Stimme herunterladen → erscheint **ohne Neustart** in der Liste (Teil B native).

---

## Self-Review

**Spec coverage:**
- Teil A (writable venv) → Task 4 (+ Setup-Skripte, `run_setup -Root`, `conventional_sidecar` beidseitig). ✓
- Teil B Mechanik (Mutex) → Task 5; Re-Resolve-Trigger → Task 6; native Provider (Piper/Kokoro) → Task 6 `reactivate_provider`. ✓
- Teil C (Qwen3) → Tasks 1 (Rust), 2 (Python/Setup), 3 (models.json), 7 (Verdrahtung). ✓
- Entscheidung 5 (Default-Modell) → Task 2 Setup `-Model` + Server `--model`. ✓
- Entscheidung 6 (24 kHz) → Task 1 `SAMPLE_RATE`, Task 2 `to_pcm16`. ✓
- Cloning braucht `.txt` → Task 1 `voices_in_dir`, Task 2 Server-Indexierung, Task 7 dev.ps1-Hinweis. ✓
- Offener Punkt „scripts/ als Bundle-Resource" → in Task 4 Step notiert; **Verifikation** gehört in die Release-Packaging-Arbeit (außerhalb dieses Slices, da kein Release-Build hier). Bewusst kein Task — als Risiko in der Spec geführt.

**Placeholder scan:** Keine TBD/TODO; jeder Code-Step zeigt vollständigen Code oder exakte Such/Ersetz-Anweisungen. ✓

**Type consistency:** `Qwen3Server`/`Qwen3Tts`, `predefined_voices()`, `voices_in_dir()`, `resolve_qwen3_paths()`, `reactivate_provider()`, `slot_provider()`, Feldnamen `qwen3_paths`/`qwen3_server`/`qwen3_warming`, Provider-id `tts.qwen3`, `SAMPLE_RATE = 24_000` — durchgängig identisch verwendet. `conventional_sidecar(name, app)` neue Signatur in Task 4 definiert und in Tasks 6/7 so genutzt. ✓
