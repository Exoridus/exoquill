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
