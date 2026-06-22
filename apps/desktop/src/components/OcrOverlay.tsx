import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { useI18n } from "../lib/i18n";
import type { OcrLayout } from "../lib/types";

interface Props {
  /** Object URL of the original (displayed) image. */
  imageUrl: string;
  layout: OcrLayout;
  /** Insert the given text into the active note. */
  onInsert: (text: string) => void;
  onClose: () => void;
}

/**
 * A Snipping-Tool-style OCR result: the image with a transparent, selectable
 * text layer positioned per recognized word. Select and copy (Ctrl+C) or push
 * the text into the note; with no selection (or "Select all") the whole
 * recognized text is used.
 *
 * The word boxes are absolutely positioned, so a raw `Selection.toString()`
 * mashes them together without spaces. To make selection actually useful, the
 * selected text is reconstructed from the underlying OCR words (`layout.words`),
 * re-inserting spaces between words and newlines between rows — and a full
 * selection falls back to the layout-preserving `layout.text`.
 */
export function OcrOverlay({ imageUrl, layout, onInsert, onClose }: Props) {
  const { t } = useI18n();
  const imgRef = useRef<HTMLImageElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const plainRef = useRef<HTMLPreElement>(null);
  // One DOM node per recognized word, so we can map a DOM selection back to the
  // OCR words it covers.
  const wordRefs = useRef<(HTMLSpanElement | null)[]>([]);
  // Rendered pixels per OCR pixel, so the word boxes line up with the image.
  const [scale, setScale] = useState(0);

  const hasBoxes = layout.words.length > 0 && layout.width > 0 && scale > 0;

  useLayoutEffect(() => {
    const img = imgRef.current;
    if (!img || !layout.width) return;
    const update = () => setScale(img.clientWidth / layout.width);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(img);
    return () => observer.disconnect();
  }, [layout.width]);

  // The current selection as clean text: reconstructed from the covered OCR
  // words (spaces between words, newlines between rows), or the full layout text
  // when nothing — or everything — is selected.
  const gatherText = useCallback((): string => {
    const sel = window.getSelection();
    if (!hasBoxes) {
      const s = sel?.toString().trim();
      return s || layout.text;
    }
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return layout.text;
    const picked: number[] = [];
    wordRefs.current.forEach((el, i) => {
      if (el && sel.containsNode(el, true)) picked.push(i);
    });
    if (picked.length === 0) return layout.text;
    if (picked.length === layout.words.length) return layout.text; // full → layout text

    let out = "";
    let prev: OcrLayout["words"][number] | null = null;
    for (const i of picked) {
      const w = layout.words[i];
      if (prev) out += w.y > prev.y + prev.height * 0.6 ? "\n" : " ";
      out += w.text;
      prev = w;
    }
    return out;
  }, [hasBoxes, layout]);

  // Select the whole recognized text (the word layer, or the plain fallback).
  const selectAll = useCallback(() => {
    const node = hasBoxes ? layerRef.current : plainRef.current;
    const sel = window.getSelection();
    if (!node || !sel) return;
    const range = document.createRange();
    range.selectNodeContents(node);
    sel.removeAllRanges();
    sel.addRange(range);
  }, [hasBoxes]);

  // Escape closes; Ctrl+A selects all; Ctrl+C copies the reconstructed text.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "a") {
        e.preventDefault();
        selectAll();
      } else if (mod && e.key.toLowerCase() === "c") {
        e.preventDefault();
        void navigator.clipboard.writeText(gatherText());
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, selectAll, gatherText]);

  // preventDefault on mousedown keeps the text selection from being cleared when
  // a toolbar button takes focus, so the buttons can act on the selection.
  const keepSelection = (e: React.MouseEvent) => e.preventDefault();

  return (
    <div className="ocr-overlay" role="dialog" aria-modal="true">
      <div className="ocr-overlay__panel">
        <div className="ocr-overlay__toolbar">
          <span className="ocr-overlay__title">{t("ocr.title")}</span>
          <span className="ocr-overlay__spacer" />
          <button
            className="action-btn"
            onMouseDown={keepSelection}
            onClick={selectAll}
            title={t("ocr.selectAll.title")}
          >
            {t("ocr.selectAll")}
          </button>
          <button
            className="action-btn"
            onMouseDown={keepSelection}
            onClick={() => onInsert(gatherText())}
            title={t("ocr.insert.title")}
          >
            {t("ocr.insert")}
          </button>
          <button
            className="action-btn"
            onMouseDown={keepSelection}
            onClick={() => void navigator.clipboard.writeText(gatherText())}
            title={t("ocr.copy.title")}
          >
            {t("ocr.copy")}
          </button>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            ✕
          </button>
        </div>
        <div className="ocr-overlay__hint">{t("ocr.hint")}</div>
        <div className="ocr-overlay__stage">
          <div className="ocr-overlay__image-wrap">
            <img
              ref={imgRef}
              src={imageUrl}
              alt={t("ocr.imageAlt")}
              className="ocr-overlay__image"
              draggable={false}
            />
            {hasBoxes && (
              <div className="ocr-overlay__layer" ref={layerRef}>
                {layout.words.map((word, i) => (
                  <span
                    key={`${i}:${word.x}:${word.y}`}
                    ref={(el) => {
                      wordRefs.current[i] = el;
                    }}
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
          {!hasBoxes && (
            <pre className="ocr-overlay__plain" ref={plainRef}>
              {layout.text}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}
