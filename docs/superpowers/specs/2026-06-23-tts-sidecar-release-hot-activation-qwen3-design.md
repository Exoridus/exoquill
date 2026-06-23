# Design — Sidecar-Release-Pfad, Hot-Aktivierung & Qwen3-TTS

- Status: **akzeptiert** · Datum: 2026-06-23
- Branch: `feat/bereich-2-4-settings-tts`
- Umsetzt: Priorität 3 (schreibbarer venv-Pfad im Release + neustartfreie Aktivierung)
  und Priorität 4 (Qwen3-TTS-Backend) aus dem TTS-Roadmap-Slice (D9 / D11).

## Kontext & Ziel

Die Python-TTS-Sidecars (XTTS, Zonos, Chatterbox) werden in-App per `run_setup`
installiert (legt `.venv-<name>` an, `models.rs`), aber:

1. **Release-venv-Pfad bricht.** `setup-*.ps1` legt das venv über
   `$root = Split-Path $PSScriptRoot -Parent` ab. Im Release liegen die Skripte
   schreibgeschützt im Tauri-Resource-Dir (z. B. `C:\Program Files\…`); die
   venv-Anlage scheitert, und `conventional_sidecar` (nur cwd-Walk-up) findet den
   Sidecar dort ohnehin nicht.
2. **Aktivierung braucht Neustart.** Alle TTS-Felder (`*_paths`, `tts`, `kokoro`)
   werden einmalig in `setup()` aufgelöst und sind danach unveränderlich. Nach
   `run_setup` / `install_model` bleibt das Feld `None`, bis ein App-Neustart die
   Resolver erneut ausführt — `run_setup` sagt heute explizit „Backend nach Neustart
   aktiv".
3. **Qwen3-TTS fehlt.** D11 rankt Qwen3-TTS (Rang 4, Apache-2.0, GPU, Deutsch unter
   10 Sprachen) als nächstes GPU-Backend nach Chatterbox.

Ziel dieses Slices: (A) Sidecars im Release in einen schreibbaren Pfad installieren,
(B) neu installierte/heruntergeladene TTS-Backends **ohne Neustart** aktivieren, und
(C) **Qwen3-TTS** als neues Sidecar-Backend nach dem etablierten Muster ergänzen.

## Verifizierte Annahmen

- **Qwen3-TTS ist real & lokal lauffähig.** Offene Gewichte, Apache-2.0, auf HF
  (`Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice`, `…-0.6B`, `…-Base`), veröffentlicht
  2026-01-22. Offizielles PyPI-Paket `qwen-tts`, GitHub `QwenLM/Qwen3-TTS`.
- **Inferenz-API** (`pip install -U qwen-tts`):
  ```python
  from qwen_tts import Qwen3TTSModel
  model = Qwen3TTSModel.from_pretrained(
      "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
      device_map="cuda:0", dtype=torch.bfloat16,
      attn_implementation="flash_attention_2",
  )
  wavs, sr = model.generate_custom_voice(text=…, language="Auto", speaker="Vivian", instruct=None)
  wavs, sr = model.generate_voice_clone(text=…, language="Auto", ref_audio="clip.wav", ref_text="…")
  wavs, sr = model.generate_voice_design(text=…, language="Auto", instruct="…")
  ```
- **Eingebaute Sprecher** (CustomVoice): Vivian, Serena, Uncle_Fu, Dylan, Eric, Ryan,
  Aiden, Ono_Anna, Sohee. Rückgabe `(wavs, sr)`, Sample-Rate variabel.
- **Referenz-Clip** `zonos-voices/dima.wav`: mono, 44,1 kHz, 16-bit, 27,9 s — als
  Cloning-Referenz geeignet. **Es fehlt das Transkript** (kein `.txt` daneben).

Quellen: github.com/QwenLM/Qwen3-TTS, pypi.org/project/qwen-tts,
huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice.

## Entscheidungen

| # | Frage | Entscheidung |
|---|-------|--------------|
| 1 | Aktivierungs-Umfang | Sidecars **+ native Provider** (Piper-Downloads + Kokoro auch ohne Neustart) |
| 2 | Aktivierungs-Mechanik | **Gecachte Pfade + Mutex**: `run_setup`/`install_model` lösen nach Erfolg neu auf und schreiben zurück |
| 3 | Qwen3-Stimmen | **Beide**: 9 eingebaute Sprecher (OOTB) **plus** Cloning aus Voices-Ordner |
| 4 | Qwen3-Cloning-Referenz | Dev: Wiederverwendung von `zonos-voices/dima.wav` + `dima.txt`-Transkript-Konvention |
| 5 | Qwen3-Default-Modell | `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` (`-Model`-Param überschreibbar) |
| 6 | Sidecar-Ausgaberate | Server resampled intern auf fixe **24 kHz** int16-PCM (Rust-Const, wie Chatterbox) |

---

## Teil A — Schreibbarer venv-Pfad im Release

### A1. Schreibbare Sidecar-Basis
Neue Funktion `sidecar_data_root(app) -> PathBuf`:
- **Release:** `app.path().app_data_dir()?/sidecars`.
- **Dev:** unverändert der Repo-Root-Walk-up (bestehende `.venv-*` + `zonos-voices/`
  laufen weiter; `EXOQUILL_*`-Env-Vars behalten Vorrang).

### A2. Setup-Skripte parametrisieren
Jedes `setup-<name>.ps1` (xtts, zonos, chatterbox, **qwen3tts**) bekommt:
```powershell
param([string]$Cuda = "cu128", [string]$Root = (Split-Path $PSScriptRoot -Parent))
$venv   = Join-Path $Root ".venv-<name>"
$voices = Join-Path $Root "<name>-voices"
```
Default = Repo-Root → Hand-Aufruf in Dev bleibt identisch.

### A3. `run_setup` übergibt den Root
`models::run_setup` ruft das Skript mit `-Root <sidecar_data_root>` auf (Release) bzw.
ohne (Dev, Default greift). Das Server-`.py` wird weiter aus dem Resource-Dir gelesen
(`resolve_repo_path`, read-only genügt). Der Sidecar startet:
`<writable>/.venv-<name>/Scripts/python.exe  <resource>/scripts/<name>-server.py  --voices <writable>/<name>-voices`.

### A4. Discovery beidseitig
`conventional_sidecar(name, app)` prüft **erst** die schreibbare app-data-Basis,
**dann** den Dev-Walk-up. Signatur bekommt `app: &AppHandle` (Aufrufer in lib.rs +
models.rs haben ihn). Das Server-`.py` darf aus dem Resource-Dir kommen, venv/voices
aus der schreibbaren Basis.

*Offener Punkt:* `tauri.conf` muss `scripts/` (mind. die `*-server.py` + `setup-*.ps1`)
als Bundle-Resource führen, damit `resolve_repo_path` sie im Release findet — in der
Planungsphase verifizieren.

---

## Teil B — Neustartfreie Aktivierung (gecachte Pfade + Mutex)

### B1. `AppState`-Felder hinter Mutex
- Sidecar-Pfade: `xtts_paths`, `zonos_paths`, `chatterbox_paths`, **`qwen3_paths`**
  → `Mutex<Option<(PathBuf, …)>>` (statt nacktes `Option`).
- Native Provider: `tts` (Piper) und `kokoro` → `Mutex<Option<Arc<dyn TextToSpeechProvider>>>`.
- Neu für Qwen3: `qwen3_server: Mutex<Option<exoquill_ai::Qwen3Server>>`,
  `qwen3_warming: AtomicBool`.

### B2. Resolver laufzeitfähig machen
`resolve_tts_provider` und `resolve_kokoro_native` von `&App` auf `&AppHandle`
umstellen (beide nutzen nur `app.path()`; `AppHandle` reicht). Die Sidecar-Resolver
(`resolve_xtts_paths` etc.) ebenso, plus eine neue `resolve_qwen3_paths`.

### B3. Re-Resolve-Trigger
- `run_setup(id)` (Sidecars): nach Erfolg `provider`→Resolver erneut ausführen,
  Ergebnis in den jeweiligen Mutex schreiben. Meldung: „Backend aktiv" (statt
  „nach Neustart").
- `install_model(id)` (Downloads): nach Erfolg anhand `entry.provider`:
  `piper` → `resolve_tts_provider` neu, in `tts` schreiben; `kokoro` →
  `resolve_kokoro_native` neu, in `kokoro` schreiben.
- Beide emittieren `tts_changed` (Tauri-Event) → Frontend ruft `list_tts_voices` neu.
  *Das Backend liefert nur das Event; die UI-Verdrahtung ist Aufgabe des Nutzers
  (Work-Split: UI = Nutzer).*

### B4. Lesestellen anpassen (jobs.rs)
`tts_for`, `warm_backend`, `ensure_tts_ready`, `list_tts_voices`, `list_model_info`
sperren künftig den Mutex (`lock()`), statt `.as_ref()`/`.is_some()` auf ein `Option`.
Jeweils kurz clonen (Pfade) bzw. `Arc::clone` (Provider), Lock nicht über die
Synthese halten. Unabhängige Mutexe → keine Lock-Ordering-Probleme.

---

## Teil C — Qwen3-TTS-Sidecar

Mirror des Chatterbox-Musters (Python-HTTP-Server + dünner blockierender Rust-Client).

### C1. `crates/exoquill-ai/src/qwen3tts.rs`
- `Qwen3Server { child, base_url }` mit `start(python, script, voices) -> ProviderResult<Self>`,
  `wait_ready`, `client()`, `Drop` (kill). Identisch zu `ChatterboxServer`, plus
  `below_normal_priority`.
- `Qwen3Tts { base_url, client }` — `Provider` (id `tts.qwen3`, Apache-2.0,
  display „Qwen3-TTS"), `TextToSpeechProvider::run` (POST `/tts`, 24 kHz int16-PCM →
  f32). `SAMPLE_RATE = 24_000`.
- **Stimmen:**
  - `predefined_voices() -> Vec<TtsVoice>` — statische Liste der 9 Sprecher
    (`provider: "qwen3"`, `quality: "qwen3"`, `language: "auto"`).
  - `voices_in_dir(dir) -> Vec<TtsVoice>` — pro `<name>.wav` **mit** vorhandenem
    `<name>.txt` eine Cloning-Voice (id `<name>`). Ohne `.txt` übersprungen.
- **Unit-Tests:** `voices_in_dir` filtert korrekt (wav+txt → Voice; wav ohne txt →
  keine); `predefined_voices` enthält die 9 erwarteten ids.

### C2. `scripts/qwen3tts-server.py`
- Lädt `Qwen3TTSModel.from_pretrained(model_id, device_map="cuda:0", dtype=bfloat16,
  attn_implementation=…)`. `attn_implementation` versucht `flash_attention_2`, fängt
  ImportError/Fehler ab und fällt auf `sdpa`, dann `eager` zurück.
- CPU-Warnung wie chatterbox-server.py, wenn keine CUDA-GPU.
- Indexiert beim Start die Cloning-Clips: pro `<name>.wav` mit `<name>.txt` →
  `{name: (wav_path, ref_text)}`. Plus die statische Sprecherliste.
- Endpunkte: `GET /` (Health, erst wenn geladen), `GET /voices` (eingebaute +
  Cloning-ids), `POST /tts` body `{text, language="Auto", speaker, speed?, instruct?}`.
  Routing: `speaker` ∈ eingebaute Sprecher → `generate_custom_voice`; `speaker` ∈
  Cloning-Clips → `generate_voice_clone(ref_audio, ref_text)`; sonst Default-Sprecher.
- Resampling auf 24 kHz via `torchaudio.functional.resample`, dann int16-LE-PCM.
- CLI: `--host --port --voices --model` (Default `…1.7B-CustomVoice`).

### C3. `scripts/setup-qwen3tts.ps1`
- `param([string]$Cuda="cu128", [string]$Root=(Split-Path $PSScriptRoot -Parent),
  [string]$Model="Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice")`.
- venv (Py 3.12) → `pip install --upgrade pip wheel` → torch/torchaudio vom
  `$Cuda`-Index → `pip install -U qwen-tts`. flash-attn **best-effort**
  (`try { pip install flash-attn } catch { Write-Host "flash-attn übersprungen" }`).
- `qwen3-voices/` anlegen; Hinweistext (Cloning braucht `<name>.wav` + `<name>.txt`).

### C4. `models.json`-Eintrag
```json
{
  "id": "tts-qwen3", "provider": "qwen3", "kind": "runtime",
  "displayName": "Qwen3-TTS — multilingual (experimentell)",
  "language": "multi", "license": "Apache-2.0", "commercialOk": true,
  "tier": "download", "setup": "scripts/setup-qwen3tts.ps1",
  "notes": "Apache-2.0 (kommerziell ok), 10 Sprachen inkl. Deutsch, ~4,5 GB (1.7B). Benötigt CUDA-GPU. 9 eingebaute Sprecher plus Voice-Cloning aus Referenz-WAVs (qwen3-voices/, je <name>.wav + <name>.txt-Transkript).",
  "files": []
}
```
Der bestehende `embedded_catalog_parses`-Test deckt den neuen Eintrag mit ab.

### C5. Verdrahtung
- **lib.rs:** `resolve_qwen3_paths(app)` (mirror chatterbox: `EXOQUILL_QWEN3_PYTHON/
  SCRIPT/VOICES` → sonst `conventional_sidecar("qwen3", app)`). AppState-Init:
  `qwen3_paths`/`qwen3_server`/`qwen3_warming`. Exit-Handler: `qwen3_server.take()`.
- **jobs.rs:** `tts_for` Arm `Some("qwen3")`; `warm_backend` Arm `"qwen3"`;
  `ensure_tts_ready` (warm/warming/configured um qwen3 erweitern); `list_tts_voices`
  hängt `predefined_voices()` + `voices_in_dir(voices)` an, wenn `qwen3_paths` gesetzt.
- **exoquill-ai/lib.rs:** `mod qwen3tts; pub use qwen3tts::{Qwen3Server, Qwen3Tts};`.
- **dev.ps1:** `EXOQUILL_QWEN3_PYTHON = .venv-qwen3\Scripts\python.exe`,
  `EXOQUILL_QWEN3_SCRIPT = scripts\qwen3tts-server.py`,
  `EXOQUILL_QWEN3_VOICES = zonos-voices` (Wiederverwendung von `dima.wav`).

---

## Risiken & Annahmen

- **Qwen3-API hier nicht laufzeit-verifizierbar** (kein GPU/Modell). Der Server wird
  gegen die verifizierte `qwen-tts`-API geschrieben und als experimentell markiert
  (wie der Zonos-Sidecar, D10 „unverified end-to-end"). Ersttest auf der GPU des
  Nutzers; `generate_*`-Signaturen/`sr` beim ersten echten Lauf bestätigen.
- **flash-attn auf Windows** fragil → Fallback `sdpa`/`eager` im Server, best-effort
  im Setup.
- **Cloning braucht `ref_text`** → `.txt`-Konvention; fehlt es, greifen die
  eingebauten Sprecher. *Nicht in diesem Slice:* automatische Transkription des Clips
  über die bereits gebündelte Whisper-Engine (mögliche spätere Bequemlichkeit).
- **Mutex-Refaktorierung** berührt alle TTS-Lesestellen; Locks bleiben kurz (clone/
  `Arc::clone`, nie über die Synthese gehalten).

## Verifikation

- `cargo build` (Workspace) grün; `cargo clippy -D warnings`; `cargo fmt --check`.
- Rust-Unit-Tests: `qwen3tts::voices_in_dir` (wav+txt-Filter), `predefined_voices`,
  `models::embedded_catalog_parses` (neuer Eintrag).
- Manuell auf der Nutzer-GPU: Setup via Model-Manager (schreibbarer venv-Pfad),
  Backend **ohne Neustart** wählbar, eingebauter Sprecher + `dima`-Clone hörbar.

## Nicht im Scope

- Frontend-Verdrahtung des `tts_changed`-Events (UI = Nutzer).
- Qwen3 `generate_voice_design` (freie Stimm-Beschreibung) — später.
- Automatische Clip-Transkription via Whisper.
- Bündeln von Qwen3-Gewichten (bleibt `download`/GPU).
