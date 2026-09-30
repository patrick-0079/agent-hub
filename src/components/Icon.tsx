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
  | "dot"
  | "cat";

/** 图标集：统一 24×24 网格 / 圆角端点 / 1.7 描边，几何规整（Lucide 风格比例） */
const PATHS: Record<IconName, React.ReactNode> = {
  dashboard: (
    <>
      <rect x="3" y="3" width="7.5" height="7.5" rx="1.8" />
      <rect x="13.5" y="3" width="7.5" height="7.5" rx="1.8" />
      <rect x="3" y="13.5" width="7.5" height="7.5" rx="1.8" />
      <rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1.8" />
    </>
  ),
  providers: (
    <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
  ),
  skills: (
    <>
      <path d="M9 18h6M10 21.5h4" />
      <path d="M15.1 14c.2-1 .7-1.8 1.4-2.5A4.6 4.6 0 0 0 18 8a6 6 0 0 0-12 0c0 1 .2 2.2 1.5 3.5.7.7 1.2 1.5 1.4 2.5" />
    </>
  ),
  mcp: (
    <>
      <circle cx="12" cy="12" r="2.6" />
      <circle cx="5" cy="5" r="1.9" />
      <circle cx="19" cy="5" r="1.9" />
      <circle cx="5" cy="19" r="1.9" />
      <circle cx="19" cy="19" r="1.9" />
      <path d="M6.4 6.4 9.5 9.5M17.6 6.4 14.5 9.5M6.4 17.6 9.5 14.5M17.6 17.6 14.5 14.5" />
    </>
  ),
  npm: (
    <>
      <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
      <path d="M3.3 7 12 12l8.7-5M12 22V12" />
    </>
  ),
  python: <path d="m9.5 8-5 4 5 4M14.5 8l5 4-5 4" />,
  profiles: (
    <>
      <path d="M12 2 2 7l10 5 10-5-10-5Z" />
      <path d="m2 17 10 5 10-5M2 12l10 5 10-5" />
    </>
  ),
  agents: (
    <>
      <rect x="5" y="8" width="14" height="12" rx="3" />
      <path d="M12 8V4.6" />
      <circle cx="12" cy="3.4" r="1.3" />
      <path d="M9.4 13.5h.01M14.6 13.5h.01M9.8 17h4.4" />
    </>
  ),
  adapter: (
    <>
      <path d="M3 7h18M3 17h18" />
      <circle cx="14.5" cy="7" r="2.4" />
      <circle cx="9" cy="17" r="2.4" />
    </>
  ),
  history: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7.5V12l3.2 2" />
    </>
  ),
  vault: (
    <>
      <rect x="4" y="10" width="16" height="11" rx="2.5" />
      <path d="M8 10V7.2a4 4 0 0 1 8 0V10M12 14.5v3" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3.2" />
      <circle cx="12" cy="12" r="8.4" />
      <path d="M12 1.6v2.4M12 20v2.4M1.6 12h2.4M20 12h2.4M4.6 4.6l1.7 1.7M17.7 17.7l1.7 1.7M4.6 19.4l1.7-1.7M17.7 6.3l1.7-1.7" />
    </>
  ),
  refresh: (
    <>
      <path d="M3 12a9 9 0 0 1 15.3-6.4L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-15.3 6.4L3 16" />
      <path d="M3 21v-5h5" />
    </>
  ),
  folder: (
    <path d="M3 7.4A2 2 0 0 1 5 5.4h3.6l2 2.4H19a2 2 0 0 1 2 2v7.2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7.4Z" />
  ),
  alert: (
    <>
      <path d="M10.3 3.9 1.9 18a2 2 0 0 0 1.7 3h16.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" />
      <path d="M12 9v4M12 16.5h.01" />
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
      <circle cx="11" cy="11" r="7" />
      <path d="m20.5 20.5-4.2-4.2" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6 6 18" />,
  terminal: <path d="m5 7 4.5 5L5 17M12.5 17H19" />,
  lock: (
    <>
      <rect x="3.5" y="11" width="17" height="10" rx="2.5" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  sparkle: (
    <path d="M12 4 14 9.5 19.5 11.5 14 13.5 12 19 10 13.5 4.5 11.5 10 9.5 12 4Z" />
  ),
  play: <path d="M8 5.4v13.2L19 12 8 5.4Z" />,
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 16v-4.5M12 8h.01" />
    </>
  ),
  shield: (
    <path d="M12 3 19 5.7v6.1c0 4.7-2.9 7.5-7 8.9-4.1-1.4-7-4.2-7-8.9V5.7L12 3Z" />
  ),
  cpu: (
    <>
      <rect x="6.5" y="6.5" width="11" height="11" rx="2.5" />
      <path d="M10 2.6v3.6M14 2.6v3.6M10 17.8v3.6M14 17.8v3.6M2.6 10h3.6M2.6 14h3.6M17.8 10h3.6M17.8 14h3.6" />
    </>
  ),
  link: (
    <>
      <path d="M10.2 13.8a3.8 3.8 0 0 0 5.4 0l2.9-2.9a3.8 3.8 0 0 0-5.4-5.4l-1.4 1.4" />
      <path d="M13.8 10.2a3.8 3.8 0 0 0-5.4 0l-2.9 2.9a3.8 3.8 0 0 0 5.4 5.4l1.4-1.4" />
    </>
  ),
  dot: <circle cx="12" cy="12" r="4.2" fill="currentColor" stroke="none" />,
  cat: (
    <>
      <path d="M4.5 11.5v2.9c0 3.6 3.2 6.1 7.5 6.1s7.5-2.5 7.5-6.1v-2.9" />
      <path d="M4.5 11.5 6 4l3.7 3.1M19.5 11.5 18 4l-3.7 3.1" />
      <circle cx="9.3" cy="13.2" r="0.95" fill="currentColor" stroke="none" />
      <circle cx="14.7" cy="13.2" r="0.95" fill="currentColor" stroke="none" />
      <path d="M12 15.4l-0.9 1.3h1.8L12 15.4Z" fill="currentColor" stroke="none" />
      <path d="M2.4 12.7l2.6.4M2.8 15.5l2.5-0.3M21.6 12.7l-2.6.4M21.2 15.5l-2.5-0.3" />
    </>
  ),
  trash: (
    <>
      <path d="M4 6.8h16M9.5 3.8h5M6.3 6.8l.9 12.9a1.4 1.4 0 0 0 1.4 1.3h6.8a1.4 1.4 0 0 0 1.4-1.3l.9-12.9" />
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
