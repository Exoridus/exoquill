import { DictateIcon, FormatIcon, OcrIcon, ReadIcon, TrashIcon } from "./icons";

const SOON = "Coming in a later milestone";

interface Props {
  onFormat: () => void;
  onDelete: () => void;
  formatting: boolean;
}

/**
 * The four core actions plus delete. Format runs through the job queue (mock
 * provider for now); Dictate/OCR/Read are disabled until their backends land
 * (OCR PR 3, Dictate PR 5, Read PR 6).
 */
export function ActionBar({ onFormat, onDelete, formatting }: Props) {
  return (
    <div className="actionbar">
      <button className="action-btn action-btn--primary" disabled title={SOON}>
        <DictateIcon size={14} />
        Dictate
      </button>
      <button className="action-btn" disabled title={SOON}>
        <OcrIcon size={14} />
        OCR
      </button>
      <button
        className="action-btn"
        onClick={onFormat}
        disabled={formatting}
        title="Quick-format this note (mock provider)"
      >
        <FormatIcon size={14} />
        {formatting ? "Formatting…" : "Format"}
      </button>
      <button className="action-btn" disabled title={SOON}>
        <ReadIcon size={14} />
        Read
      </button>
      <span className="actionbar__badge">MD</span>
      <button className="icon-btn" onClick={onDelete} title="Delete note" aria-label="Delete note">
        <TrashIcon size={15} />
      </button>
    </div>
  );
}
