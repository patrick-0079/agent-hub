import React from "react";

export type IconName =
  | "dashboard"
  | "providers"
  | "skills"
  | "mcp"
  | "npm"
  | "python"
  | "profiles"
  | "agents"
  | "adapter"
  | "history"
  | "vault"
  | "settings"
  | "refresh"
  | "folder"
  | "alert"
  | "check"
  | "chevronDown"
  | "chevronRight"
  | "external"
  | "search"
  | "close"
  | "terminal"
  | "lock"
  | "plus"
  | "sparkle"
  | "play"
  | "info"
  | "shield"
  | "cpu"
  | "link"
  | "trash"
  | "dot";

const PATHS: Record<IconName, React.ReactNode> = {
  dashboard: (
    <>
      <rect x="3" y="3" width="7.5" height="7.5" rx="2" />
      <rect x="13.5" y="3" width="7.5" height="7.5" rx="2" />
      <rect x="3" y="13.5" width="7.5" height="7.5" rx="2" />
      <rect x="13.5" y="13.5" width="7.5" height="7.5" rx="2" />
    </>
  ),
  providers: (
    <path d="M6.5 18.5h10.8a4.2 4.2 0 0 0 .5-8.4 6.3 6.3 0 0 0-12-1.6A3.7 3.7 0 0 0 6.5 18.5Z" />
  ),
  skills: (
    <>
      <path d="M12 3a6.5 6.5 0 0 0-3.7 11.8V18h7.4v-3.2A6.5 6.5 0 0 0 12 3Z" />
      <path d="M9.5 21h5" />
    </>
  ),
  mcp: (
    <>
      <circle cx="12" cy="12" r="3" />
      <circle cx="4.5" cy="5" r="2" />
      <circle cx="19.5" cy="5" r="2" />
      <circle cx="4.5" cy="19" r="2" />
      <circle cx="19.5" cy="19" r="2" />
      <path d="m6 6.4 3.9 3.6M18 6.4 14.1 10M6 17.6 9.9 14M18 17.6 14.1 14" />
    </>
  ),
  npm: (
    <>
      <path d="M12 3 21 8.5v7L12 21 3 15.5v-7L12 3Z" />
      <path d="M12 3v18M3 8.5h18" />
    </>
  ),
  python: <path d="m9.5 8-5 4 5 4M14.5 8l5 4-5 4" />,
  profiles: (
    <>
      <path d="m12 3 9 4.8-9 4.8-9-4.8L12 3Z" />
      <path d="m3 12.6 9 4.8 9-4.8M3 17.2l9 4.8 9-4.8" />
    </>
  ),
  agents: (
    <>
      <rect x="4" y="8" width="16" height="12" rx="3" />
      <path d="M12 8V4M9 13.5h.01M15 13.5h.01M9.5 17h5" />
    </>
  ),
  adapter: (
    <>
      <path d="M3 7h9M18 7h3M3 17h4M13 17h8" />
      <circle cx="15" cy="7" r="2.4" />
      <circle cx="10" cy="17" r="2.4" />
    </>
  ),
  history: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  vault: (
    <>
      <rect x="4" y="10" width="16" height="11" rx="2.5" />
      <path d="M8 10V7.2a4 4 0 0 1 8 0V10M12 14v3" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 2.5v2.8M12 18.7v2.8M4.9 4.9l2 2M17.1 17.1l2 2M2.5 12h2.8M18.7 12h2.8M4.9 19.1l2-2M17.1 6.9l2-2" />
    </>
  ),
  refresh: (
    <>
      <path d="M20.5 12a8.5 8.5 0 1 1-2.8-6.3" />
      <path d="M21 3.5v5.2h-5.2" />
    </>
  ),
  folder: (
    <path d="M3 7.4A2 2 0 0 1 5 5.4h3.4l2 2.4H19a2 2 0 0 1 2 2v7.2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7.4Z" />
  ),
  alert: (
    <>
      <path d="M12 4 2.6 20h18.8L12 4Z" />
      <path d="M12 10v4M12 17h.01" />
    </>
  ),
  check: <path d="m4.5 12.5 5 5L20 6.5" />,
  chevronDown: <path d="m6 9.5 6 6 6-6" />,
  chevronRight: <path d="m9.5 6 6 6-6 6" />,
  external: (
    <>
      <path d="M14 4h6v6M20 4l-8.5 8.5" />
      <path d="M18 14v5.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 19.5v-11A1.5 1.5 0 0 1 5.5 7H11" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="6.2" />
      <path d="m20 20-3.6-3.6" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6 6 18" />,
  terminal: <path d="m5 7 4.5 5L5 17M12.5 17h7" />,
  lock: (
    <>
      <rect x="4.5" y="10.5" width="15" height="10" rx="2.5" />
      <path d="M8.5 10.5V7.8a3.5 3.5 0 0 1 7 0v2.7" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  sparkle: (
    <path d="M12 3.5 13.6 9 19 10.6 13.6 12.2 12 17.7 10.4 12.2 5 10.6 10.4 9 12 3.5ZM18.5 16.5l.7 2.3 2.3.7-2.3.7-.7 2.3-.7-2.3-2.3-.7 2.3-.7.7-2.3Z" />
  ),
  play: <path d="M7.5 4.8v14.4L19.5 12 7.5 4.8Z" />,
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5.2M12 7.8h.01" />
    </>
  ),
  shield: <path d="M12 3.2l7 2.8v6c0 4.8-2.9 7.6-7 8.8-4.1-1.2-7-4-7-8.8v-6l7-2.8Z" />,
  cpu: (
    <>
      <rect x="6.5" y="6.5" width="11" height="11" rx="2.5" />
      <path d="M10 2.8v3.7M14 2.8v3.7M10 17.5v3.7M14 17.5v3.7M2.8 10h3.7M2.8 14h3.7M17.5 10h3.7M17.5 14h3.7" />
    </>
  ),
  link: (
    <>
      <path d="M10.2 13.8a3.8 3.8 0 0 0 5.4 0l2.9-2.9a3.8 3.8 0 0 0-5.4-5.4l-1.4 1.4" />
      <path d="M13.8 10.2a3.8 3.8 0 0 0-5.4 0l-2.9 2.9a3.8 3.8 0 0 0 5.4 5.4l1.4-1.4" />
    </>
  ),
  dot: <circle cx="12" cy="12" r="4.2" fill="currentColor" stroke="none" />,
  trash: (
    <>
      <path d="M4 6.8h16M9.5 3.8h5M6.2 6.8l1 13.2a1.2 1.2 0 0 0 1.2 1.1h7.2a1.2 1.2 0 0 0 1.2-1.1l1-13.2" />
      <path d="M10.2 10.8v6M13.8 10.8v6" />
    </>
  ),
};

interface IconProps {
  name: IconName;
  className?: string;
  strokeWidth?: number;
}

export function Icon({ name, className = "h-4 w-4", strokeWidth = 1.7 }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}