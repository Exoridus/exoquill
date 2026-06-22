import type { Lang } from "../lib/i18n";
import { useI18n } from "../lib/i18n";
import type { Theme } from "../hooks/useTheme";
import {
  DictateIcon,
  FormatIcon,
  GlobeIcon,
  MoonIcon,
  OcrIcon,
  ReadIcon,
  SunIcon,
  TrashIcon,
} from "./icons";

interface Props {
  onDictate: () => void;
  onOcr: () => void;
  onFormat: () => void;
  onRead: () => void;
  onExport: () => void;
  onHistory: () => void;
  onDelete: () => void;
  dictating: boolean;
  formatting: boolean;
  reading: boolean;
  /** Whether a note is open; note-scoped actions are disabled without one. */
  hasNote: boolean;
  // Global controls (moved here from the former top bar).
  theme: Theme;
  onToggleTheme: () => void;
  onShowModels: () => void;
  lang: Lang;
  onToggleLang: () => void;
}

/**
 * The per-note actions on the left (dictate / OCR / format / read / export /
 * history / delete) and the global controls on the right (models, language,
 * theme). This is the app's only toolbar — the former top bar was folded in.
 */
export function ActionBar({
  onDictate,
  onOcr,
  onFormat,
  onRead,
  onExport,
  onHistory,
  onDelete,
  dictating,
  formatting,
  reading,
  hasNote,
  theme,
  onToggleTheme,
  onShowModels,
  lang,
  onToggleLang,
}: Props) {
  const { t } = useI18n();
  return (
    <div className="actionbar">
      <button
        className={`action-btn action-btn--primary${dictating ? " action-btn--recording" : ""}`}
        onClick={onDictate}
        aria-pressed={dictating}
        title={dictating ? t("action.dictate.stopTitle") : t("action.dictate.title")}
      >
        <DictateIcon size={14} />
        {dictating ? t("action.stop") : t("action.dictate")}
      </button>
      <button className="action-btn" onClick={onOcr} title={t("action.ocr.title")}>
        <OcrIcon size={14} />
        {t("action.ocr")}
      </button>
      <button
        className="action-btn"
        onClick={onFormat}
        disabled={formatting || !hasNote}
        title={t("action.format.title")}
      >
        <FormatIcon size={14} />
        {formatting ? t("action.formatting") : t("action.format")}
      </button>
      <button
        className="action-btn"
        onClick={onRead}
        disabled={!hasNote}
        title={t("action.read.title")}
      >
        <ReadIcon size={14} />
        {reading ? t("action.stop") : t("action.read")}
      </button>
      <button
        className="action-btn"
        onClick={onExport}
        disabled={!hasNote}
        title={t("action.export.title")}
      >
        {t("action.export")}
      </button>
      <button
        className="action-btn"
        onClick={onHistory}
        disabled={!hasNote}
        title={t("action.history.title")}
      >
        {t("action.history")}
      </button>

      <span className="actionbar__spacer" />

      <button className="action-btn" onClick={onShowModels} title={t("toolbar.models.title")}>
        {t("toolbar.models")}
      </button>
      <button
        className="icon-btn icon-btn--lang"
        onClick={onToggleLang}
        title={t("toolbar.lang.title")}
        aria-label={t("toolbar.lang.aria")}
      >
        <GlobeIcon size={15} />
        <span className="icon-btn__tag">{lang.toUpperCase()}</span>
      </button>
      <button
        className="icon-btn"
        onClick={onToggleTheme}
        title={t("toolbar.theme.title")}
        aria-label={t("toolbar.theme.aria")}
      >
        {theme === "dark" ? <SunIcon size={16} /> : <MoonIcon size={16} />}
      </button>
      <span className="actionbar__divider" />
      <button
        className="icon-btn"
        onClick={onDelete}
        disabled={!hasNote}
        title={t("action.delete.title")}
        aria-label={t("action.delete.title")}
      >
        <TrashIcon size={15} />
      </button>
    </div>
  );
}
