import { useEffect } from "react";
import type { ReactElement } from "react";

import "../styles/shortcuts.css";

export interface ShortcutItem {
  keys: string;
  label: string;
}

export interface ShortcutGroup {
  title: string;
  items: ShortcutItem[];
}

interface ShortcutsOverlayProps {
  open: boolean;
  onClose: () => void;
  title: string;
  groups: ShortcutGroup[];
}

export function ShortcutsOverlay({ open, onClose, title, groups }: ShortcutsOverlayProps): ReactElement | null {
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="shortcuts-backdrop" onClick={onClose}>
      <div
        className="shortcuts-panel"
        role="dialog"
        aria-modal
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="shortcuts-panel__head">
          <span>{title}</span>
          <button className="icon-btn" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="shortcuts-grid">
          {groups.map((group) => (
            <div key={group.title} className="shortcuts-group">
              <div className="shortcuts-group__title">{group.title}</div>
              {group.items.map((item) => {
                const tokens = item.keys.split("+").map((t) => t.trim());
                return (
                  <div key={item.keys + item.label} className="shortcuts-row">
                    <span className="shortcuts-row__label">{item.label}</span>
                    <span className="shortcuts-row__keys">
                      {tokens.map((token, i) => (
                        <kbd key={i}>{token}</kbd>
                      ))}
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
