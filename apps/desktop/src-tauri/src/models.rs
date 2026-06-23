//! On-demand model catalog + install/delete — the model manager backend.
//!
//! The catalog (`models.json`, embedded) lists TTS assets in three tiers:
//! `bundled` (ships in the installer, redistributable), `download` (free, fetched
//! on demand), and `gated` (restrictive license, e.g. XTTS-v2 CPML — installed
//! locally via a setup script, never bundled). See docs/decisions.md for the
//! licensing rationale + the verified per-asset matrix.
//!
//! Files download to a writable models root (`EXOQUILL_MODELS_ROOT`, else the
//! app-data dir) under each file's `relPath`, which mirrors the layout the
//! providers resolve from (e.g. `piper-voices/…`). Newly downloaded voices are
//! picked up on the next app start.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const CATALOG_JSON: &str = include_str!("../models.json");

#[derive(Deserialize)]
struct Catalog {
    models: Vec<ModelEntry>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelEntry {
    id: String,
    provider: String,
    kind: String,
    display_name: String,
    language: String,
    license: String,
    commercial_ok: bool,
    tier: String,
    #[serde(default)]
    files: Vec<ModelFile>,
    #[serde(default)]
    setup: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelFile {
    url: String,
    rel_path: String,
}

/// A catalog entry plus its on-disk status, for the manager UI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    id: String,
    provider: String,
    kind: String,
    display_name: String,
    language: String,
    license: String,
    commercial_ok: bool,
    tier: String,
    setup: Option<String>,
    notes: Option<String>,
    installed: bool,
    installed_bytes: u64,
}

/// Per-file download progress, emitted on the `model_progress` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelProgress {
    id: String,
    file: String,
    downloaded: u64,
    total: u64,
}

/// One line of setup-script output, emitted on the `setup_progress` event so the
/// model manager can show a live install log.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SetupProgress {
    id: String,
    line: String,
}

/// The repo/portable root that holds `scripts/<name>-server.py`, found by walking
/// up from the current dir. This is where a setup script puts its `.venv-<name>`
/// and `<name>-voices` folder.
pub(crate) fn sidecar_root(name: &str) -> Option<PathBuf> {
    let rel = format!("scripts/{name}-server.py");
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(&rel).exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// A sidecar installed by `run_setup` at the conventional layout, if its venv
/// python exists: `(python, server script, voices dir)`. The voices dir may be
/// empty until the user adds reference clips, but the venv proves it's installed.
pub(crate) fn conventional_sidecar(name: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let root = sidecar_root(name)?;
    let python = root.join(format!(".venv-{name}/Scripts/python.exe"));
    if !python.exists() {
        return None;
    }
    let script = root.join(format!("scripts/{name}-server.py"));
    let voices = root.join(format!("{name}-voices"));
    Some((python, script, voices))
}

fn catalog() -> Catalog {
    serde_json::from_str(CATALOG_JSON).expect("embedded models.json is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the embedded `models.json` against typos (e.g. a straight `"` inside a
    /// German note closing the string early) — those would otherwise only surface as
    /// a runtime panic when the model manager opens.
    #[test]
    fn embedded_catalog_parses() {
        let cat = catalog();
        assert!(!cat.models.is_empty(), "catalog is empty");
        // Every entry must have a non-empty id and a known tier.
        for m in &cat.models {
            assert!(!m.id.is_empty(), "entry with empty id");
            assert!(
                matches!(m.tier.as_str(), "bundled" | "download" | "gated"),
                "unknown tier {:?} for {}",
                m.tier,
                m.id
            );
        }
        // The native Kokoro voices (English + German) must be present.
        assert!(cat.models.iter().any(|m| m.id == "tts-kokoro"));
        assert!(cat.models.iter().any(|m| m.id == "tts-kokoro-de"));
    }
}

/// The writable root downloaded models land in. `EXOQUILL_MODELS_ROOT` (dev →
/// `runtimes/`, where the providers look) or the app-data dir in release.
fn models_root(app: &AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("EXOQUILL_MODELS_ROOT") {
        return PathBuf::from(p);
    }
    app.path()
        .app_data_dir()
        .map(|d| d.join("models"))
        .unwrap_or_else(|_| PathBuf::from("models"))
}

/// Whether an entry is installed + the bytes it occupies. File entries check the
/// files under the models root; the XTTS runtime is detected by its env path.
fn entry_status(app: &AppHandle, entry: &ModelEntry) -> (bool, u64) {
    if entry.files.is_empty() {
        // Sidecar runtimes (no downloadable files): installed if the dev env var
        // points at a python, or a `run_setup` venv exists at the conventional path.
        let env_key = match entry.provider.as_str() {
            "xtts" => Some("EXOQUILL_XTTS_PYTHON"),
            "chatterbox" => Some("EXOQUILL_CHATTERBOX_PYTHON"),
            "zonos" => Some("EXOQUILL_ZONOS_PYTHON"),
            _ => None,
        };
        let via_env = env_key
            .and_then(|k| std::env::var(k).ok())
            .map(|p| PathBuf::from(p).exists())
            .unwrap_or(false);
        let via_setup = conventional_sidecar(&entry.provider).is_some();
        return (via_env || via_setup, 0);
    }
    let root = models_root(app);
    let mut bytes = 0u64;
    let mut all = true;
    for f in &entry.files {
        match fs::metadata(root.join(&f.rel_path)) {
            Ok(m) => bytes += m.len(),
            Err(_) => all = false,
        }
    }
    (all, bytes)
}

/// The installable model catalog with on-disk status, for the manager window.
#[tauri::command(async)]
pub fn list_catalog(app: AppHandle) -> Vec<CatalogItem> {
    catalog()
        .models
        .into_iter()
        .map(|e| {
            let (installed, installed_bytes) = entry_status(&app, &e);
            CatalogItem {
                id: e.id,
                provider: e.provider,
                kind: e.kind,
                display_name: e.display_name,
                language: e.language,
                license: e.license,
                commercial_ok: e.commercial_ok,
                tier: e.tier,
                setup: e.setup,
                notes: e.notes,
                installed,
                installed_bytes,
            }
        })
        .collect()
}

/// Download a catalog entry's files to the models root, streaming with progress
/// (`model_progress` events). Each file goes to a `.part` then atomically renamed
/// so a crash never leaves a half file looking complete. Async so the download
/// never blocks the UI thread. Gated/setup-only entries return guidance instead.
#[tauri::command(async)]
pub fn install_model(app: AppHandle, id: String) -> Result<(), String> {
    let entry = catalog()
        .models
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unbekanntes Modell: {id}"))?;
    if entry.files.is_empty() {
        return Err(match entry.setup {
            Some(s) => format!("Dieses Modell wird per Setup-Skript installiert: {s}"),
            None => "Dieses Modell lässt sich nicht herunterladen.".into(),
        });
    }

    let root = models_root(&app);
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("HTTP-Client: {e}"))?;

    for f in &entry.files {
        let dest = root.join(&f.rel_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Ordner anlegen: {e}"))?;
        }
        let mut resp = client
            .get(&f.url)
            .send()
            .map_err(|e| format!("Download {}: {e}", f.url))?;
        if !resp.status().is_success() {
            return Err(format!("Download {} → HTTP {}", f.url, resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let tmp = dest.with_extension("part");
        let mut out = fs::File::create(&tmp).map_err(|e| format!("Datei anlegen: {e}"))?;
        let mut buf = vec![0u8; 1 << 16];
        let mut downloaded = 0u64;
        loop {
            let n = resp.read(&mut buf).map_err(|e| format!("Lesen: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("Schreiben: {e}"))?;
            downloaded += n as u64;
            let _ = app.emit(
                "model_progress",
                ModelProgress {
                    id: id.clone(),
                    file: f.rel_path.clone(),
                    downloaded,
                    total,
                },
            );
        }
        out.flush().ok();
        drop(out);
        fs::rename(&tmp, &dest).map_err(|e| format!("Abschließen: {e}"))?;
    }
    Ok(())
}

/// Delete a downloaded entry's files, freeing the disk. Bundled/gated entries
/// with no downloadable files are a no-op.
#[tauri::command(async)]
pub fn delete_model(app: AppHandle, id: String) -> Result<(), String> {
    let entry = catalog()
        .models
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unbekanntes Modell: {id}"))?;
    let root = models_root(&app);
    for f in &entry.files {
        let path = root.join(&f.rel_path);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Löschen {}: {e}", f.rel_path))?;
        }
    }
    Ok(())
}

/// Resolve a repo-relative path (e.g. `scripts/setup-zonos.ps1`) to an absolute
/// one: an explicit `EXOQUILL_SCRIPTS_ROOT` override, the bundled resource dir,
/// or by walking up from the current dir (dev).
fn resolve_repo_path(app: &AppHandle, rel: &str) -> Option<PathBuf> {
    if let Ok(root) = std::env::var("EXOQUILL_SCRIPTS_ROOT") {
        let p = PathBuf::from(root).join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let p = dir.join(rel);
        if p.exists() {
            return Some(p);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Run a catalog entry's setup script (the Python-venv install for a TTS sidecar)
/// from inside the app, streaming its output line-by-line on `setup_progress`.
/// The in-app alternative to running the `.ps1` by hand. Returns Ok on a zero exit
/// code; the sidecar is then discoverable on next app start (`conventional_sidecar`).
#[tauri::command(async)]
pub fn run_setup(app: AppHandle, id: String) -> Result<(), String> {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let entry = catalog()
        .models
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unbekanntes Modell: {id}"))?;
    let setup = entry
        .setup
        .ok_or_else(|| "Dieses Modell hat kein Setup-Skript.".to_string())?;
    let script = resolve_repo_path(&app, &setup)
        .ok_or_else(|| format!("Setup-Skript nicht gefunden: {setup}"))?;

    let emit = |line: String| {
        let _ = app.emit(
            "setup_progress",
            SetupProgress {
                id: id.clone(),
                line,
            },
        );
    };
    emit(format!("▸ {}", script.display()));

    // Run via PowerShell, merging all streams (`*>&1`) so the install log is complete.
    let mut child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!("& '{}' *>&1", script.display()),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("PowerShell konnte nicht gestartet werden: {e}"))?;

    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            emit(line);
        }
    }
    let status = child.wait().map_err(|e| format!("Setup-Prozess: {e}"))?;
    if let Some(mut err) = child.stderr.take() {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        for line in s.lines() {
            emit(line.to_string());
        }
    }
    if status.success() {
        emit("✓ Einrichtung abgeschlossen — Backend nach Neustart aktiv.".to_string());
        Ok(())
    } else {
        Err(format!("Setup fehlgeschlagen (Code {:?}).", status.code()))
    }
}
