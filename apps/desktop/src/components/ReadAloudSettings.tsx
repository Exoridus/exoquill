// Read-aloud synthesis settings dialog. Speed applies to every backend; the
// other knobs are backend-specific and shown only for their backend: Piper gets
// expressiveness/cadence/sentence-silence, Zonos gets intonation/brightness. A
// "Probe" button synthesizes one short sentence with the current settings for
// quick A/B tuning.

import { useI18n } from "../lib/i18n";
import type { TranslationKey } from "../lib/i18n";
import type { TtsTuning } from "../lib/types";

/** Model defaults, mirrored from the Rust providers (Piper CLI defaults; Zonos
 *  sidecar defaults for intonation/brightness; "neutral" emotion). */
export const TTS_DEFAULTS: Required<TtsTuning> = {
  speed: 1.0,
  expressiveness: 0.667,
  cadence: 0.8,
  sentenceSilence: 0.2,
  intonation: 42,
  brightness: 22050,
  emotion: "neutral",
};

/** The numeric tuning knobs — the ones a slider can drive (excludes `emotion`,
 *  which is a preset key shown as a dropdown next to the voice picker). */
type SliderKey = {
  [K in keyof TtsTuning]-?: NonNullable<TtsTuning[K]> extends number ? K : never;
}[keyof TtsTuning];

interface SliderDef {
  key: SliderKey;
  labelKey: TranslationKey;
  hintKey: TranslationKey;
  min: number;
  max: number;
  step: number;
  /** Render the live value (defaults to two decimals). */
  format?: (value: number) => string;
  /** Only meaningful for Piper; hidden for the other backends. */
  piperOnly?: boolean;
  /** Only meaningful for Zonos; hidden for the other backends. */
  zonosOnly?: boolean;
}

const SLIDERS: SliderDef[] = [
  {
    key: "speed",
    labelKey: "slider.speed",
    hintKey: "slider.speed.hint",
    min: 0.5,
    max: 1.5,
    step: 0.05,
  },
  {
    key: "expressiveness",
    labelKey: "slider.expressiveness",
    hintKey: "slider.expressiveness.hint",
    min: 0,
    max: 1,
    step: 0.05,
    piperOnly: true,
  },
  {
    key: "cadence",
    labelKey: "slider.cadence",
    hintKey: "slider.cadence.hint",
    min: 0,
    max: 1.5,
    step: 0.05,
    piperOnly: true,
  },
  {
    key: "sentenceSilence",
    labelKey: "slider.sentenceSilence",
    hintKey: "slider.sentenceSilence.hint",
    min: 0,
    max: 1,
    step: 0.05,
    piperOnly: true,
  },
  {
    key: "intonation",
    labelKey: "slider.intonation",
    hintKey: "slider.intonation.hint",
    min: 0,
    max: 100,
    step: 1,
    format: (v) => v.toFixed(0),
    zonosOnly: true,
  },
  {
    key: "brightness",
    labelKey: "slider.brightness",
    hintKey: "slider.brightness.hint",
    min: 12000,
    max: 22050,
    step: 50,
    format: (v) => `${(v / 1000).toFixed(1)} kHz`,
    zonosOnly: true,
  },
];

interface Props {
  /** Label of the active backend (Piper / XTTS / Zonos), shown in the header. */
  backendLabel: string;
  /** Whether the active voice is a Piper voice (shows the Piper-only knobs). */
  isPiper: boolean;
  /** Whether the active voice is a Zonos voice (shows the Zonos-only knobs). */
  isZonos: boolean;
  tuning: TtsTuning;
  onChange: (tuning: TtsTuning) => void;
  /** Whether to run an LLM "prepare for speech" pass before reading. */
  speechPrep: boolean;
  onSpeechPrepChange: (value: boolean) => void;
  /** Synthesize + play one short test sentence with the current settings. */
  onPreview: () => void;
  /** Whether a preview is currently rendering (disables the button). */
  previewing: boolean;
  onClose: () => void;
}

export function ReadAloudSettings({
  backendLabel,
  isPiper,
  isZonos,
  tuning,
  onChange,
  speechPrep,
  onSpeechPrepChange,
  onPreview,
  previewing,
  onClose,
}: Props) {
  const { t } = useI18n();
  const sliders = SLIDERS.filter(
    (s) => (!s.piperOnly || isPiper) && (!s.zonosOnly || isZonos),
  );
  return (
    <div className="history-backdrop" onClick={onClose}>
      <div
        className="history-panel"
        role="dialog"
        aria-label={t("raSettings.aria")}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="history-panel__head">
          <span>{t("raSettings.title", { backend: backendLabel })}</span>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            ×
          </button>
        </header>
        <div className="tts-settings">
          {sliders.map((s) => {
            const value = tuning[s.key] ?? TTS_DEFAULTS[s.key];
            return (
              <label key={s.key} className="tts-setting">
                <span className="tts-setting__label">
                  {t(s.labelKey)}
                  <span className="tts-setting__value">
                    {(s.format ?? ((v) => v.toFixed(2)))(value)}
                  </span>
                </span>
                <input
                  type="range"
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  value={value}
                  onChange={(e) => onChange({ ...tuning, [s.key]: Number(e.target.value) })}
                />
                <span className="tts-setting__hint">{t(s.hintKey)}</span>
              </label>
            );
          })}
          {!isPiper && !isZonos && (
            <p className="tts-setting__hint">
              {t("raSettings.onlySpeed", { backend: backendLabel })}
            </p>
          )}
        </div>
        <label className="tts-prep">
          <input
            type="checkbox"
            checked={speechPrep}
            onChange={(e) => onSpeechPrepChange(e.target.checked)}
          />
          <span>
            {t("raSettings.speechPrep")}
            <span className="tts-setting__hint">{t("raSettings.speechPrepHint")}</span>
          </span>
        </label>
        <div className="tts-settings__actions">
          <button className="tts-reset" onClick={onPreview} disabled={previewing}>
            {previewing ? t("raSettings.previewing") : t("raSettings.preview")}
          </button>
          <button
            className="tts-reset"
            onClick={() => onChange({ ...TTS_DEFAULTS, speed: tuning.speed ?? TTS_DEFAULTS.speed })}
          >
            {t("common.reset")}
          </button>
        </div>
      </div>
    </div>
  );
}
