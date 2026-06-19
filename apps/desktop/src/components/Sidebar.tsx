import type { Note } from "../lib/types";
import { PlusIcon, SearchIcon } from "./icons";

interface Props {
  notes: Note[];
  activeId: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  onSelect: (id: string) => void;
  onNewNote: () => void;
}

function preview(note: Note): string {
  const lines = note.contentMarkdown
    .split("\n")
    .map((line) => line.replace(/^#+\s*/, "").replace(/[*`_>]/g, "").trim());
  // The first meaningful line is usually the title; show the next one.
  return lines.slice(1).find(Boolean) ?? lines.find(Boolean) ?? "Empty note";
}

export function Sidebar({ notes, activeId, query, onQueryChange, onSelect, onNewNote }: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar__search">
        <SearchIcon size={13} />
        <input
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          placeholder="Search notes"
        />
      </div>
      <div className="sidebar__count">{notes.length} NOTES</div>
      <div className="note-list">
        {notes.map((note) => (
          <div
            key={note.id}
            className={`note-item${note.id === activeId ? " active" : ""}`}
            onClick={() => onSelect(note.id)}
          >
            <div className="note-item__title">{note.title}</div>
            <div className="note-item__preview">{preview(note)}</div>
          </div>
        ))}
      </div>
      <div className="sidebar__footer">
        <button className="btn-primary" onClick={onNewNote}>
          <PlusIcon size={14} />
          New note
        </button>
      </div>
    </aside>
  );
}
