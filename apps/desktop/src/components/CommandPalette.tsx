import { useEffect, useRef, useState, type ReactElement } from "react";

import "../styles/palette.css";

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  shortcut?: string;
  group?: string;
  disabled?: boolean;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  commands: PaletteCommand[];
  placeholder: string;
  emptyLabel: string;
}

export function CommandPalette({
  open,
  onClose,
  commands,
  placeholder,
  emptyLabel,
}: CommandPaletteProps): ReactElement | null {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Reset state whenever the palette opens
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // Focus on next tick so the element is visible
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [open]);

  if (!open) return null;

  // Filter: case-insensitive substring match against label + hint + group
  const filtered = commands.filter((cmd) => {
    if (!query) return true;
    const q = query.toLowerCase();
    const haystack = [cmd.label, cmd.hint ?? "", cmd.group ?? ""]
      .join(" ")
      .toLowerCase();
    return haystack.includes(q);
  });

  // Selectable items (non-disabled) for keyboard navigation
  const selectable = filtered.filter((cmd) => !cmd.disabled);

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>): void {
    if (e.key === "Escape") {
      onClose();
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      const nextIndex = (activeIndex + 1) % selectable.length;
      setActiveIndex(nextIndex);
      scrollItemIntoView(nextIndex, selectable);
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      const prevIndex = (activeIndex - 1 + selectable.length) % selectable.length;
      setActiveIndex(prevIndex);
      scrollItemIntoView(prevIndex, selectable);
      return;
    }

    if (e.key === "Enter") {
      const cmd = selectable[activeIndex];
      if (cmd && !cmd.disabled) {
        cmd.run();
        onClose();
      }
    }
  }

  function scrollItemIntoView(selectableIdx: number, selectableList: PaletteCommand[]): void {
    const cmd = selectableList[selectableIdx];
    if (!cmd) return;
    const filteredIdx = filtered.findIndex((c) => c.id === cmd.id);
    if (filteredIdx === -1) return;
    const el = itemRefs.current[filteredIdx];
    el?.scrollIntoView({ block: "nearest" });
  }

  function handleItemClick(cmd: PaletteCommand): void {
    if (cmd.disabled) return;
    cmd.run();
    onClose();
  }

  function handleItemMouseEnter(cmd: PaletteCommand): void {
    if (cmd.disabled) return;
    const idx = selectable.findIndex((c) => c.id === cmd.id);
    if (idx !== -1) setActiveIndex(idx);
  }

  function handleQueryChange(e: React.ChangeEvent<HTMLInputElement>): void {
    setQuery(e.target.value);
    setActiveIndex(0);
  }

  // Build rendered list with group headers
  const renderedItems: ReactElement[] = [];
  let lastGroup: string | undefined = undefined;
  let filteredItemIdx = 0;

  for (const cmd of filtered) {
    // Render group header when group changes (skip when undefined)
    if (cmd.group !== undefined && cmd.group !== lastGroup) {
      renderedItems.push(
        <div key={`group-${cmd.group}`} className="palette__group">
          {cmd.group}
        </div>,
      );
      lastGroup = cmd.group;
    } else if (cmd.group === undefined) {
      lastGroup = undefined;
    }

    const isActive =
      !cmd.disabled &&
      selectable[activeIndex]?.id === cmd.id;

    const currentIdx = filteredItemIdx;
    filteredItemIdx++;

    renderedItems.push(
      <div
        key={cmd.id}
        ref={(el) => {
          itemRefs.current[currentIdx] = el;
        }}
        className={[
          "palette__item",
          isActive ? "palette__item--active" : "",
          cmd.disabled ? "palette__item--disabled" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        onClick={() => handleItemClick(cmd)}
        onMouseEnter={() => handleItemMouseEnter(cmd)}
        role="option"
        aria-selected={isActive}
        aria-disabled={cmd.disabled}
      >
        <span className="palette__label">{cmd.label}</span>
        {cmd.hint !== undefined && (
          <span className="palette__hint">{cmd.hint}</span>
        )}
        {cmd.shortcut !== undefined && (
          <span className="palette__shortcut">{cmd.shortcut}</span>
        )}
      </div>,
    );
  }

  return (
    <div
      className="palette-backdrop"
      onMouseDown={(e) => {
        // Close only when clicking the backdrop itself, not the panel
        if (e.target === e.currentTarget) onClose();
      }}
      role="dialog"
      aria-modal="true"
    >
      <div className="palette">
        <input
          ref={inputRef}
          className="palette__input"
          type="text"
          value={query}
          onChange={handleQueryChange}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          aria-label={placeholder}
          role="combobox"
          aria-expanded={true}
          aria-haspopup="listbox"
          aria-autocomplete="list"
        />
        <div
          ref={listRef}
          className="palette__list"
          role="listbox"
        >
          {filtered.length === 0 ? (
            <div className="palette__empty">{emptyLabel}</div>
          ) : (
            renderedItems
          )}
        </div>
      </div>
    </div>
  );
}
