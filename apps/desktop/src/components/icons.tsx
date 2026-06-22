import type { SVGProps } from "react";

type Props = SVGProps<SVGSVGElement> & { size?: number };

function Svg({ size = 16, children, ...rest }: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...rest}
    >
      {children}
    </svg>
  );
}

export const SearchIcon = (p: Props) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="m21 21-4.3-4.3" />
  </Svg>
);

export const DictateIcon = (p: Props) => (
  <Svg {...p}>
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0" />
    <path d="M12 18v3" />
  </Svg>
);

export const OcrIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M4 8V5a1 1 0 0 1 1-1h3" />
    <path d="M20 8V5a1 1 0 0 0-1-1h-3" />
    <path d="M4 16v3a1 1 0 0 0 1 1h3" />
    <path d="M20 16v3a1 1 0 0 1-1 1h-3" />
    <path d="M8 12h8" />
  </Svg>
);

export const FormatIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M4 6h16" />
    <path d="M4 11h11" />
    <path d="M4 16h16" />
  </Svg>
);

export const ReadIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M4 9v6h4l5 4V5L8 9H4z" />
    <path d="M17 9a4 4 0 0 1 0 6" />
  </Svg>
);

export const PlusIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M12 5v14" />
    <path d="M5 12h14" />
  </Svg>
);

export const TrashIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M3 6h18" />
    <path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2" />
    <path d="M6 6v14a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6" />
  </Svg>
);

export const SunIcon = (p: Props) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
  </Svg>
);

export const MoonIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
  </Svg>
);

export const GlobeIcon = (p: Props) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18" />
    <path d="M12 3a14 14 0 0 1 0 18a14 14 0 0 1 0-18z" />
  </Svg>
);

// Pin (quill nib). Pass `fill="currentColor"` for the filled/pinned state.
export const PinIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M14 2l8 8-5 1-3 6-2-2-4 4-1-1 4-4-2-2 6-3z" />
  </Svg>
);

export const ArchiveIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M4 9v9a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9" />
    <rect x="2" y="4" width="20" height="5" rx="1" />
    <path d="M10 13h4" />
  </Svg>
);

// Counter-clockwise arrow — restore from archive/trash, and version restore.
export const RestoreIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
    <path d="M3 3v5h5" />
  </Svg>
);

export const DuplicateIcon = (p: Props) => (
  <Svg {...p}>
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V5a2 2 0 0 1 2-2h10" />
  </Svg>
);

export const RenameIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
  </Svg>
);

// Arrow down to a line — export/download.
export const ExportIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M12 3v12" />
    <path d="m8 11 4 4 4-4" />
    <path d="M4 19h16" />
  </Svg>
);

export const CheckIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M5 12l5 5 9-11" />
  </Svg>
);

// Clock with a rewind arrow — edit history.
export const HistoryIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M3 12a9 9 0 1 0 9-9 9 9 0 0 0-9 9" />
    <path d="M3 3v5h5" />
    <path d="M12 8v4l3 2" />
  </Svg>
);

export const ClockIcon = (p: Props) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 8v4l3 2" />
  </Svg>
);

export const ChevronDownIcon = (p: Props) => (
  <Svg {...p}>
    <path d="m6 9 6 6 6-6" />
  </Svg>
);

export const ChevronRightIcon = (p: Props) => (
  <Svg {...p}>
    <path d="M9 18l6-6-6-6" />
  </Svg>
);
