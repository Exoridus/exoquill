import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "tiptap-markdown";

interface Props {
  /** Initial Markdown. The parent remounts this via `key` on note switch. */
  initialMarkdown: string;
  onChange: (markdown: string) => void;
}

export function Editor({ initialMarkdown, onChange }: Props) {
  const editor = useEditor({
    extensions: [
      StarterKit,
      Markdown.configure({ html: false, transformPastedText: true }),
      Placeholder.configure({ placeholder: "Start writing, or capture something…" }),
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

  return <EditorContent editor={editor} className="editor-body" />;
}
