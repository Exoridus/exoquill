import "../styles/selection-menu.css";
import { useEffect, useRef, useState, type ReactElement } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";

export interface SelectionAction {
  id: string;
  label: string;
  title?: string;
  run: (selectionText: string) => void;
}

interface SelectionMenuProps {
  editor: TiptapEditor | null;
  actions: SelectionAction[];
}

interface Position {
  x: number;
  y: number;
}

export function SelectionMenu({ editor, actions }: SelectionMenuProps): ReactElement | null {
  const [position, setPosition] = useState<Position | null>(null);
  const capturedTextRef = useRef<string>("");

  useEffect(() => {
    if (!editor) {
      setPosition(null);
      return;
    }

    const update = () => {
      const { from, to } = editor.state.selection;

      if (from === to || !editor.view) {
        setPosition(null);
        return;
      }

      try {
        const docSize = editor.state.doc.content.size;
        if (from < 0 || to > docSize) {
          setPosition(null);
          return;
        }

        const start = editor.view.coordsAtPos(from);
        const end = editor.view.coordsAtPos(to);

        const rawMidX = (start.left + end.right) / 2;
        const midX = Math.min(Math.max(rawMidX, 12), window.innerWidth - 12);
        const top = start.top;

        capturedTextRef.current = editor.state.doc.textBetween(from, to, "\n");
        setPosition({ x: midX, y: top });
      } catch {
        setPosition(null);
      }
    };

    const hide = () => {
      setPosition(null);
    };

    editor.on("selectionUpdate", update);
    editor.on("transaction", update);
    editor.on("blur", hide);
    editor.on("focus", update);

    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);

    return () => {
      editor.off("selectionUpdate", update);
      editor.off("transaction", update);
      editor.off("blur", hide);
      editor.off("focus", update);

      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  }, [editor]);

  if (!position || actions.length === 0) {
    return null;
  }

  const handleAction = (action: SelectionAction) => (e: React.MouseEvent) => {
    e.preventDefault();
    const text = capturedTextRef.current;
    setPosition(null);
    action.run(text);
  };

  return (
    <div
      className="selection-menu"
      style={{
        left: position.x,
        top: position.y - 8,
        transform: "translate(-50%, -100%)",
      }}
    >
      {actions.map((action, index) => (
        <>
          {index > 0 && <div key={`divider-${action.id}`} className="selection-menu__divider" />}
          <button
            key={action.id}
            className="selection-menu__btn"
            title={action.title}
            onMouseDown={handleAction(action)}
          >
            {action.label}
          </button>
        </>
      ))}
    </div>
  );
}
