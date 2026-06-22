// Undo-toast queue — the feedback channel for reversible/destructive actions
// (trash, archive, restore, bulk ops), replacing modal confirm()/alert(). Each
// toast auto-dismisses after a few seconds; an action toast carries an "Undo"
// button and can also be triggered with Ctrl/⌘+Z while not typing in the editor.

import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

export interface Toast {
  id: number;
  message: string;
  /** Label for the action button (e.g. "Rückgängig"); omit for a plain toast. */
  actionLabel?: string;
  /** Run on click / Ctrl+Z; its presence makes the toast an "undo" toast. */
  onAction?: () => void;
  icon?: ReactNode;
}

export interface ToastOptions {
  actionLabel?: string;
  onAction?: () => void;
  icon?: ReactNode;
  /** Auto-dismiss delay in ms (default 6000). */
  duration?: number;
}

export interface ToastApi {
  toasts: Toast[];
  push: (message: string, opts?: ToastOptions) => number;
  dismiss: (id: number) => void;
  /** Run a toast's action and dismiss it (the "Undo" button handler). */
  runAction: (toast: Toast) => void;
}

/** Toast queue with auto-dismiss timers and a window-level undo shortcut. */
export function useToasts(): ToastApi {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timers = useRef<Map<number, number>>(new Map());
  const idRef = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((ts) => ts.filter((t) => t.id !== id));
    const timer = timers.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
  }, []);

  const push = useCallback(
    (message: string, opts: ToastOptions = {}) => {
      const id = (idRef.current += 1);
      setToasts((ts) => [
        ...ts,
        { id, message, actionLabel: opts.actionLabel, onAction: opts.onAction, icon: opts.icon },
      ]);
      const timer = window.setTimeout(() => dismiss(id), opts.duration ?? 6000);
      timers.current.set(id, timer);
      return id;
    },
    [dismiss],
  );

  const runAction = useCallback(
    (toast: Toast) => {
      toast.onAction?.();
      dismiss(toast.id);
    },
    [dismiss],
  );

  // Ctrl/⌘+Z triggers the newest undoable toast — but only when the user isn't
  // typing in the editor or an input, where the same chord means text-undo.
  const toastsRef = useRef(toasts);
  toastsRef.current = toasts;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.key.toLowerCase() !== "z") return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.isContentEditable || /^(input|textarea|select)$/i.test(target.tagName))
      ) {
        return;
      }
      const undoable = [...toastsRef.current].reverse().find((t) => t.onAction);
      if (!undoable) return;
      e.preventDefault();
      runAction(undoable);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runAction]);

  // Clear pending timers on unmount.
  useEffect(() => {
    const map = timers.current;
    return () => map.forEach((t) => clearTimeout(t));
  }, []);

  return { toasts, push, dismiss, runAction };
}

interface StackProps {
  toasts: Toast[];
  onAction: (toast: Toast) => void;
  onDismiss: (id: number) => void;
}

/** Bottom-center stack of toasts. */
export function ToastStack({ toasts, onAction, onDismiss }: StackProps) {
  if (toasts.length === 0) return null;
  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className="toast">
          {t.icon && <span className="toast__icon">{t.icon}</span>}
          <span className="toast__msg">{t.message}</span>
          {t.onAction && (
            <button className="toast__action" onClick={() => onAction(t)}>
              {t.actionLabel}
            </button>
          )}
          <button className="toast__close" aria-hidden onClick={() => onDismiss(t.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
