import type { Note } from "../lib/types";

interface Props {
  note: Note | null;
  saved: boolean;
}

function countWords(markdown: string): number {
  const text = markdown.replace(/[#*`>_\-]/g, " ").trim();
  return text ? text.split(/\s+/).length : 0;
}

export function Statusbar({ note, saved }: Props) {
  return (
    <footer className="statusbar">
      <span>de-DE · en-US</span>
      <span>{saved ? "SAVED" : "SAVING…"}</span>
      <span className="statusbar__accent">LOCAL · ON-DEVICE</span>
      <span className="statusbar__spacer">{note ? countWords(note.contentMarkdown) : 0} WORDS</span>
    </footer>
  );
}
