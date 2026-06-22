// Plays raw PCM (from the local TTS provider) via the Web Audio API. Used for
// read-aloud when a real TTS provider is available; otherwise the app falls back
// to the system speech synthesis in speech.ts. Pause/resume suspend the shared
// AudioContext so the read-aloud queue naturally waits.

let ctx: AudioContext | null = null;
let currentSource: AudioBufferSourceNode | null = null;

function context(): AudioContext {
  ctx ??= new AudioContext();
  return ctx;
}

/** Decode base64 16-bit little-endian mono PCM into Web-Audio float samples.
 *  Cheap compared to parsing a JSON number array — the IPC sends this per
 *  sentence during read-aloud. */
export function decodePcm(b64: string): Float32Array {
  if (!b64) return new Float32Array(0);
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const i16 = new Int16Array(bytes.buffer, 0, bytes.length >> 1);
  const out = new Float32Array(i16.length);
  for (let i = 0; i < i16.length; i++) out[i] = i16[i] / 32768;
  return out;
}

export function playSamples(samples: Float32Array, sampleRate: number, onEnd?: () => void): void {
  stopPlayback();
  if (samples.length === 0) {
    onEnd?.();
    return;
  }
  const audioCtx = context();
  const buffer = audioCtx.createBuffer(1, samples.length, sampleRate);
  buffer.copyToChannel(samples, 0);
  const source = audioCtx.createBufferSource();
  source.buffer = buffer;
  source.connect(audioCtx.destination);
  source.onended = () => {
    if (currentSource === source) currentSource = null;
    onEnd?.();
  };
  source.start();
  currentSource = source;
  void audioCtx.resume(); // also un-suspends after a pause
}

export function stopPlayback(): void {
  if (currentSource) {
    currentSource.onended = null;
    try {
      currentSource.stop();
    } catch {
      // already stopped
    }
    currentSource = null;
  }
}

/** Pause playback (and the queue, which awaits the current segment's end). */
export function pausePlayback(): void {
  void ctx?.suspend();
}

export function resumePlayback(): void {
  void ctx?.resume();
}
