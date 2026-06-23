import { useI18n } from "../../lib/i18n";
import type { Lang } from "../../lib/i18n";
import type { ThemeMode } from "../../hooks/useTheme";
import type { EditorPrefs } from "../../lib/types";

interface AppearanceTabProps {
  theme: ThemeMode;
  onThemeChange: (mode: ThemeMode) => void;
  lang: Lang;
  onLangChange: (lang: Lang) => void;
  prefs: EditorPrefs;
  onPrefsChange: (prefs: EditorPrefs) => void;
}

export function AppearanceTab({
  theme,
  onThemeChange,
  lang,
  onLangChange,
  prefs,
  onPrefsChange,
}: AppearanceTabProps) {
  const { t } = useI18n();

  const themeOptions: { value: ThemeMode; label: string }[] = [
    { value: "light", label: t("settings.appearance.themeLight") },
    { value: "dark", label: t("settings.appearance.themeDark") },
    { value: "system", label: t("settings.appearance.themeSystem") },
  ];

  const langOptions: { value: Lang; label: string }[] = [
    { value: "de", label: "Deutsch" },
    { value: "en", label: "English" },
  ];

  return (
    <>
      {/* Theme section */}
      <div className="settings-section">
        <p className="settings-section__title">
          {t("settings.appearance.themeTitle")}
        </p>
        <div className="settings-row">
          <div className="settings-field__control">
            <div className="settings-segmented">
              {themeOptions.map(({ value, label }) => (
                <button
                  key={value}
                  type="button"
                  className={
                    "settings-segmented__option" +
                    (theme === value ? " settings-segmented__option--active" : "")
                  }
                  onClick={() => onThemeChange(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Language section */}
      <div className="settings-section">
        <p className="settings-section__title">
          {t("settings.appearance.languageTitle")}
        </p>
        <div className="settings-row">
          <div className="settings-field__control">
            <div className="settings-segmented">
              {langOptions.map(({ value, label }) => (
                <button
                  key={value}
                  type="button"
                  className={
                    "settings-segmented__option" +
                    (lang === value ? " settings-segmented__option--active" : "")
                  }
                  onClick={() => onLangChange(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Editor section */}
      <div className="settings-section">
        <p className="settings-section__title">
          {t("settings.appearance.editorTitle")}
        </p>

        {/* Font size row */}
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">
              {t("settings.appearance.fontSize")}
            </span>
          </div>
          <div className="settings-field__control">
            <input
              type="range"
              className="settings-range"
              min={0.85}
              max={1.3}
              step={0.05}
              value={prefs.fontScale}
              onChange={(e) =>
                onPrefsChange({ ...prefs, fontScale: Number(e.target.value) })
              }
            />
            <span className="settings-value">
              {Math.round(prefs.fontScale * 100)}%
            </span>
          </div>
        </div>

        {/* Content width row */}
        <div className="settings-row">
          <div className="settings-field">
            <span className="settings-field__label">
              {t("settings.appearance.contentWidth")}
            </span>
          </div>
          <div className="settings-field__control">
            <input
              type="range"
              className="settings-range"
              min={560}
              max={920}
              step={20}
              value={prefs.contentWidth}
              onChange={(e) =>
                onPrefsChange({ ...prefs, contentWidth: Number(e.target.value) })
              }
            />
            <span className="settings-value">{prefs.contentWidth}px</span>
          </div>
        </div>
      </div>
    </>
  );
}
