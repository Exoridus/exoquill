// Plays raw PCM samples (from the local Piper TTS provider) via the Web Audio
// API. Used for read-aloud when a real TTS provider is available; otherwise the
// app falls back to the system speech synthesis in speech.ts.

let ctx: AudioContext | null = null;
let currentSource: AudioBufferSourceNode | null = null;

function context(): AudioContext {
  ctx ??= new AudioContext();
  return ctx;
}

export function playSamples(samples: number[], sampleRate: number, onEnd?: () => void): void {
  stopPlayback();
  if (samples.length === 0) {
    onEnd?.();
    return;
  }
  const audioCtx = context();
  const buffer = audioCtx.createBuffer(1, samples.length, sampleRate);
  buffer.copyToChannel(Float32Array.from(samples), 0);
  const source = audioCtx.createBufferSource();
  source.buffer = buffer;
  source.connect(audioCtx.destination);
  source.onended = () => {
    if (currentSource === source) currentSource = null;
    onEnd?.();
  };
  source.start();
  currentSource = source;
  void audioCtx.resume();
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
