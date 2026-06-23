// One status/progress bar for every long-running activity (dictation, format,
// read-aloud prepare/speak, voice loading). Replaces five near-identical inline
// `.dictation-bar` blocks in App.tsx; the markup + classes are unchanged so it
// looks identical, just deduplicated.

import type { ReactNode } from "react";

interface ActivityAction {
  label: string;
  onClick: () => void;
}

interface ActivityBarProps {
  label: ReactNode;
  /** Mic level 0..1; renders the level meter when provided. */
  meter?: number;
  /** Trailing buttons (cancel / pause / stop). */
  actions?: ActivityAction[];
}

export function ActivityBar({ label, meter, actions }: ActivityBarProps) {
  return (
    <div className="dictation-bar" role="status" aria-live="polite">
      <span className="dictation-bar__dot" />
      <span className="dictation-bar__label">{label}</span>
      {meter !== undefined && (
        <span className="dictation-bar__meter">
          <span
            className="dictation-bar__level"
            style={{ width: `${Math.round(Math.min(1, meter) * 100)}%` }}
          />
        </span>
      )}
      {actions?.map((a) => (
        <button key={a.label} className="tts-reset" onClick={a.onClick}>
          {a.label}
        </button>
      ))}
    </div>
  );
}
