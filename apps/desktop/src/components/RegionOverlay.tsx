// Fullscreen snipping-tool overlay shown in its own borderless window over the
// monitor under the cursor. It displays the frozen screenshot, lets the user
// drag a selection rectangle, then crops + OCRs that region in the backend and
// forwards the result to the main window (which opens the selectable result
// overlay). Escape / right-click cancels. Coordinates are CSS px relative to the
// window, which exactly covers the monitor, so the backend maps them to pixels.

import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";

import { cancelRegionOcr, getRegionCapture, ocrRegion } from "../lib/api";
import { useI18n } from "../lib/i18n";

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

function rectFrom(ax: number, ay: number, bx: number, by: number): Rect {
  return {
    x: Math.min(ax, bx),
    y: Math.min(ay, by),
    width: Math.abs(ax - bx),
    height: Math.abs(ay - by),
  };
}

export function RegionOverlay() {
  const { t } = useI18n();
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [busy, setBusy] = useState(false);
  const start = useRef<{ x: number; y: number } | null>(null);

  const close = useCallback(() => void getCurrentWindow().close(), []);

  const cancel = useCallback(() => {
    void cancelRegionOcr().finally(close);
  }, [close]);

  // Load the frozen screenshot; if it isn't there, there's nothing to select.
  useEffect(() => {
    getRegionCapture()
      .then((c) => setImageUrl(c.dataUrl))
      .catch(cancel);
  }, [cancel]);

  // Escape cancels the whole thing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cancel]);

  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0 || busy) return;
    start.current = { x: e.clientX, y: e.clientY };
    setRect({ x: e.clientX, y: e.clientY, width: 0, height: 0 });
  };

  const onMouseMove = (e: React.MouseEvent) => {
    if (!start.current) return;
    setRect(rectFrom(start.current.x, start.current.y, e.clientX, e.clientY));
  };

  const onMouseUp = async () => {
    const s = start.current;
    start.current = null;
    if (!s || !rect) return;
    // Too small to be a real selection → treat as a cancel.
    if (rect.width < 4 || rect.height < 4) {
      cancel();
      return;
    }
    setBusy(true);
    try {
      const result = await ocrRegion(rect);
      await emit("region-ocr-result", result);
    } catch (error) {
      await emit("region-ocr-error", String(error));
    } finally {
      close();
    }
  };

  return (
    <div
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={() => void onMouseUp()}
      onContextMenu={(e) => {
        e.preventDefault();
        cancel();
      }}
      style={{
        position: "fixed",
        inset: 0,
        cursor: busy ? "progress" : "crosshair",
        userSelect: "none",
        overflow: "hidden",
        background: "#000",
      }}
    >
      {imageUrl && (
        <img
          src={imageUrl}
          alt=""
          draggable={false}
          style={{ width: "100vw", height: "100vh", display: "block" }}
        />
      )}
      {/* Dim everything; the selection rectangle punches a bright, bordered hole. */}
      <div
        style={{
          position: "fixed",
          inset: 0,
          background: "rgba(0,0,0,0.35)",
          pointerEvents: "none",
        }}
      />
      {rect && (
        <div
          style={{
            position: "fixed",
            left: rect.x,
            top: rect.y,
            width: rect.width,
            height: rect.height,
            border: "1.5px solid #4c9aff",
            boxShadow: "0 0 0 9999px rgba(0,0,0,0)",
            background: "rgba(76,154,255,0.12)",
            pointerEvents: "none",
          }}
        />
      )}
      {!busy && (
        <div
          style={{
            position: "fixed",
            top: 16,
            left: "50%",
            transform: "translateX(-50%)",
            padding: "6px 14px",
            borderRadius: 6,
            background: "rgba(20,20,20,0.8)",
            color: "#eee",
            fontFamily: "var(--font-mono, monospace)",
            fontSize: 12,
            letterSpacing: "0.03em",
            pointerEvents: "none",
          }}
        >
          {t("region.hint")}
        </div>
      )}
    </div>
  );
}
