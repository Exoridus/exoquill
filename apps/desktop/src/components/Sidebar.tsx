// Notes sidebar (design "Bereich 1"): scope tabs (Active / Archived / Trash),
// a pinned group, sort, per-note hover + context-menu actions, multi-select with
// a bulk action bar, and the archive/trash views with restore / delete-forever.

import { type MouseEvent, type ReactNode, useEffect, useRef, useState } from "react";

import { useI18n } from "../lib/i18n";
import { relativeTime, daysUntilPurge } from "../lib/datetime";
import type { Note, NoteScope, NoteSort } from "../lib/types";
import {
  ArchiveIcon,
  CheckIcon,
  ChevronDownIcon,
  DuplicateIcon,
  ExportIcon,
  PinIcon,
  PlusIcon,
  RenameIcon,
  RestoreIcon,
  SearchIcon,
  TrashIcon,
} from "./icons";
import { LogoMark } from "./Logo";

interface Props {
  notes: Note[];
  scope: NoteScope;
  onScopeChange: (scope: NoteScope) => void;
  sort: NoteSort;
  onSortChange: (sort: NoteSort) => void;
  activeId: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  /** Open a note (plain click). The event carries Ctrl/⌘/Shift for selection. */
  onSelect: (id: string, e: MouseEvent) => void;
  onNewNote: () => void;
  /** Currently multi-selected note ids. Non-empty → selection mode. */
  selected: Set<string>;
  onClearSelection: () => void;
  // Per-note actions.
  onPin: (note: Note) => void;
  onRename: (note: Note) => void;
  onDuplicate: (note: Note) => void;
  onArchive: (note: Note) => void;
  onExport: (note: Note) => void;
  onTrash: (note: Note) => void;
  onRestore: (note: Note) => void;
  onDeleteForever: (note: Note) => void;
  // Bulk actions (operate on `selected`).
  onBulkPin: () => void;
  onBulkArchive: () => void;
  onBulkExport: () => void;
  onBulkTrash: () => void;
  onEmptyTrash: () => void;
}

const SCOPES: NoteScope[] = ["active", "archived", "trash"];
const SORTS: NoteSort[] = ["modified", "created", "title"];

function preview(note: Note, emptyLabel: string): string {
  const lines = note.contentMarkdown
    .split("\n")
    .map((line) => line.replace(/^#+\s*/, "").replace(/[*`_>]/g, "").trim());
  return lines.slice(1).find(Boolean) ?? lines.find(Boolean) ?? emptyLabel;
}

/** Wrap case-insensitive matches of `query` in <mark> for search highlighting. */
function highlight(text: string, query: string): ReactNode {
  const q = query.trim();
  if (!q) return text;
  const lower = text.toLowerCase();
  const ql = q.toLowerCase();
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;
  for (let found = lower.indexOf(ql); found !== -1; found = lower.indexOf(ql, i)) {
    if (found > i) out.push(text.slice(i, found));
    out.push(
      <mark key={key++} className="note-hl">
        {text.slice(found, found + q.length)}
      </mark>,
    );
    i = found + q.length;
  }
  if (i < text.length) out.push(text.slice(i));
  return out.length ? out : text;
}

export function Sidebar(props: Props) {
  const { notes, scope, sort, activeId, query, selected, onSelect } = props;
  const { t } = useI18n();
  const emptyLabel = t("note.emptyPreview");
  const selectionMode = selected.size > 0;

  // The open context menu: which note + screen position.
  const [menu, setMenu] = useState<{ note: Note; x: number; y: number } | null>(null);
  // Inline sort dropdown open state.
  const [sortOpen, setSortOpen] = useState(false);

  const openMenu = (note: Note, e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ note, x: e.clientX, y: e.clientY });
  };

  const pinned = notes.filter((n) => n.pinned);
  const rest = notes.filter((n) => !n.pinned);

  const countLabel =
    scope === "archived"
      ? t("sidebar.archivedCount", { count: notes.length })
      : scope === "trash"
        ? t("sidebar.trashCount", { count: notes.length })
        : t("sidebar.count", { count: notes.length });

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <LogoMark />
        <span className="sidebar__wordmark">exoquill</span>
      </div>

      <div className="sidebar__search">
        <SearchIcon size={13} />
        <input
          value={query}
          onChange={(e) => props.onQueryChange(e.target.value)}
          placeholder={t("sidebar.search")}
        />
      </div>

      <div className="scope-tabs" role="tablist">
        {SCOPES.map((s) => (
          <button
            key={s}
            role="tab"
            aria-selected={scope === s}
            className={`scope-tab scope-tab--${s}${scope === s ? " active" : ""}`}
            onClick={() => props.onScopeChange(s)}
          >
            {t(`scope.${s}`)}
          </button>
        ))}
      </div>

      {selectionMode ? (
        <BulkBar
          count={selected.size}
          scope={scope}
          onCancel={props.onClearSelection}
          onPin={props.onBulkPin}
          onArchive={props.onBulkArchive}
          onExport={props.onBulkExport}
          onTrash={props.onBulkTrash}
        />
      ) : (
        <div className="sidebar__meta">
          <span className="sidebar__count">{countLabel}</span>
          {scope === "trash" ? (
            notes.length > 0 && (
              <button className="sidebar__danger-link" onClick={props.onEmptyTrash}>
                {t("trash.emptyTrash")}
              </button>
            )
          ) : (
            <div className="sort-control">
              <button
                className="sort-control__btn"
                onClick={() => setSortOpen((o) => !o)}
                aria-haspopup="listbox"
                aria-expanded={sortOpen}
                aria-label={t("sort.aria")}
              >
                {t(`sort.${sort}`)}
                <ChevronDownIcon size={9} />
              </button>
              {sortOpen && (
                <>
                  <div className="popover-scrim" onClick={() => setSortOpen(false)} />
                  <div className="sort-menu" role="listbox">
                    {SORTS.map((s) => (
                      <button
                        key={s}
                        role="option"
                        aria-selected={sort === s}
                        className={`sort-menu__item${sort === s ? " active" : ""}`}
                        onClick={() => {
                          props.onSortChange(s);
                          setSortOpen(false);
                        }}
                      >
                        {t(`sort.${s}`)}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}

      <div className="note-list">
        {notes.length === 0 && (
          <p className="note-list__empty">
            {scope === "archived"
              ? t("scope.archivedEmpty")
              : scope === "trash"
                ? t("scope.trashEmpty")
                : t("scope.activeEmpty")}
          </p>
        )}

        {scope === "trash"
          ? notes.map((note) => (
              <TrashRow
                key={note.id}
                note={note}
                onRestore={() => props.onRestore(note)}
                onDeleteForever={() => props.onDeleteForever(note)}
              />
            ))
          : scope === "archived"
            ? notes.map((note) => (
                <ArchiveRow
                  key={note.id}
                  note={note}
                  onRestore={() => props.onRestore(note)}
                  onTrash={() => props.onTrash(note)}
                  onClick={(e) => onSelect(note.id, e)}
                  selected={selected.has(note.id)}
                  selectionMode={selectionMode}
                />
              ))
            : (
              <>
                {pinned.length > 0 && (
                  <>
                    <div className="note-group note-group--pin">
                      <PinIcon size={10} fill="currentColor" stroke="none" />
                      {t("group.pinned")}
                    </div>
                    {pinned.map((note) => (
                      <NoteRow
                        key={note.id}
                        note={note}
                        active={note.id === activeId}
                        selected={selected.has(note.id)}
                        selectionMode={selectionMode}
                        emptyLabel={emptyLabel}
                        query={query}
                        onClick={(e) => onSelect(note.id, e)}
                        onPin={() => props.onPin(note)}
                        onMenu={(e) => openMenu(note, e)}
                      />
                    ))}
                    {rest.length > 0 && (
                      <div className="note-group">{t("group.allNotes")}</div>
                    )}
                  </>
                )}
                {rest.map((note) => (
                  <NoteRow
                    key={note.id}
                    note={note}
                    active={note.id === activeId}
                    selected={selected.has(note.id)}
                    selectionMode={selectionMode}
                    emptyLabel={emptyLabel}
                    query={query}
                    onClick={(e) => onSelect(note.id, e)}
                    onPin={() => props.onPin(note)}
                    onMenu={(e) => openMenu(note, e)}
                  />
                ))}
              </>
            )}
      </div>

      {scope !== "trash" && (
        <div className="sidebar__footer">
          <button className="btn-primary" onClick={props.onNewNote}>
            <PlusIcon size={14} />
            {t("sidebar.newNote")}
          </button>
        </div>
      )}

      {menu && (
        <NoteContextMenu
          note={menu.note}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onPin={props.onPin}
          onRename={props.onRename}
          onDuplicate={props.onDuplicate}
          onArchive={props.onArchive}
          onExport={props.onExport}
          onTrash={props.onTrash}
        />
      )}
    </aside>
  );
}

interface RowProps {
  note: Note;
  active: boolean;
  selected: boolean;
  selectionMode: boolean;
  emptyLabel: string;
  /** Current search query, for match highlighting (empty = no search). */
  query: string;
  onClick: (e: MouseEvent) => void;
  onPin: () => void;
  onMenu: (e: MouseEvent) => void;
}

function NoteRow({
  note,
  active,
  selected,
  selectionMode,
  emptyLabel,
  query,
  onClick,
  onPin,
  onMenu,
}: RowProps) {
  return (
    <div
      className={`note-item${active && !selectionMode ? " active" : ""}${
        selected ? " selected" : ""
      }`}
      onClick={onClick}
      onContextMenu={onMenu}
    >
      {selectionMode && (
        <span className={`note-check${selected ? " checked" : ""}`}>
          {selected && <CheckIcon size={10} stroke="var(--accent-contrast)" strokeWidth={3} />}
        </span>
      )}
      <div className="note-item__body">
        <div className="note-item__title">{highlight(note.title, query)}</div>
        <div className="note-item__preview">{highlight(preview(note, emptyLabel), query)}</div>
      </div>
      {!selectionMode && (
        <div className="note-item__actions">
          <button
            className={`note-item__pin${note.pinned ? " pinned" : ""}`}
            title="Pin"
            onClick={(e) => {
              e.stopPropagation();
              onPin();
            }}
          >
            <PinIcon size={13} fill={note.pinned ? "currentColor" : "none"} />
          </button>
          <button
            className="note-item__menu"
            aria-label="Menu"
            onClick={(e) => {
              e.stopPropagation();
              onMenu(e);
            }}
          >
            <span className="note-item__dots">⋯</span>
          </button>
        </div>
      )}
      {note.pinned && selectionMode && (
        <PinIcon size={12} fill="var(--pin)" stroke="var(--pin)" />
      )}
    </div>
  );
}

function ArchiveRow({
  note,
  onRestore,
  onTrash,
  onClick,
  selected,
  selectionMode,
}: {
  note: Note;
  onRestore: () => void;
  onTrash: () => void;
  onClick: (e: MouseEvent) => void;
  selected: boolean;
  selectionMode: boolean;
}) {
  const { t } = useI18n();
  return (
    <div
      className={`archive-row${selected ? " selected" : ""}`}
      onClick={onClick}
    >
      <div className="archive-row__head">
        {selectionMode ? (
          <span className={`note-check${selected ? " checked" : ""}`}>
            {selected && <CheckIcon size={10} stroke="var(--accent-contrast)" strokeWidth={3} />}
          </span>
        ) : (
          <ArchiveIcon size={13} stroke="var(--faint, var(--text-muted))" />
        )}
        <span className="archive-row__title">{note.title}</span>
      </div>
      <div className="archive-row__meta">{t("archive.archivedAgo", { when: relativeTime(note.updatedAt, t) })}</div>
      <div className="row-actions">
        <button
          className="row-action row-action--restore"
          onClick={(e) => {
            e.stopPropagation();
            onRestore();
          }}
        >
          <RestoreIcon size={12} />
          {t("noteAction.restore")}
        </button>
        <button
          className="row-action row-action--danger-icon"
          aria-label={t("noteAction.toTrash")}
          onClick={(e) => {
            e.stopPropagation();
            onTrash();
          }}
        >
          <TrashIcon size={13} />
        </button>
      </div>
    </div>
  );
}

function TrashRow({
  note,
  onRestore,
  onDeleteForever,
}: {
  note: Note;
  onRestore: () => void;
  onDeleteForever: () => void;
}) {
  const { t } = useI18n();
  const days = note.deletedAt ? daysUntilPurge(note.deletedAt) : 0;
  return (
    <div className="trash-row">
      <div className="trash-row__title">{note.title}</div>
      <div className="trash-row__meta">
        {note.deletedAt && relativeTime(note.deletedAt, t)
          ? t("trash.deletedAgo", { when: relativeTime(note.deletedAt, t) })
          : ""}
        {" · "}
        {t("trash.daysLeft", { count: days })}
      </div>
      <div className="row-actions">
        <button className="row-action row-action--restore" onClick={onRestore}>
          <RestoreIcon size={12} />
          {t("noteAction.restore")}
        </button>
        <button className="row-action row-action--danger" onClick={onDeleteForever}>
          {t("noteAction.deleteForever")}
        </button>
      </div>
    </div>
  );
}

function BulkBar({
  count,
  scope,
  onCancel,
  onPin,
  onArchive,
  onExport,
  onTrash,
}: {
  count: number;
  scope: NoteScope;
  onCancel: () => void;
  onPin: () => void;
  onArchive: () => void;
  onExport: () => void;
  onTrash: () => void;
}) {
  const { t } = useI18n();
  return (
    <div className="bulk-bar">
      <div className="bulk-bar__head">
        <span className="bulk-bar__count">{t("select.count", { count })}</span>
        <button className="bulk-bar__cancel" onClick={onCancel}>
          {t("select.cancel")}
        </button>
      </div>
      <div className="bulk-bar__actions">
        {scope === "active" && (
          <button className="bulk-action" onClick={onPin}>
            <PinIcon size={15} stroke="var(--pin)" />
            {t("noteAction.pin")}
          </button>
        )}
        {scope === "active" && (
          <button className="bulk-action" onClick={onArchive}>
            <ArchiveIcon size={15} />
            {t("noteAction.archive")}
          </button>
        )}
        <button className="bulk-action" onClick={onExport}>
          <ExportIcon size={15} />
          {t("noteAction.export")}
        </button>
        <button className="bulk-action bulk-action--danger" onClick={onTrash}>
          <TrashIcon size={15} />
          {t("noteAction.toTrash")}
        </button>
      </div>
    </div>
  );
}

function NoteContextMenu({
  note,
  x,
  y,
  onClose,
  onPin,
  onRename,
  onDuplicate,
  onArchive,
  onExport,
  onTrash,
}: {
  note: Note;
  x: number;
  y: number;
  onClose: () => void;
  onPin: (n: Note) => void;
  onRename: (n: Note) => void;
  onDuplicate: (n: Note) => void;
  onArchive: (n: Note) => void;
  onExport: (n: Note) => void;
  onTrash: (n: Note) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Keep the menu on-screen: clamp to the viewport once we know its size.
  const [pos, setPos] = useState({ x, y });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.min(x, window.innerWidth - r.width - 8),
      y: Math.min(y, window.innerHeight - r.height - 8),
    });
  }, [x, y]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const { t } = useI18n();
  const run = (fn: () => void) => () => {
    fn();
    onClose();
  };

  return (
    <>
      <div className="popover-scrim" onClick={onClose} onContextMenu={(e) => e.preventDefault()} />
      <div
        ref={ref}
        className="context-menu"
        style={{ left: pos.x, top: pos.y }}
        role="menu"
      >
        <button className="context-menu__item" role="menuitem" onClick={run(() => onPin(note))}>
          <PinIcon size={13} stroke="var(--pin)" />
          {note.pinned ? t("noteAction.unpin") : t("noteAction.pin")}
        </button>
        <button className="context-menu__item" role="menuitem" onClick={run(() => onRename(note))}>
          <RenameIcon size={13} />
          {t("noteAction.rename")}
        </button>
        <button
          className="context-menu__item"
          role="menuitem"
          onClick={run(() => onDuplicate(note))}
        >
          <DuplicateIcon size={13} />
          {t("noteAction.duplicate")}
        </button>
        <button
          className="context-menu__item"
          role="menuitem"
          onClick={run(() => onArchive(note))}
        >
          <ArchiveIcon size={13} />
          {t("noteAction.archive")}
        </button>
        <button className="context-menu__item" role="menuitem" onClick={run(() => onExport(note))}>
          <ExportIcon size={13} />
          {t("noteAction.export")}
        </button>
        <div className="context-menu__divider" />
        <button
          className="context-menu__item context-menu__item--danger"
          role="menuitem"
          onClick={run(() => onTrash(note))}
        >
          <TrashIcon size={13} />
          {t("noteAction.toTrash")}
        </button>
      </div>
    </>
  );
}
