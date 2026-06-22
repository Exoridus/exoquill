#!/usr/bin/env python3
r"""Minimal XTTS-v2 HTTP sidecar for ExoQuill (EXPERIMENTAL, test-only).

Loads Coqui XTTS-v2 once and serves synthesis over localhost HTTP, mirroring the
whisper-server pattern. Endpoints:
  GET  /          -> 200 "ok"                (health check)
  GET  /speakers  -> JSON list of names      (built-in studio speakers)
  POST /tts       -> raw int16 mono PCM @ 24 kHz
                     body: {"text": str, "language": "de"|"en"|...,
                            "speaker": str, "speed": float (optional)}

The XTTS-v2 *weights* are non-commercial (CPML); the library (coqui-tts fork) is
MPL-2.0. Run only for local testing — do not bundle the weights in a release.

Setup:  pwsh scripts/setup-xtts.ps1
Run:    .\.venv-xtts\Scripts\python.exe scripts\xtts-server.py --port 8020
Use:    $env:EXOQUILL_XTTS_URL = "http://127.0.0.1:8020"; pnpm dev
"""

import argparse
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

# Accept the CPML non-interactively so the first run can download the weights.
os.environ.setdefault("COQUI_TOS_AGREED", "1")

from TTS.api import TTS  # noqa: E402  (import after the env var is set)

MODEL = "tts_models/multilingual/multi-dataset/xtts_v2"


def load_model():
    import torch

    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"[xtts] loading {MODEL} on {device} (first run downloads ~1.8 GB) ...", flush=True)
    tts = TTS(MODEL).to(device)

    # Built-in speaker names vary by version; try the known attributes.
    speakers = []
    manager = getattr(tts.synthesizer.tts_model, "speaker_manager", None)
    for attr in ("speakers", "name_to_id"):
        table = getattr(manager, attr, None)
        if table:
            speakers = list(table.keys())
            break
    print(f"[xtts] ready. {len(speakers)} speakers: {', '.join(speakers) or '(none)'}", flush=True)
    return tts, speakers


def make_handler(tts, speakers):
    default_speaker = speakers[0] if speakers else None

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
            if self.path.startswith("/speakers"):
                self._send(200, json.dumps(speakers).encode(), "application/json")
            else:
                self._send(200, b"ok", "text/plain")

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
                text = (req.get("text") or "").strip()
                language = req.get("language") or "de"
                speaker = req.get("speaker") or default_speaker
                speed = float(req.get("speed") or 1.0)
                if not text:
                    self._send(200, b"", "application/octet-stream")
                    return
                try:
                    wav = tts.tts(text=text, speaker=speaker, language=language, speed=speed)
                except TypeError:
                    # Older builds don't accept `speed`.
                    wav = tts.tts(text=text, speaker=speaker, language=language)
                pcm = np.clip(np.asarray(wav, dtype=np.float32), -1.0, 1.0)
                pcm = (pcm * 32767.0).astype("<i2").tobytes()
                self._send(200, pcm, "application/octet-stream")
            except Exception as e:  # surface the error to the Rust client
                self._send(500, f"xtts error: {e}".encode(), "text/plain")

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8020)
    args = ap.parse_args()

    tts, speakers = load_model()
    server = ThreadingHTTPServer((args.host, args.port), make_handler(tts, speakers))
    print(f"[xtts] serving on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
