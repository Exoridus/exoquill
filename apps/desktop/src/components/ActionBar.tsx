import { DictateIcon, FormatIcon, OcrIcon, ReadIcon, TrashIcon } from "./icons";

const SOON = "Coming in a later milestone";

interface Props {
  onDelete: () => void;
}

/**
 * The four core actions plus delete. The capture/AI actions are visible but
 * disabled until their backends land (OCR PR 3, Format PR 4, Dictate PR 5,
 * Read PR 6).
 */
export function ActionBar({ onDelete }: Props) {
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
      <button className="action-btn" disabled title={SOON}>
        <FormatIcon size={14} />
        Format
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
