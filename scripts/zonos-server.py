#!/usr/bin/env python3
r"""Minimal Zonos-v0.1 HTTP sidecar for ExoQuill (EXPERIMENTAL).

Loads Zyphra Zonos-v0.1 once and serves synthesis over localhost HTTP, mirroring
the XTTS sidecar. Unlike XTTS, Zonos has no fixed studio speakers — it clones a
voice from a reference clip, so each `.wav` in --voices is one selectable voice
(its file stem is the voice id), embedded once at startup.

Endpoints:
  GET  /          -> 200 "ok"                (health check, only once ready)
  GET  /voices    -> JSON list of voice ids  (reference clip stems)
  POST /tts       -> raw int16 mono PCM @ 44.1 kHz
                     body: {"text": str, "language": "de"|"en"|...,
                            "speaker": str (voice id), "speed": float (optional),
                            "pitch": float (optional, pitch_std intonation),
                            "fmax": float (optional, Hz frequency ceiling),
                            "emotion": [float]*8 (optional, emotion vector)}

The Zonos *weights* are Apache-2.0 (fine to redistribute), but the model needs a
CUDA GPU to be usable. Zonos also depends on eSpeak NG for phonemization — it
must be installed and discoverable (see scripts/setup-zonos.ps1).

Setup:  pwsh scripts/setup-zonos.ps1
Run:    .\.venv-zonos\Scripts\python.exe scripts\zonos-server.py --port 8021 --voices .\zonos-voices
"""

import argparse
import glob
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

# Help phonemizer (Zonos' text frontend) find eSpeak NG on Windows without a
# system-wide install: the `espeakng-loader` package ships the shared library.
# phonemizer finds the .dll via PHONEMIZER_ESPEAK_LIBRARY; espeak-ng finds its
# voice data via the ESPEAK_DATA_PATH env var, pointed at the directory that
# *contains* `espeak-ng-data`. Without this the bundled DLL reports a build-time
# data path that doesn't exist here ("phontab: No such file or directory"), which
# crashes synthesis. Best-effort — if absent, a system eSpeak NG is used.
try:
    import espeakng_loader

    os.environ["PHONEMIZER_ESPEAK_LIBRARY"] = espeakng_loader.get_library_path()
    os.environ["ESPEAK_DATA_PATH"] = os.path.dirname(espeakng_loader.get_data_path())
except Exception:
    pass

# Default model — the transformer variant (broadest GPU compatibility; the hybrid
# variant needs mamba-ssm kernels that are painful on Windows). Override with
# EXOQUILL_ZONOS_MODEL.
MODEL = os.environ.get("EXOQUILL_ZONOS_MODEL", "Zyphra/Zonos-v0.1-transformer")

# torch.compile makes generation several times faster but needs an MSVC compiler
# (cl.exe) on the PATH (Inductor codegen). Off by default — eager mode is robust
# and needs no toolchain. Set EXOQUILL_ZONOS_COMPILE=1 and launch from a VS dev
# environment (vcvars on the PATH) to enable it.
USE_COMPILE = os.environ.get("EXOQUILL_ZONOS_COMPILE") == "1"

# eSpeak language codes Zonos expects, mapped from our short de/en tags.
LANGUAGE_MAP = {"de": "de", "en": "en-us"}

# Zonos' default speaking rate (phonemes/sec); our `speed` scales it.
BASE_SPEAKING_RATE = 15.0


def normalize_loudness(wav, target_rms=0.12, peak_ceiling=0.99):
    """Scale a clip to a target RMS so per-sentence volume stays consistent
    (Zonos' output level drifts between generations). Capped below `peak_ceiling`
    to avoid clipping; near-silent clips are left untouched. ~0.12 RMS ≈ -18 dBFS,
    a comfortable speech level."""
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
    from zonos.model import Zonos
    from zonos.speaker_cloning import SpeakerEmbeddingLDA

    device = "cuda" if torch.cuda.is_available() else "cpu"
    if device != "cuda":
        print("[zonos] WARNING: no CUDA GPU found — Zonos on CPU is far too slow.", flush=True)
    print(f"[zonos] loading {MODEL} on {device} ...", flush=True)
    model = Zonos.from_pretrained(MODEL, device=device)

    # Run speaker cloning on CPU: Zonos builds the embedding net under
    # `with torch.device(cuda)`, which leaves some buffers on CPU and raises
    # "tensors on cuda:0 and cpu" during embedding. CPU keeps every tensor on one
    # device; it's a one-time step at startup (not in the synthesis hot path), and
    # we move just the small resulting embedding back to the GPU for generation.
    model.spk_clone_model = SpeakerEmbeddingLDA(device="cpu")

    # Embed every reference clip once: stem -> speaker embedding (on the GPU).
    speakers = {}
    for path in sorted(glob.glob(os.path.join(voices_dir, "*.wav"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        try:
            wav, sr = torchaudio.load(path)
            speakers[stem] = model.make_speaker_embedding(wav, sr).to(device)
        except Exception as e:  # skip an unreadable clip, keep the rest
            print(f"[zonos] skip {stem}: {e}", flush=True)
    names = ", ".join(speakers) or "(none — add .wav clips to the voices folder)"
    print(f"[zonos] ready. {len(speakers)} voices: {names}", flush=True)
    return model, speakers


def make_handler(model, speakers):
    import torch
    from zonos.conditioning import make_cond_dict

    default_speaker = next(iter(speakers), None)
    sample_rate = int(model.autoencoder.sampling_rate)
    # Serialize generation: one GPU model, not safe for concurrent generate().
    lock = threading.Lock()

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
                self._send(200, json.dumps(list(speakers)).encode(), "application/json")
            else:
                self._send(200, b"ok", "text/plain")

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
                text = (req.get("text") or "").strip()
                language = LANGUAGE_MAP.get(req.get("language") or "de", "de")
                speaker_id = req.get("speaker") or default_speaker
                speed = float(req.get("speed") or 1.0)
                if not text or speaker_id not in speakers:
                    self._send(200, b"", "application/octet-stream")
                    return
                rate = max(5.0, min(30.0, BASE_SPEAKING_RATE * speed))
                # Per-request intonation (pitch_std) and brightness (fmax), each
                # falling back to the read-aloud-tuned default when the client omits
                # it. Zonos' own pitch_std=20 sounds monotone; 40-45 gives a lively-
                # but-not-crazy intonation. fmax is the synthesis frequency ceiling;
                # 22050 suits 44.1 kHz clones, lower sounds warmer/duller.
                pitch_std = float(
                    req.get("pitch")
                    if req.get("pitch") is not None
                    else os.environ.get("EXOQUILL_ZONOS_PITCH", "42")
                )
                fmax = float(req.get("fmax") if req.get("fmax") is not None else 22050.0)
                pitch_std = max(0.0, min(400.0, pitch_std))
                fmax = max(0.0, min(24000.0, fmax))
                cond_kwargs = dict(
                    text=text,
                    speaker=speakers[speaker_id],
                    language=language,
                    speaking_rate=rate,
                    pitch_std=pitch_std,
                    fmax=fmax,
                )
                # Optional emotion conditioning: an 8-value vector [happiness,
                # sadness, disgust, fear, surprise, anger, other, neutral]. Some
                # Zonos builds list "emotion" in the default unconditional_keys
                # (which would silently ignore it), so when a vector is given we
                # also pin unconditional_keys to the quality keys, leaving emotion
                # conditioned. Omitted → Zonos uses its own default vector.
                emotion = req.get("emotion")
                if isinstance(emotion, list) and len(emotion) == 8:
                    cond_kwargs["emotion"] = [float(x) for x in emotion]
                    cond_kwargs["unconditional_keys"] = ["vqscore_8", "dnsmos_ovrl"]
                with lock:
                    try:
                        cond = make_cond_dict(**cond_kwargs)
                    except TypeError:
                        # Older/newer Zonos signature without these kwargs — drop
                        # the optional ones and synthesize without emotion.
                        cond_kwargs.pop("emotion", None)
                        cond_kwargs.pop("unconditional_keys", None)
                        cond = make_cond_dict(**cond_kwargs)
                    codes = model.generate(
                        model.prepare_conditioning(cond),
                        disable_torch_compile=not USE_COMPILE,
                    )
                    audio = model.autoencoder.decode(codes).cpu().detach()
                wav = np.asarray(audio, dtype=np.float32).reshape(-1)
                wav = normalize_loudness(wav)
                pcm = np.clip(wav, -1.0, 1.0)
                pcm = (pcm * 32767.0).astype("<i2").tobytes()
                self._send(200, pcm, "application/octet-stream")
            except Exception as e:  # surface the error to the Rust client
                self._send(500, f"zonos error: {e}".encode(), "text/plain")

    Handler.sample_rate = sample_rate
    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8021)
    ap.add_argument("--voices", default="zonos-voices", help="folder of reference .wav clips")
    args = ap.parse_args()

    model, speakers = load_model(args.voices)
    server = ThreadingHTTPServer((args.host, args.port), make_handler(model, speakers))
    print(f"[zonos] serving on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
