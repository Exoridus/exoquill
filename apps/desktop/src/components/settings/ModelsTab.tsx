// Polished replacement for ModelManager.tsx rendered inside the Settings window.
// Uses the settings-* CSS vocabulary from settings.css — never the old model-row classes.

import { useI18n } from "../../lib/i18n";
import type { TranslationKey } from "../../lib/i18n";
import type { CatalogItem, ModelInfo, ModelProgress } from "../../lib/types";

export interface ModelsTabProps {
  items: CatalogItem[];
  providers: ModelInfo[];
  /** Live download progress keyed by model id. */
  progress: Record<string, ModelProgress>;
  /** The id currently installing/deleting (disables buttons), or null. */
  busyId: string | null;
  onInstall: (item: CatalogItem) => void;
  onDelete: (item: CatalogItem) => void;
}

// Maps a tier string to its i18n key.
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

interface Group {
  key: TranslationKey;
  items: CatalogItem[];
}

export function ModelsTab({
  items,
  providers,
  progress,
  busyId,
  onInstall,
  onDelete,
}: ModelsTabProps) {
  const { t } = useI18n();

  // Build ordered groups; skip empty ones.
  const allGroups: Group[] = [
    {
      key: "settings.models.group.voices",
      items: items.filter((m) => m.kind === "voice"),
    },
    {
      key: "settings.models.group.runtimes",
      items: items.filter((m) => m.kind === "runtime"),
    },
    {
      key: "settings.models.group.other",
      items: items.filter((m) => m.kind !== "voice" && m.kind !== "runtime"),
    },
  ];
  const groups = allGroups.filter((g) => g.items.length > 0);

  return (
    <>
      {/* ── Catalog ── */}
      {items.length === 0 ? (
        <div className="settings-section">
          <p className="settings-section__hint">{t("settings.models.empty")}</p>
        </div>
      ) : (
        groups.map((group, groupIdx) => (
          <div key={group.key} className="settings-section">
            {groupIdx === 0 ? (
              <>
                <h3 className="settings-section__title">{t("settings.models.catalogTitle")}</h3>
                <p className="settings-section__hint">{t("settings.models.catalogHint")}</p>
              </>
            ) : null}
            <h4 className="settings-section__title">{t(group.key)}</h4>

            <div className="settings-cards">
              {group.items.map((m) => {
                const p = progress[m.id];
                const pct =
                  p && p.total ? Math.round((p.downloaded / p.total) * 100) : null;
                const busy = busyId === m.id;

                return (
                  <div key={m.id} className="settings-card">
                    {/* Head: title + actions */}
                    <div className="settings-card__head">
                      <span className="settings-card__title">{m.displayName}</span>
                      <div className="settings-card__actions">
                        {m.setup && !m.installed ? (
                          <span className="settings-card__setup">
                            {t("settings.models.setupHint")} <code>{m.setup}</code>
                          </span>
                        ) : m.installed ? (
                          <>
                            <span className="settings-card__ok">{t("models.installed")}</span>
                            {m.tier !== "bundled" && !m.setup && (
                              <button
                                className="settings-btn settings-btn--danger"
                                disabled={busy}
                                onClick={() => onDelete(m)}
                              >
                                {t("common.delete")}
                              </button>
                            )}
                          </>
                        ) : (
                          <button
                            className="settings-btn settings-btn--primary"
                            disabled={busy}
                            onClick={() => onInstall(m)}
                          >
                            {t("models.install")}
                          </button>
                        )}
                      </div>
                    </div>

                    {/* Meta badges */}
                    <div className="settings-card__meta">
                      <span className="settings-badge">{m.language}</span>
                      <span className={`settings-badge settings-badge--${m.tier}`}>
                        {TIER_KEY[m.tier] ? t(TIER_KEY[m.tier]) : m.tier}
                      </span>
                      <span className="settings-badge">{m.license}</span>
                      {!m.commercialOk && (
                        <span className="settings-badge settings-badge--nc">
                          {t("models.nonCommercial")}
                        </span>
                      )}
                      {m.installed && m.installedBytes > 0 && (
                        <span className="settings-badge">
                          {t("settings.models.size")}: {fmtBytes(m.installedBytes)}
                        </span>
                      )}
                    </div>

                    {/* Optional notes */}
                    {m.notes && (
                      <span className="settings-card__notes">{m.notes}</span>
                    )}

                    {/* Download progress bar */}
                    {busy && (
                      <>
                        <div className="settings-card__progress">
                          <div
                            className="settings-card__bar"
                            style={{ width: `${pct ?? 0}%` }}
                          />
                        </div>
                        <span className="settings-card__pct">
                          {pct != null ? `${pct}%` : t("models.loading")}
                        </span>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ))
      )}

      {/* ── Providers ── */}
      {providers.length > 0 && (
        <div className="settings-section">
          <h3 className="settings-section__title">{t("settings.models.providersTitle")}</h3>
          {providers.map((pr) => (
            <div key={pr.feature} className="settings-provider">
              <div>
                <div className="settings-provider__name">{pr.displayName}</div>
                <div className="settings-provider__feature">{pr.feature}</div>
              </div>
              <div className="settings-card__actions">
                <span
                  className={`settings-provider__status settings-provider__status--${pr.status}`}
                >
                  {pr.status}
                </span>
                <span className="settings-badge">{pr.runtimeLicense}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
