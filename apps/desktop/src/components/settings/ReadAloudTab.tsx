// Read-aloud tab for the Settings window. Ports the tuning sliders, speech-prep
// toggle, and preview/reset actions from ReadAloudSettings.tsx, rendered with
// settings-* CSS classes instead of the dialog overlay chrome.

import { useI18n } from "../../lib/i18n";
import type { TranslationKey } from "../../lib/i18n";
import type { TtsTuning } from "../../lib/types";
import { TTS_DEFAULTS } from "../ReadAloudSettings";

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
  format?: (value: number) => string;
  piperOnly?: boolean;
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

interface ReadAloudTabProps {
  backendLabel: string;
  isPiper: boolean;
  isZonos: boolean;
  tuning: TtsTuning;
  onChange: (tuning: TtsTuning) => void;
  speechPrep: boolean;
  onSpeechPrepChange: (value: boolean) => void;
  onPreview: () => void;
  previewing: boolean;
}

export function ReadAloudTab({
  backendLabel,
  isPiper,
  isZonos,
  tuning,
  onChange,
  speechPrep,
  onSpeechPrepChange,
  onPreview,
  previewing,
}: ReadAloudTabProps) {
  const { t } = useI18n();

  const sliders = SLIDERS.filter(
    (s) => (!s.piperOnly || isPiper) && (!s.zonosOnly || isZonos),
  );

  return (
    <>
      {/* Section 1: Tuning sliders */}
      <div className="settings-section">
        <p className="settings-section__title">{t("settings.readaloud.tuningTitle")}</p>
        <p className="settings-section__hint">
          {t("settings.readaloud.intro", { backend: backendLabel })}
        </p>
        {!isPiper && !isZonos && (
          <p className="settings-section__hint">
            {t("raSettings.onlySpeed", { backend: backendLabel })}
          </p>
        )}
        {sliders.map((s) => {
          const value = tuning[s.key] ?? TTS_DEFAULTS[s.key];
          const fmt = s.format ?? ((v: number) => v.toFixed(2));
          return (
            <div key={s.key} className="settings-row">
              <div className="settings-field">
                <span className="settings-field__label">{t(s.labelKey)}</span>
                <span className="settings-field__hint">{t(s.hintKey)}</span>
              </div>
              <div className="settings-field__control">
                <input
                  type="range"
                  className="settings-range"
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  value={value}
                  onChange={(e) =>
                    onChange({ ...tuning, [s.key]: Number(e.target.value) })
                  }
                />
                <span className="settings-value">{fmt(value)}</span>
              </div>
            </div>
          );
        })}
      </div>

      {/* Section 2: Speech preparation */}
      <div className="settings-section">
        <p className="settings-section__title">{t("settings.readaloud.prepTitle")}</p>
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">{t("raSettings.speechPrep")}</span>
            <span className="settings-field__hint">{t("raSettings.speechPrepHint")}</span>
          </div>
          <div className="settings-field__control">
            <input
              type="checkbox"
              checked={speechPrep}
              onChange={(e) => onSpeechPrepChange(e.target.checked)}
            />
          </div>
        </div>
      </div>

      {/* Footer actions */}
      <div className="settings-row">
        <div className="settings-field__control" style={{ gap: "8px" }}>
          <button
            className="settings-btn settings-btn--primary"
            onClick={onPreview}
            disabled={previewing}
          >
            {previewing ? t("raSettings.previewing") : t("raSettings.preview")}
          </button>
          <button
            className="settings-btn"
            onClick={() =>
              onChange({ ...TTS_DEFAULTS, speed: tuning.speed ?? TTS_DEFAULTS.speed })
            }
          >
            {t("common.reset")}
          </button>
        </div>
      </div>
    </>
  );
}
