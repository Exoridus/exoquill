import { useI18n } from "../lib/i18n";
import type { Note } from "../lib/types";

interface Props {
  note: Note | null;
  saved: boolean;
}

function countWords(markdown: string): number {
  const text = markdown.replace(/[#*`>_\-]/g, " ").trim();
  return text ? text.split(/\s+/).length : 0;
}

export function Statusbar({ note, saved }: Props) {
  const { t } = useI18n();
  const words = note ? countWords(note.contentMarkdown) : 0;
  const readingMin = Math.max(1, Math.ceil(words / 200));
  return (
    <footer className="statusbar">
      <span>de-DE · en-US</span>
      <span>{saved ? t("status.saved") : t("status.saving")}</span>
      <span className="statusbar__accent">LOCAL · ON-DEVICE</span>
      <span className="statusbar__spacer">{t("status.words", { count: words })}</span>
      {words > 0 && <span>{t("status.readingTime", { count: readingMin })}</span>}
    </footer>
  );
}
