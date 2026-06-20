import { DictateIcon, FormatIcon, OcrIcon, ReadIcon, TrashIcon } from "./icons";

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
}

/**
 * The four core actions plus delete. Dictate toggles microphone capture; the
 * audio is transcribed locally by Whisper (PR 5). OCR, Format and Read are wired.
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
}: Props) {
  return (
    <div className="actionbar">
      <button
        className={`action-btn action-btn--primary${dictating ? " action-btn--recording" : ""}`}
        onClick={onDictate}
        aria-pressed={dictating}
        title={dictating ? "Stop dictation" : "Dictate into this note"}
      >
        <DictateIcon size={14} />
        {dictating ? "Stop" : "Dictate"}
      </button>
      <button className="action-btn" onClick={onOcr} title="OCR an image into this note">
        <OcrIcon size={14} />
        OCR
      </button>
      <button
        className="action-btn"
        onClick={onFormat}
        disabled={formatting}
        title="Format the selection, or the whole note"
      >
        <FormatIcon size={14} />
        {formatting ? "Formatting…" : "Format"}
      </button>
      <button
        className="action-btn"
        onClick={onRead}
        title="Read the selection or note aloud"
      >
        <ReadIcon size={14} />
        {reading ? "Stop" : "Read"}
      </button>
      <button className="action-btn" onClick={onExport} title="Export this note as Markdown">
        Export
      </button>
      <button className="action-btn" onClick={onHistory} title="Show this note's event history">
        History
      </button>
      <span className="actionbar__badge">MD</span>
      <button className="icon-btn" onClick={onDelete} title="Delete note" aria-label="Delete note">
        <TrashIcon size={15} />
      </button>
    </div>
  );
}
