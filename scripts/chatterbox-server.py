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
    from chatterbox.tts import ChatterboxMultilingualTTS

    device = "cuda" if torch.cuda.is_available() else "cpu"
    if device != "cuda":
        print("[chatterbox] WARNING: no CUDA GPU found — Chatterbox on CPU is far too slow.", flush=True)
    print(f"[chatterbox] loading ChatterboxMultilingualTTS on {device} ...", flush=True)
    model = ChatterboxMultilingualTTS.from_pretrained(device=device)

    # Pre-index every reference clip from the voices folder (path stored; clip
    # is passed per-request to generate()).
    speakers = {}
    for path in sorted(glob.glob(os.path.join(voices_dir, "*.wav"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        speakers[stem] = path
    names = ", ".join(speakers) or "(none — add .wav clips to the voices folder)"
    print(f"[chatterbox] ready. {len(speakers)} voices: {names}", flush=True)
    return model, speakers


def make_handler(model, speakers):
    default_speaker = next(iter(speakers), None)
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
                speaker_id = req.get("speaker") or default_speaker
                speed = float(req.get("speed") or 1.0)
                if not text or speaker_id not in speakers:
                    self._send(200, b"", "application/octet-stream")
                    return
                reference_wav = speakers[speaker_id]
                with lock:
                    # generate() returns a tensor of float32 samples at 24 kHz.
                    # exaggeration controls expressiveness (1.0 = default).
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
            except Exception as e:  # surface the error to the Rust client
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
