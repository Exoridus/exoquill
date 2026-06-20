// A TipTap extension that renders live dictation "ghost text": the in-progress
// partial transcript shown at the cursor while you speak, before the segment
// finalizes. It is a view-only ProseMirror widget decoration — it never enters
// the document, so it doesn't touch undo history or autosave. App.tsx replaces
// it with the real text when the final `dictation_segment` arrives.

import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

const ghostKey = new PluginKey<string>("dictationGhost");

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    dictationGhost: {
      /** Show `text` as ghost text at the cursor (replaces any current ghost). */
      setDictationGhost: (text: string) => ReturnType;
      /** Remove the ghost text. */
      clearDictationGhost: () => ReturnType;
    };
  }
}

export const DictationGhost = Extension.create({
  name: "dictationGhost",

  addCommands() {
    return {
      setDictationGhost:
        (text: string) =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(ghostKey, text));
          return true;
        },
      clearDictationGhost:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(ghostKey, ""));
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    return [
      new Plugin<string>({
        key: ghostKey,
        state: {
          init: () => "",
          apply(tr, value) {
            const meta = tr.getMeta(ghostKey);
            return meta !== undefined ? (meta as string) : value;
          },
        },
        props: {
          decorations(state) {
            const text = ghostKey.getState(state) ?? "";
            if (!text) return null;
            const pos = state.selection.to;
            // Mirror insertAtCursor: a separating space when the preceding
            // character isn't already whitespace.
            const preceding =
              pos > 1 ? state.doc.textBetween(pos - 1, pos, "\n") : "";
            const prefix = preceding && !/\s$/.test(preceding) ? " " : "";
            const span = document.createElement("span");
            span.className = "dictation-ghost";
            span.textContent = prefix + text;
            return DecorationSet.create(state.doc, [
              Decoration.widget(pos, span, { side: 1 }),
            ]);
          },
        },
      }),
    ];
  },
});
