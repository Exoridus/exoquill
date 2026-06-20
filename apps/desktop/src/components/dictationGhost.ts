// A TipTap extension that renders live dictation "ghost text": the in-progress
// partial transcript shown at the cursor while you speak, before the segment
// finalizes. It is a view-only ProseMirror widget decoration — it never enters
// the document, so it doesn't touch undo history or autosave. App.tsx replaces
// it with the real text when the final `dictation_segment` arrives.
//
// The partial is split into a `stable` prefix (words the backend has committed
// via LocalAgreement-2) and a `tail` (still tentative). They render with
// different opacity so the settled prefix reads calmly and only the tail moves.

import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

/** The current ghost: a frozen prefix and the still-tentative tail. */
interface GhostText {
  stable: string;
  tail: string;
}

const EMPTY: GhostText = { stable: "", tail: "" };

const ghostKey = new PluginKey<GhostText>("dictationGhost");

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    dictationGhost: {
      /** Show the ghost at the cursor: `stable` prefix + tentative `tail`. */
      setDictationGhost: (stable: string, tail: string) => ReturnType;
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
        (stable: string, tail: string) =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(ghostKey, { stable, tail }));
          return true;
        },
      clearDictationGhost:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(ghostKey, EMPTY));
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    return [
      new Plugin<GhostText>({
        key: ghostKey,
        state: {
          init: () => EMPTY,
          apply(tr, value) {
            const meta = tr.getMeta(ghostKey);
            return meta !== undefined ? (meta as GhostText) : value;
          },
        },
        props: {
          decorations(state) {
            const { stable, tail } = ghostKey.getState(state) ?? EMPTY;
            if (!stable && !tail) return null;
            const pos = state.selection.to;
            // Mirror insertAtCursor: a separating space when the preceding
            // character isn't already whitespace.
            const preceding =
              pos > 1 ? state.doc.textBetween(pos - 1, pos, "\n") : "";
            const lead = preceding && !/\s$/.test(preceding) ? " " : "";

            const container = document.createElement("span");
            container.className = "dictation-ghost";
            // Render the committed prefix and the tentative tail as separate
            // spans (first piece carries the separating space, then a space
            // between the two).
            const parts: Array<[string, string]> = [];
            if (stable) parts.push(["dictation-ghost__stable", stable]);
            if (tail) parts.push(["dictation-ghost__tail", tail]);
            parts.forEach(([cls, text], i) => {
              const child = document.createElement("span");
              child.className = cls;
              child.textContent = (i === 0 ? lead : " ") + text;
              container.appendChild(child);
            });

            return DecorationSet.create(state.doc, [
              Decoration.widget(pos, container, { side: 1 }),
            ]);
          },
        },
      }),
    ];
  },
});
