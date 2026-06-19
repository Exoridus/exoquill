// Microphone dictation: captures mic audio, segments it into utterances with a
// simple energy VAD, resamples each utterance to 16 kHz and sends it to the
// local Whisper provider for transcription. The heavy real-time loop lives in
// the webview — mirroring how TTS playback lives in audio.ts — and only model
// inference crosses into the Rust sidecar (decisions D8).

import { transcribe } from "./api";

const TARGET_RATE = 16_000; // whisper.cpp input rate
const SILENCE_HANGOVER_MS = 700; // trailing silence that ends a segment
const PREROLL_MS = 250; // audio kept before speech onset so we don't clip it
const MIN_SEGMENT_MS = 350; // drop blips shorter than this (clicks, coughs)
const MAX_SEGMENT_MS = 20_000; // force-flush long speech to bound memory/latency
const SPEECH_RMS = 0.012; // normalized RMS above which a frame counts as speech
const BUFFER_SIZE = 4096; // ScriptProcessor frame size

export interface DictationHandlers {
  /** A finalized transcript segment, ready to insert into the note. */
  onText: (text: string) => void;
  /** Live input level in `[0, 1]` for an optional meter. */
  onLevel?: (level: number) => void;
  /** A non-fatal error (e.g. a failed transcription). */
  onError?: (message: string) => void;
  languageMode?: string;
  customTerms?: string[];
  /** `deviceId` of the chosen microphone; omitted uses the system default. */
  deviceId?: string;
}

export interface DictationSession {
  /** Stop capture, release the mic and flush any in-progress utterance. */
  stop: () => Promise<void>;
}

function rms(frame: Float32Array): number {
  let sum = 0;
  for (let i = 0; i < frame.length; i += 1) sum += frame[i] * frame[i];
  return Math.sqrt(sum / frame.length);
}

function merge(frames: Float32Array[]): Float32Array {
  const total = frames.reduce((n, f) => n + f.length, 0);
  const out = new Float32Array(total);
  let offset = 0;
  for (const frame of frames) {
    out.set(frame, offset);
    offset += frame.length;
  }
  return out;
}

/** Resample mono `samples` from `rate` to 16 kHz via an OfflineAudioContext
 *  (anti-aliased). Returns the input untouched when already at the target. */
async function resampleTo16k(samples: Float32Array, rate: number): Promise<Float32Array> {
  if (rate === TARGET_RATE || samples.length === 0) return samples;
  const frames = Math.max(1, Math.round((samples.length * TARGET_RATE) / rate));
  const offline = new OfflineAudioContext(1, frames, TARGET_RATE);
  const buffer = offline.createBuffer(1, samples.length, rate);
  buffer.copyToChannel(samples, 0);
  const source = offline.createBufferSource();
  source.buffer = buffer;
  source.connect(offline.destination);
  source.start();
  const rendered = await offline.startRendering();
  return rendered.getChannelData(0).slice();
}

/** Lists available microphones. Labels are only populated once mic permission
 *  has been granted (i.e. after the first `startDictation`). */
export async function listMicrophones(): Promise<MediaDeviceInfo[]> {
  if (!navigator.mediaDevices?.enumerateDevices) return [];
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((d) => d.kind === "audioinput");
}

export async function startDictation(handlers: DictationHandlers): Promise<DictationSession> {
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      deviceId: handlers.deviceId ? { exact: handlers.deviceId } : undefined,
      channelCount: 1,
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    },
  });

  const ctx = new AudioContext();
  await ctx.resume();
  const rate = ctx.sampleRate;
  const source = ctx.createMediaStreamSource(stream);
  const processor = ctx.createScriptProcessor(BUFFER_SIZE, 1, 1);
  // A ScriptProcessorNode only runs while connected to a destination, but we
  // must not echo the mic to the speakers — route it through a muted gain node.
  const mute = ctx.createGain();
  mute.gain.value = 0;

  const prerollLen = Math.floor((PREROLL_MS / 1000) * rate);
  let preroll = new Float32Array(0); // rolling pre-speech audio
  let segment: Float32Array[] = [];
  let speaking = false;
  let segmentMs = 0;
  let silenceMs = 0;
  let stopped = false;

  const flush = (frames: Float32Array[]): void => {
    const merged = merge(frames);
    if ((merged.length / rate) * 1000 < MIN_SEGMENT_MS) return;
    void (async () => {
      try {
        const resampled = await resampleTo16k(merged, rate);
        const text = await transcribe(
          Array.from(resampled),
          TARGET_RATE,
          handlers.languageMode,
          handlers.customTerms,
        );
        const trimmed = text.trim();
        if (trimmed) handlers.onText(trimmed);
      } catch (err) {
        handlers.onError?.(String(err));
      }
    })();
  };

  processor.onaudioprocess = (event) => {
    if (stopped) return;
    const input = event.inputBuffer.getChannelData(0);
    const frame = input.slice(); // copy: the input buffer is reused per callback
    const frameMs = (frame.length / rate) * 1000;
    const level = rms(frame);
    handlers.onLevel?.(Math.min(1, level * 4));

    if (level >= SPEECH_RMS) {
      if (!speaking) {
        speaking = true;
        segment = preroll.length ? [preroll] : [];
        segmentMs = (preroll.length / rate) * 1000;
      }
      segment.push(frame);
      segmentMs += frameMs;
      silenceMs = 0;
    } else if (speaking) {
      segment.push(frame);
      segmentMs += frameMs;
      silenceMs += frameMs;
    } else {
      // Idle: keep the most recent PREROLL_MS so a segment includes its onset.
      const next = merge([preroll, frame]);
      preroll = next.length > prerollLen ? next.slice(next.length - prerollLen) : next;
    }

    if (speaking && (silenceMs >= SILENCE_HANGOVER_MS || segmentMs >= MAX_SEGMENT_MS)) {
      const finished = segment;
      speaking = false;
      segment = [];
      segmentMs = 0;
      silenceMs = 0;
      preroll = new Float32Array(0);
      flush(finished);
    }
  };

  source.connect(processor);
  processor.connect(mute);
  mute.connect(ctx.destination);

  return {
    stop: async () => {
      stopped = true;
      if (speaking && segment.length) flush(segment); // flush trailing utterance
      processor.disconnect();
      mute.disconnect();
      source.disconnect();
      stream.getTracks().forEach((track) => track.stop());
      await ctx.close();
    },
  };
}
