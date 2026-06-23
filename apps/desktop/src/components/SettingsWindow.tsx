// The single Settings window (design "Bereich 3"). Replaces the standalone
// ModelManager + ReadAloudSettings overlays: a left tab rail and a scrollable
// body, with every section reachable from one place. Each tab is its own
// self-contained component under ./settings/.

import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { useI18n } from "../lib/i18n";
import type { Lang } from "../lib/i18n";
import type { ThemeMode } from "../hooks/useTheme";
import type {
  CaptureSource,
  CatalogItem,
  DictationOpts,
  EditorPrefs,
  ModelInfo,
  ModelProgress,
  TtsTuning,
} from "../lib/types";
import { ModelsTab } from "./settings/ModelsTab";
import { ReadAloudTab } from "./settings/ReadAloudTab";
import { DictationTab } from "./settings/DictationTab";
import { AppearanceTab } from "./settings/AppearanceTab";
import { AboutTab } from "./settings/AboutTab";

export type SettingsTab = "models" | "readaloud" | "dictation" | "appearance" | "about";

export interface SettingsWindowProps {
  /** Which tab to open on (the entry point decides). Defaults to "models". */
  initialTab?: SettingsTab;
  onClose: () => void;

  // --- Models tab ---
  catalog: CatalogItem[];
  providers: ModelInfo[];
  modelProgress: Record<string, ModelProgress>;
  modelBusy: string | null;
  onInstallModel: (item: CatalogItem) => void;
  onDeleteModel: (item: CatalogItem) => void;

  // --- Read-aloud tab ---
  backendLabel: string;
  isPiper: boolean;
  isZonos: boolean;
  tuning: TtsTuning;
  onTuningChange: (tuning: TtsTuning) => void;
  speechPrep: boolean;
  onSpeechPrepChange: (value: boolean) => void;
  onPreview: () => void;
  previewing: boolean;

  // --- Dictation tab ---
  sources: CaptureSource[];
  sourceName: string | null;
  onSourceChange: (name: string | null) => void;
  dictationOpts: DictationOpts;
  onDictationOptsChange: (opts: DictationOpts) => void;
  dictationLanguage: string;
  onDictationLanguageChange: (mode: string) => void;

  // --- Appearance tab ---
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
  lang: Lang;
  onLangChange: (lang: Lang) => void;
  editorPrefs: EditorPrefs;
  onEditorPrefsChange: (prefs: EditorPrefs) => void;

  // --- About tab ---
  version: string;
}

const ICONS: Record<SettingsTab, ReactElement> = {
  models: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M8 1.5 14 5v6L8 14.5 2 11V5z" />
      <path d="M2 5l6 3 6-3M8 8v6.5" />
    </svg>
  ),
  readaloud: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M3 6v4h2l3 2.5v-9L5 6z" />
      <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12.5 3.5a6 6 0 0 1 0 9" />
    </svg>
  ),
  dictation: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="6" y="1.5" width="4" height="8" rx="2" />
      <path d="M3.5 7a4.5 4.5 0 0 0 9 0M8 11.5V14M5.5 14h5" />
    </svg>
  ),
  appearance: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 1.8V8l4.2 2.4" />
    </svg>
  ),
  about: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 7.2v4M8 5.2v.01" strokeLinecap="round" />
    </svg>
  ),
};

export function SettingsWindow(props: SettingsWindowProps) {
  const { t } = useI18n();
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? "models");

  // Escape closes from anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props]);

  const tabs: SettingsTab[] = ["models", "readaloud", "dictation", "appearance", "about"];

  return (
    <div className="settings-backdrop" onClick={props.onClose}>
      <div
        className="settings-window"
        role="dialog"
        aria-modal="true"
        aria-label={t("settings.title")}
        onClick={(e) => e.stopPropagation()}
      >
        <nav className="settings-window__rail" role="tablist" aria-orientation="vertical">
          <span className="settings-window__brand">{t("settings.title")}</span>
          {tabs.map((id) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              className={`settings-tab${tab === id ? " settings-tab--active" : ""}`}
              onClick={() => setTab(id)}
            >
              <span className="settings-tab__icon">{ICONS[id]}</span>
              {t(`settings.tab.${id}`)}
            </button>
          ))}
        </nav>

        <div className="settings-window__main">
          <header className="settings-window__head">
            <span className="settings-window__title">{t(`settings.tab.${tab}`)}</span>
            <button className="icon-btn" onClick={props.onClose} aria-label={t("common.close")}>
              ×
            </button>
          </header>
          <div className="settings-window__body" role="tabpanel">
            {tab === "models" && (
              <ModelsTab
                items={props.catalog}
                providers={props.providers}
                progress={props.modelProgress}
                busyId={props.modelBusy}
                onInstall={props.onInstallModel}
                onDelete={props.onDeleteModel}
              />
            )}
            {tab === "readaloud" && (
              <ReadAloudTab
                backendLabel={props.backendLabel}
                isPiper={props.isPiper}
                isZonos={props.isZonos}
                tuning={props.tuning}
                onChange={props.onTuningChange}
                speechPrep={props.speechPrep}
                onSpeechPrepChange={props.onSpeechPrepChange}
                onPreview={props.onPreview}
                previewing={props.previewing}
              />
            )}
            {tab === "dictation" && (
              <DictationTab
                sources={props.sources}
                sourceName={props.sourceName}
                onSourceChange={props.onSourceChange}
                opts={props.dictationOpts}
                onOptsChange={props.onDictationOptsChange}
                languageMode={props.dictationLanguage}
                onLanguageModeChange={props.onDictationLanguageChange}
              />
            )}
            {tab === "appearance" && (
              <AppearanceTab
                theme={props.themeMode}
                onThemeChange={props.onThemeModeChange}
                lang={props.lang}
                onLangChange={props.onLangChange}
                prefs={props.editorPrefs}
                onPrefsChange={props.onEditorPrefsChange}
              />
            )}
            {tab === "about" && <AboutTab version={props.version} providers={props.providers} />}
          </div>
        </div>
      </div>
    </div>
  );
}
