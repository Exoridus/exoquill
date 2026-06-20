import Placeholder from "@tiptap/extension-placeholder";
import { type Editor as TiptapEditor, EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect } from "react";
import { Markdown } from "tiptap-markdown";

import { DictationGhost } from "./dictationGhost";

interface Props {
  /** Initial Markdown. The parent remounts this via `key` on note switch. */
  initialMarkdown: string;
  onChange: (markdown: string) => void;
  /** Receives the editor instance so the parent can act on the selection. */
  onReady?: (editor: TiptapEditor) => void;
}

export function Editor({ initialMarkdown, onChange, onReady }: Props) {
  const editor = useEditor({
    extensions: [
      StarterKit,
      Markdown.configure({ html: false, transformPastedText: true }),
      Placeholder.configure({ placeholder: "Start writing, or capture something…" }),
      DictationGhost,
    ],
    content: initialMarkdown,
    onUpdate: ({ editor }) => {
      // tiptap-markdown adds `storage.markdown` at runtime but doesn't augment
      // the TipTap 3 Storage type; fall back to plain text if it's absent.
      const markdown = (editor.storage as { markdown?: { getMarkdown: () => string } })
        .markdown;
      onChange(markdown ? markdown.getMarkdown() : editor.getText());
    },
  });

  useEffect(() => {
    if (editor && onReady) onReady(editor);
  }, [editor, onReady]);

  return <EditorContent editor={editor} className="editor-body" />;
}

/** Read the current selection as plain text, or "" if nothing is selected. */
export function selectionText(editor: TiptapEditor | null): string {
  if (!editor) return "";
  const { from, to } = editor.state.selection;
  if (from === to) return "";
  return editor.state.doc.textBetween(from, to, "\n");
}

/** Replace the current selection with `text` (a single undoable step). */
export function replaceSelection(editor: TiptapEditor, text: string): void {
  const { from, to } = editor.state.selection;
  editor.chain().focus().insertContentAt({ from, to }, text).run();
}

/** Insert `text` at the cursor (used for dictation), adding a separating space
 *  when the preceding character isn't already whitespace. */
export function insertAtCursor(editor: TiptapEditor, text: string): void {
  const { from } = editor.state.selection;
  const preceding = from > 1 ? editor.state.doc.textBetween(from - 1, from, "\n") : "";
  const prefix = preceding && !/\s$/.test(preceding) ? " " : "";
  editor.chain().focus().insertContent(prefix + text).run();
}

/** A separating space before dictated text when the char before `pos` isn't
 *  whitespace and `pos` isn't the document start. */
export function separatorBefore(editor: TiptapEditor, pos: number): string {
  const preceding = pos > 1 ? editor.state.doc.textBetween(pos - 1, pos, "\n") : "";
  return preceding && !/\s$/.test(preceding) ? " " : "";
}

/** Replace the document range `[from, from+length)` with `text` as one step,
 *  leaving the cursor after it. Used to live-commit the stabilized prefix of a
 *  dictation partial, then to overwrite that region with the final transcript. */
export function replaceRange(
  editor: TiptapEditor,
  from: number,
  length: number,
  text: string,
): void {
  const to = Math.min(from + length, editor.state.doc.content.size);
  editor.chain().focus().insertContentAt({ from, to }, text).run();
}
