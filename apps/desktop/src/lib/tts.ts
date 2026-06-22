// Zonos read-aloud emotion presets. Zonos conditions synthesis on an 8-value
// emotion vector [happiness, sadness, disgust, fear, surprise, anger, other,
// neutral] — each ~a probability, summing to ~1. These presets map a friendly
// mood label to a plausible vector; "neutral" carries no vector, so Zonos falls
// back to its own balanced default (same as before this feature existed). Only
// Zonos reads emotion; Piper/XTTS ignore it.

export interface ZonosEmotion {
  /** Stable key persisted in the tuning + sent over IPC as the preset id. */
  key: string;
  label: string;
  /** Leading glyph for the picker (matches the 🗣 voice style). */
  icon: string;
  /** The 8-value Zonos emotion vector, or undefined to leave Zonos' default. */
  vector?: number[];
}

// The full set of Zonos emotion dimensions (the six Ekman emotions + neutral),
// each preset making its target emotion dominant. Vector order is
// [happiness, sadness, disgust, fear, surprise, anger, other, neutral] and each
// sums to ~1. "neutral" carries no vector (Zonos uses its own default). Two
// softened blends ("ruhig"/"lebhaft") round out the read-aloud-friendly moods.
export const ZONOS_EMOTIONS: ZonosEmotion[] = [
  { key: "neutral", label: "Neutral", icon: "😐" },
  { key: "happy", label: "Fröhlich", icon: "😊", vector: [0.8, 0.02, 0.02, 0.02, 0.04, 0.02, 0.04, 0.04] },
  { key: "lively", label: "Lebhaft", icon: "✨", vector: [0.45, 0.02, 0.02, 0.03, 0.35, 0.03, 0.05, 0.05] },
  { key: "surprised", label: "Überrascht", icon: "😲", vector: [0.1, 0.02, 0.02, 0.04, 0.7, 0.02, 0.04, 0.06] },
  { key: "calm", label: "Ruhig", icon: "🧘", vector: [0.15, 0.08, 0.02, 0.02, 0.02, 0.02, 0.19, 0.5] },
  { key: "sad", label: "Traurig", icon: "😢", vector: [0.02, 0.8, 0.02, 0.04, 0.02, 0.02, 0.04, 0.04] },
  { key: "fearful", label: "Ängstlich", icon: "😨", vector: [0.02, 0.04, 0.02, 0.8, 0.04, 0.02, 0.02, 0.04] },
  { key: "angry", label: "Wütend", icon: "😠", vector: [0.02, 0.04, 0.04, 0.02, 0.02, 0.8, 0.02, 0.04] },
  { key: "disgust", label: "Angewidert", icon: "🤢", vector: [0.02, 0.04, 0.8, 0.02, 0.02, 0.04, 0.02, 0.04] },
];

/** The emotion vector for a preset key, or undefined for "neutral"/unknown — in
 *  which case Zonos uses its own default vector. */
export function emotionVector(key: string | undefined): number[] | undefined {
  return ZONOS_EMOTIONS.find((e) => e.key === key)?.vector;
}
