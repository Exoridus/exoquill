import { useI18n } from "../../lib/i18n";
import type { CaptureSource, DictationOpts } from "../../lib/types";

interface DictationTabProps {
  sources: CaptureSource[];
  sourceName: string | null;
  onSourceChange: (name: string | null) => void;
  opts: DictationOpts;
  onOptsChange: (opts: DictationOpts) => void;
  languageMode: string;
  onLanguageModeChange: (mode: string) => void;
}

export function DictationTab({
  sources,
  sourceName,
  onSourceChange,
  opts,
  onOptsChange,
  languageMode,
  onLanguageModeChange,
}: DictationTabProps) {
  const { t } = useI18n();

  return (
    <>
      {/* ---- Capture source ---- */}
      <div className="settings-section">
        <h3 className="settings-section__title">
          {t("settings.dictation.sourceTitle")}
        </h3>
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">
              {t("settings.dictation.source")}
            </span>
          </div>
          <div className="settings-field__control">
            <select
              className="settings-select"
              value={sourceName ?? ""}
              onChange={(e) => onSourceChange(e.target.value || null)}
            >
              <option value="">{t("dictation.defaultMic")}</option>
              {sources.map((src) => (
                <option key={src.name} value={src.name}>
                  {src.name}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      {/* ---- Dictation language ---- */}
      <div className="settings-section">
        <h3 className="settings-section__title">
          {t("settings.dictation.languageTitle")}
        </h3>
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">
              {t("settings.dictation.language")}
            </span>
          </div>
          <div className="settings-field__control">
            <select
              className="settings-select"
              value={languageMode}
              onChange={(e) => onLanguageModeChange(e.target.value)}
            >
              <option value="auto">{t("settings.dictation.langAuto")}</option>
              <option value="de">{t("settings.dictation.langDe")}</option>
              <option value="en">{t("settings.dictation.langEn")}</option>
            </select>
          </div>
        </div>
      </div>

      {/* ---- Detection & level ---- */}
      <div className="settings-section">
        <h3 className="settings-section__title">
          {t("settings.dictation.vadTitle")}
        </h3>

        {/* Silero VAD */}
        <div className="settings-row">
          <div className="settings-field">
            <label className="settings-field__label">
              {t("settings.dictation.useSilero")}
            </label>
            <span className="settings-field__hint">
              {t("settings.dictation.useSileroHint")}
            </span>
          </div>
          <div className="settings-field__control">
            <input
              type="checkbox"
              checked={opts.useSilero}
              onChange={(e) =>
                onOptsChange({ ...opts, useSilero: e.target.checked })
              }
            />
          </div>
        </div>

        {/* Auto gain */}
        <div className="settings-row">
          <div className="settings-field">
            <label className="settings-field__label">
              {t("settings.dictation.autoGain")}
            </label>
            <span className="settings-field__hint">
              {t("settings.dictation.autoGainHint")}
            </span>
          </div>
          <div className="settings-field__control">
            <input
              type="checkbox"
              checked={opts.autoGain}
              onChange={(e) =>
                onOptsChange({ ...opts, autoGain: e.target.checked })
              }
            />
          </div>
        </div>

        {/* Manual gain */}
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">
              {t("settings.dictation.gain")}
            </span>
          </div>
          <div className="settings-field__control">
            <input
              type="range"
              className="settings-range"
              min={0.5}
              max={3}
              step={0.1}
              value={opts.gain}
              disabled={opts.autoGain}
              onChange={(e) =>
                onOptsChange({ ...opts, gain: Number(e.target.value) })
              }
            />
            <span className="settings-value">{opts.gain.toFixed(1)}×</span>
          </div>
        </div>
      </div>
    </>
  );
}
