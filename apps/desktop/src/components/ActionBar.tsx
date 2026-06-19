import { DictateIcon, FormatIcon, OcrIcon, ReadIcon, TrashIcon } from "./icons";

const SOON = "Dictation needs a local Whisper model (PR 5)";

interface Props {
  onOcr: () => void;
  onFormat: () => void;
  onRead: () => void;
  onDelete: () => void;
  formatting: boolean;
  reading: boolean;
}

/**
 * The four core actions plus delete. Dictate is disabled until the audio +
 * Whisper backend lands (PR 5); OCR, Format and Read are wired.
 */
export function ActionBar({ onOcr, onFormat, onRead, onDelete, formatting, reading }: Props) {
  return (
    <div className="actionbar">
      <button className="action-btn action-btn--primary" disabled title={SOON}>
        <DictateIcon size={14} />
        Dictate
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
      <span className="actionbar__badge">MD</span>
      <button className="icon-btn" onClick={onDelete} title="Delete note" aria-label="Delete note">
        <TrashIcon size={15} />
      </button>
    </div>
  );
}
