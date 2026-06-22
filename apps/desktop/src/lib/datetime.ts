// Human-relative timestamps for the sidebar / history, localized via i18n.

import type { I18n } from "./i18n";

/** Relative time ("gerade eben", "vor 5 Min", "gestern", "vor 3 Tagen") for a
 *  stored RFC-3339 timestamp. Falls back to the locale date for anything older
 *  than a week. */
export function relativeTime(iso: string, t: I18n["t"]): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const min = Math.floor((Date.now() - then) / 60_000);
  if (min < 1) return t("time.justNow");
  if (min < 60) return t("time.minutesAgo", { count: min });
  const hours = Math.floor(min / 60);
  if (hours < 24) return t("time.hoursAgo", { count: hours });
  const days = Math.floor(hours / 24);
  if (days === 1) return t("time.yesterday");
  if (days < 7) return t("time.daysAgo", { count: days });
  return new Date(iso).toLocaleDateString();
}

/** Whole days remaining until a note trashed at `deletedAt` is purged, given a
 *  `retentionDays` window. Clamped to ≥ 0. */
export function daysUntilPurge(deletedAt: string, retentionDays = TRASH_RETENTION_DAYS): number {
  const deleted = new Date(deletedAt).getTime();
  if (Number.isNaN(deleted)) return retentionDays;
  const elapsedDays = (Date.now() - deleted) / 86_400_000;
  return Math.max(0, Math.ceil(retentionDays - elapsedDays));
}

/** How long trashed notes are kept before the purge cleanup removes them. */
export const TRASH_RETENTION_DAYS = 30;

/** The RFC-3339 cutoff for `purgeTrash`: notes trashed before now − retention. */
export function purgeCutoff(retentionDays = TRASH_RETENTION_DAYS): string {
  return new Date(Date.now() - retentionDays * 86_400_000).toISOString();
}
