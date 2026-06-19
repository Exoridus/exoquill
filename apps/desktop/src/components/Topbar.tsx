import type { Theme } from "../hooks/useTheme";
import { LogoMark } from "./Logo";
import { MoonIcon, SunIcon } from "./icons";

interface Props {
  theme: Theme;
  onToggleTheme: () => void;
}

export function Topbar({ theme, onToggleTheme }: Props) {
  return (
    <header className="topbar">
      <LogoMark />
      <span className="topbar__wordmark">exoquill</span>
      <div className="topbar__spacer" />
      <span className="on-device">
        <span className="on-device__dot" />
        ON-DEVICE
      </span>
      <button
        className="icon-btn"
        onClick={onToggleTheme}
        title="Toggle theme"
        aria-label="Toggle light/dark theme"
      >
        {theme === "dark" ? <SunIcon size={16} /> : <MoonIcon size={16} />}
      </button>
    </header>
  );
}
