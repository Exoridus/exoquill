#!/usr/bin/env python3
r"""Minimal Kokoro-82M HTTP sidecar for ExoQuill (EXPERIMENTAL).

Loads hexgrad/Kokoro-82M once and serves synthesis over localhost HTTP,
mirroring the Chatterbox sidecar. Unlike Chatterbox/Zonos, Kokoro has a fixed
set of built-in voices — no reference .wav clips are needed.

Endpoints:
  GET  /          -> 200 "ok"                (health check, only once ready)
  POST /tts       -> raw int16 mono PCM @ 24 kHz
                     body: {"text": str, "voice": str (voice id), "speed": float}

Weights are Apache-2.0 licensed (commercial ok). Runs on CPU.

Setup:  pwsh scripts/setup-kokoro.ps1
Run:    .\.venv-kokoro\Scripts\python.exe scripts\kokoro-server.py --port 8023
"""

import argparse
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np

SAMPLE_RATE = 24000

# Built-in voice set matching kokoro.rs VOICES constant.
DEFAULT_VOICE = "af_heart"
VOICES = ["af_heart", "af_bella", "am_michael", "bf_emma", "bm_george"]


def load_model():
    from kokoro import KPipeline

    print("[kokoro] loading KPipeline (lang='a' = American English) ...", flush=True)
    pipeline = KPipeline(lang_code="a")
    print(f"[kokoro] ready. Built-in voices: {', '.join(VOICES)}", flush=True)
    return pipeline


def make_handler(pipeline):
    # Serialize generation: one model instance, not safe for concurrent calls.
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
            self._send(200, b"ok", "text/plain")

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", 0))
                req = json.loads(self.rfile.read(length) or b"{}")
                text = (req.get("text") or "").strip()
                voice = req.get("voice") or DEFAULT_VOICE
                speed = float(req.get("speed") or 1.0)
                if not text:
                    self._send(200, b"", "application/octet-stream")
                    return
                if voice not in VOICES:
                    voice = DEFAULT_VOICE
                with lock:
                    # KPipeline returns a generator of (graphemes, phonemes, audio)
                    # tuples; concatenate all audio chunks.
                    chunks = []
                    for _, _, audio in pipeline(text, voice=voice, speed=speed):
                        if audio is not None:
                            arr = np.asarray(audio, dtype=np.float32).reshape(-1)
                            chunks.append(arr)
                wav = np.concatenate(chunks) if chunks else np.array([], dtype=np.float32)
                pcm = np.clip(wav, -1.0, 1.0)
                pcm = (pcm * 32767.0).astype("<i2").tobytes()
                self._send(200, pcm, "application/octet-stream")
            except Exception as e:  # surface the error to the Rust client
                self._send(500, f"kokoro error: {e}".encode(), "text/plain")

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8023)
    args = ap.parse_args()

    pipeline = load_model()
    server = ThreadingHTTPServer((args.host, args.port), make_handler(pipeline))
    print(f"[kokoro] serving on http://{args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
