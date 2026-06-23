import { useEffect, useState } from "react";

/** The effective theme actually applied to the document. */
export type Theme = "light" | "dark";
/** The user's chosen mode; "system" follows the OS preference. */
export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "exoquill-theme";

function initialMode(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  // No stored choice: follow the system from the start.
  return "system";
}

function systemTheme(): Theme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export interface ThemeControl {
  /** The user's chosen mode (may be "system"). */
  mode: ThemeMode;
  /** The effective theme actually applied (system resolved to light/dark). */
  theme: Theme;
  /** Choose a mode explicitly (light / dark / follow system). */
  setMode: (mode: ThemeMode) => void;
  /** Quick binary flip to the opposite of the current effective theme. */
  toggle: () => void;
}

/** Manages the `data-theme` attribute and persists the chosen mode. Supports a
 *  "system" mode that tracks `prefers-color-scheme` live. */
export function useTheme(): ThemeControl {
  const [mode, setMode] = useState<ThemeMode>(initialMode);
  const [sysTheme, setSysTheme] = useState<Theme>(systemTheme);

  // Track the OS preference so "system" mode updates without a reload.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSysTheme(mq.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const theme: Theme = mode === "system" ? sysTheme : mode;

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem(STORAGE_KEY, mode);
  }, [theme, mode]);

  const toggle = () => setMode(theme === "dark" ? "light" : "dark");
  return { mode, theme, setMode, toggle };
}
