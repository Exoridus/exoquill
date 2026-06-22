// Edit-history overlay (design "Bereich 1", wireframe F): a version timeline on
// the left (current state + stored snapshots with op badges and +x/−y word
// counts) and a word-level diff of the selected version against the current
// content on the right, with a non-destructive "restore this version" action.

import { useMemo, useState } from "react";

import { relativeTime } from "../lib/datetime";
import { diffStats, diffWords } from "../lib/diff";
import { useI18n } from "../lib/i18n";
import type { TranslationKey } from "../lib/i18n";
import type { NoteVersion } from "../lib/types";
import { ClockIcon, HistoryIcon, RestoreIcon } from "./icons";

interface Props {
  versions: NoteVersion[];
  currentContent: string;
  noteTitle: string;
  onRestore: (versionId: string) => void;
  onClose: () => void;
}

/** The i18n key for a version's operation badge. */
function opKey(v: NoteVersion): TranslationKey {
  if (v.source === "manual") return "history.op.manual";
  switch (v.op) {
    case "format":
      return "history.op.format";
    case "ocr":
      return "history.op.ocr";
    case "dictation":
      return "history.op.dictation";
    case "restore":
      return "history.op.restore";
    default:
      return "history.op.snapshot";
  }
}

export function HistoryOverlay({ versions, currentContent, noteTitle, onRestore, onClose }: Props) {
  const { t } = useI18n();
  // The selected version (defaults to the most recent). `null` = the current
  // state node is selected → nothing to diff/restore.
  const [selectedId, setSelectedId] = useState<string | null>(versions[0]?.id ?? null);
  const selected = versions.find((v) => v.id === selectedId) ?? null;

  const segments = useMemo(
    () => (selected ? diffWords(selected.contentMarkdown, currentContent) : []),
    [selected, currentContent],
  );

  return (
    <div className="history-backdrop" onClick={onClose}>
      <div
        className="diff-overlay"
        role="dialog"
        aria-label={t("history.title")}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Timeline */}
        <div className="diff-timeline">
          <div className="diff-timeline__head">
            <div className="diff-timeline__title">
              <HistoryIcon size={14} />
              {t("history.title")}
            </div>
            <div className="diff-timeline__sub">
              {t("history.changesOnly")} · {t("history.versions", { count: versions.length })}
            </div>
          </div>
          <div className="diff-timeline__list">
            <button
              className={`diff-node${selectedId === null ? " active" : ""}`}
              onClick={() => setSelectedId(null)}
            >
              <span className="diff-node__rail">
                <span className="diff-node__dot diff-node__dot--current" />
                {versions.length > 0 && <span className="diff-node__line" />}
              </span>
              <span className="diff-node__body">
                <span className="diff-node__current">{t("history.current")}</span>
                <span className="diff-node__time">{t("time.justNow")}</span>
              </span>
            </button>
            {versions.map((v, i) => {
              const prev = versions[i + 1]?.contentMarkdown ?? "";
              const { added, removed } = diffStats(prev, v.contentMarkdown);
              const isLast = i === versions.length - 1;
              return (
                <button
                  key={v.id}
                  className={`diff-node${selectedId === v.id ? " active" : ""}`}
                  onClick={() => setSelectedId(v.id)}
                >
                  <span className="diff-node__rail">
                    <span className="diff-node__dot" />
                    {!isLast && <span className="diff-node__line" />}
                  </span>
                  <span className="diff-node__body">
                    <span className={`op-badge op-badge--${v.source === "manual" ? "manual" : "op"}`}>
                      {t(opKey(v))}
                    </span>
                    <span className="diff-node__stats">
                      +{added} / −{removed} {t("history.words")}
                    </span>
                    <span className="diff-node__time">
                      {relativeTime(v.createdAt, t)}
                      {v.providerId ? ` · ${v.providerId}` : ""}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Diff view */}
        <div className="diff-view">
          <div className="diff-view__head">
            <span className="diff-view__compare">{t("history.compare")}</span>
            <span className="diff-view__versions">
              <span className="muted">{t("history.thisVersion")}</span>
              <span className="diff-view__arrow">→</span>
              <span className="accent">{t("history.currentState")}</span>
            </span>
            <span className="diff-view__readonly">{t("history.readOnly")}</span>
          </div>
          <div className="diff-view__body">
            <div className="diff-view__title">{noteTitle}</div>
            {selected ? (
              <p className="diff-text">
                {segments.map((seg, i) => (
                  <span key={i} className={`diff-seg diff-seg--${seg.type}`}>
                    {seg.text}
                  </span>
                ))}
              </p>
            ) : (
              <p className="diff-view__hint">{t("history.selectVersion")}</p>
            )}
          </div>
          <div className="diff-view__actions">
            <button
              className="diff-restore"
              disabled={!selected}
              onClick={() => selected && onRestore(selected.id)}
            >
              <RestoreIcon size={13} />
              {t("history.restoreVersion")}
            </button>
            <span className="diff-view__resthint">
              <ClockIcon size={11} />
              {t("history.restoreHint")}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
