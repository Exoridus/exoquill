import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { OcrLayout } from "../lib/types";

interface Props {
  /** Object URL of the original (displayed) image. */
  imageUrl: string;
  layout: OcrLayout;
  /** Insert the given text into the active note. */
  onInsert: (text: string) => void;
  onClose: () => void;
}

/** The current selection within the overlay, or the full layout text if none. */
function selectionOrAll(layout: OcrLayout): string {
  const selection = window.getSelection()?.toString().trim();
  return selection || layout.text;
}

/**
 * A Snipping-Tool-style OCR result: the image with a transparent, selectable
 * text layer positioned per recognized word. Select and copy (Ctrl+C) or push
 * the text into the note; with no selection the whole recognized text is used.
 */
export function OcrOverlay({ imageUrl, layout, onInsert, onClose }: Props) {
  const imgRef = useRef<HTMLImageElement>(null);
  // Rendered pixels per OCR pixel, so the word boxes line up with the image.
  const [scale, setScale] = useState(0);

  useLayoutEffect(() => {
    const img = imgRef.current;
    if (!img || !layout.width) return;
    const update = () => setScale(img.clientWidth / layout.width);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(img);
    return () => observer.disconnect();
  }, [layout.width]);

  // Ctrl+C with no selection copies the whole text; Escape closes.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
        if (!window.getSelection()?.toString()) {
          e.preventDefault();
          void navigator.clipboard.writeText(layout.text);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [layout.text, onClose]);

  // preventDefault on mousedown keeps the text selection from being cleared when
  // a toolbar button takes focus, so the buttons can act on the selection.
  const keepSelection = (e: React.MouseEvent) => e.preventDefault();
  const hasBoxes = layout.words.length > 0 && layout.width > 0 && scale > 0;

  return (
    <div className="ocr-overlay" role="dialog" aria-modal="true">
      <div className="ocr-overlay__panel">
        <div className="ocr-overlay__toolbar">
          <span className="ocr-overlay__title">Texterkennung</span>
          <span className="ocr-overlay__spacer" />
          <button
            className="action-btn"
            onMouseDown={keepSelection}
            onClick={() => onInsert(selectionOrAll(layout))}
            title="Auswahl – oder den gesamten Text – in die Notiz übernehmen"
          >
            In Notiz
          </button>
          <button
            className="action-btn"
            onMouseDown={keepSelection}
            onClick={() => void navigator.clipboard.writeText(selectionOrAll(layout))}
            title="Auswahl – oder den gesamten Text – kopieren (Strg+C)"
          >
            Kopieren
          </button>
          <button className="icon-btn" onClick={onClose} aria-label="Schließen">
            ✕
          </button>
        </div>
        <div className="ocr-overlay__hint">
          Text markieren und kopieren oder „In Notiz“ übernehmen — ohne Auswahl wird der gesamte
          erkannte Text genutzt.
        </div>
        <div className="ocr-overlay__stage">
          <div className="ocr-overlay__image-wrap">
            <img
              ref={imgRef}
              src={imageUrl}
              alt="Erkanntes Bild"
              className="ocr-overlay__image"
              draggable={false}
            />
            {hasBoxes && (
              <div className="ocr-overlay__layer">
                {layout.words.map((word, i) => (
                  <span
                    key={`${i}:${word.x}:${word.y}`}
                    className="ocr-overlay__word"
                    style={{
                      left: word.x * scale,
                      top: word.y * scale,
                      width: word.width * scale,
                      height: word.height * scale,
                      fontSize: Math.max(6, word.height * scale * 0.8),
                    }}
                  >
                    {word.text}
                  </span>
                ))}
              </div>
            )}
          </div>
          {!hasBoxes && <pre className="ocr-overlay__plain">{layout.text}</pre>}
        </div>
      </div>
    </div>
  );
}
