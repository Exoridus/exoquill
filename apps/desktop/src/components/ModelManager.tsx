// Model manager window: install / delete the catalog of TTS voices + runtimes,
// with per-entry license + tier badges and download progress. Restrictive
// entries (non-commercial) are gated behind a confirm in the parent handler.

import { useI18n } from "../lib/i18n";
import type { TranslationKey } from "../lib/i18n";
import type { CatalogItem, ModelInfo, ModelProgress } from "../lib/types";

interface Props {
  items: CatalogItem[];
  providers: ModelInfo[];
  /** Live download progress keyed by model id. */
  progress: Record<string, ModelProgress>;
  /** The id currently installing (disables its buttons), or null. */
  busyId: string | null;
  onInstall: (item: CatalogItem) => void;
  onDelete: (item: CatalogItem) => void;
  onClose: () => void;
}

const TIER_KEY: Record<string, TranslationKey> = {
  bundled: "models.tier.bundled",
  download: "models.tier.download",
  gated: "models.tier.gated",
};

function fmtBytes(n: number): string {
  if (!n) return "";
  const mb = n / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

export function ModelManager({
  items,
  providers,
  progress,
  busyId,
  onInstall,
  onDelete,
  onClose,
}: Props) {
  const { t } = useI18n();
  return (
    <div className="history-backdrop" onClick={onClose}>
      <div
        className="history-panel model-mgr"
        role="dialog"
        aria-label={t("models.title")}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="history-panel__head">
          <span>{t("models.title")}</span>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            ×
          </button>
        </header>

        <ul className="model-list">
          {items.map((m) => {
            const p = progress[m.id];
            const pct = p && p.total ? Math.round((p.downloaded / p.total) * 100) : null;
            const busy = busyId === m.id;
            return (
              <li key={m.id} className="model-row">
                <div className="model-row__main">
                  <span className="model-row__name">{m.displayName}</span>
                  <span className="model-row__meta">
                    <span className="model-badge">{m.language}</span>
                    <span className={`model-badge model-badge--${m.tier}`}>
                      {TIER_KEY[m.tier] ? t(TIER_KEY[m.tier]) : m.tier}
                    </span>
                    <span className="model-badge">{m.license}</span>
                    {!m.commercialOk && (
                      <span className="model-badge model-badge--nc">{t("models.nonCommercial")}</span>
                    )}
                    {m.installed && m.installedBytes > 0 && (
                      <span className="model-row__size">{fmtBytes(m.installedBytes)}</span>
                    )}
                  </span>
                  {m.notes && <span className="model-row__notes">{m.notes}</span>}
                  {busy && (
                    <span className="model-row__progress">
                      <span className="model-row__bar" style={{ width: `${pct ?? 0}%` }} />
                      <span className="model-row__pct">
                        {pct != null ? `${pct}%` : t("models.loading")}
                      </span>
                    </span>
                  )}
                </div>
                <div className="model-row__actions">
                  {m.setup && !m.installed ? (
                    <span className="model-row__hint">
                      {t("models.setup")} <code>{m.setup}</code>
                    </span>
                  ) : m.installed ? (
                    <>
                      <span className="model-row__ok">{t("models.installed")}</span>
                      {m.tier !== "bundled" && !m.setup && (
                        <button className="tts-reset" disabled={busy} onClick={() => onDelete(m)}>
                          {t("common.delete")}
                        </button>
                      )}
                    </>
                  ) : (
                    <button className="tts-reset" disabled={busy} onClick={() => onInstall(m)}>
                      {busy ? "…" : t("models.install")}
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>

        {providers.length > 0 && (
          <>
            <h4 className="model-mgr__subhead">{t("models.activeProviders")}</h4>
            <ul className="model-list">
              {providers.map((pr) => (
                <li key={pr.feature} className="model-row model-row--provider">
                  <span className="model-row__name">
                    {pr.feature}: {pr.displayName}
                  </span>
                  <span className="model-row__meta">
                    <span className={`model-info__status model-info__status--${pr.status}`}>
                      {pr.status}
                    </span>
                    <span className="model-badge">{pr.runtimeLicense}</span>
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}
