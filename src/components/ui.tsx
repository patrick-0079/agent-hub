/** 通用可视化组件：卡片、状态灯、徽章、进度条、空态、路径行、抽屉、计划页。 */

import React, { useState } from "react";
import { Icon, type IconName } from "./Icon";

/* ------------------------------------------------------------------ 容器 */

export function Card({
  children,
  className = "",
  padded = true,
}: {
  children: React.ReactNode;
  className?: string;
  padded?: boolean;
}) {
  return <div className={`card ${padded ? "p-5" : ""} ${className}`}>{children}</div>;
}

export function SectionCard({
  title,
  subtitle,
  action,
  children,
  className = "",
  bodyClassName = "",
}: {
  title: string;
  subtitle?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClassName?: string;
}) {
  return (
    <section className={`card flex flex-col overflow-hidden ${className}`}>
      <header className="flex items-start justify-between gap-4 border-b border-ink-700/60 px-5 py-3.5">
        <div>
          <h2 className="text-sm font-semibold text-slate-100">{title}</h2>
          {subtitle && <p className="mt-0.5 text-xs text-slate-500">{subtitle}</p>}
        </div>
        {action}
      </header>
      <div className={`min-h-0 flex-1 overflow-auto px-5 py-4 ${bodyClassName}`}>{children}</div>
    </section>
  );
}

/* ------------------------------------------------------------------ 状态 */

export type Tone = "teal" | "violet" | "amber" | "rose" | "slate" | "sky";

/** 扁平色调：实底暗色 + 同系边框 + 同系文字，无阴影无发光 */
const TONE_CLASS: Record<Tone, string> = {
  teal: "border-brand-800 bg-brand-900 text-brand-400",
  violet: "border-accent-800 bg-accent-900 text-accent-400",
  amber: "border-amber-800/70 bg-amber-950 text-amber-300",
  rose: "border-rose-800/70 bg-rose-950 text-rose-300",
  slate: "border-ink-700 bg-ink-800 text-slate-400",
  sky: "border-sky-800/70 bg-sky-950 text-sky-300",
};

export function Badge({
  children,
  tone = "slate",
  icon,
  className = "",
}: {
  children: React.ReactNode;
  tone?: Tone;
  icon?: IconName;
  className?: string;
}) {
  return (
    <span className={`chip ${TONE_CLASS[tone]} ${className}`}>
      {icon && <Icon name={icon} className="h-3 w-3" />}
      {children}
    </span>
  );
}

export type DotState = "ok" | "warn" | "error" | "idle";

const DOT_CLASS: Record<DotState, string> = {
  ok: "bg-brand-500",
  warn: "bg-amber-400",
  error: "bg-rose-500",
  idle: "bg-ink-500",
};

export function StatusDot({ state, className = "" }: { state: DotState; className?: string }) {
  return <span className={`inline-block h-2 w-2 shrink-0 rounded-full ${DOT_CLASS[state]} ${className}`} />;
}

/* ---------------------------------------------------------------- 进度条 */

export function ProgressBar({
  value,
  max,
  label,
  tone = "teal",
  indeterminate = false,
}: {
  value: number;
  max: number;
  label?: string;
  tone?: Tone;
  indeterminate?: boolean;
}) {
  const pct = max > 0 ? Math.min(100, Math.round((value / max) * 100)) : indeterminate ? 100 : 0;
  const barColor =
    tone === "rose" ? "bg-rose-500" : tone === "amber" ? "bg-amber-400" : "bg-brand-500";
  return (
    <div>
      {label && (
        <div className="mb-1 flex items-center justify-between text-xs text-slate-400">
          <span>{label}</span>
          {!indeterminate && max > 0 && <span className="font-mono">{pct}%</span>}
        </div>
      )}
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-ink-800">
        {indeterminate ? (
          <div className="h-full w-1/3 animate-sweep rounded-full bg-brand-500" />
        ) : (
          <div
            className={`h-full rounded-full transition-[width] duration-300 ${barColor}`}
            style={{ width: `${pct}%` }}
          />
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ KPI */

export function Kpi({
  label,
  value,
  unit,
  hint,
  icon,
  tone = "teal",
  onClick,
}: {
  label: string;
  value: React.ReactNode;
  unit?: string;
  hint?: string;
  icon: IconName;
  tone?: Tone;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!onClick}
      className={`card group flex items-start gap-3.5 p-4 text-left transition-colors ${
        onClick ? "hover:border-ink-600 hover:bg-ink-800" : "cursor-default"
      }`}
    >
      <span className={`mt-0.5 rounded-md border p-2 ${TONE_CLASS[tone]}`}>
        <Icon name={icon} className="h-4 w-4" />
      </span>
      <span className="min-w-0">
        <span className="block text-[11px] font-medium uppercase tracking-wide text-slate-500">
          {label}
        </span>
        <span className="mt-0.5 block text-2xl font-semibold leading-tight text-slate-50">
          {value}
          {unit && <span className="ml-1 text-xs font-normal text-slate-500">{unit}</span>}
        </span>
        {hint && <span className="mt-0.5 block truncate text-[11px] text-slate-500">{hint}</span>}
      </span>
    </button>
  );
}

/* ------------------------------------------------------------------ 空态 */

export function Empty({
  icon = "search",
  title,
  description,
  action,
}: {
  icon?: IconName;
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
      <span className="rounded-md border border-ink-700 bg-ink-800 p-3 text-slate-500">
        <Icon name={icon} className="h-5 w-5" />
      </span>
      <p className="text-sm font-medium text-slate-300">{title}</p>
      {description && <p className="max-w-md text-xs leading-relaxed text-slate-500">{description}</p>}
      {action}
    </div>
  );
}

/* ---------------------------------------------------------------- 路径行 */

export function useCopy(): [string | null, (text: string) => void] {
  const [copied, setCopied] = useState<string | null>(null);
  const copy = (text: string) => {
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(text);
        window.setTimeout(() => setCopied(null), 1400);
      })
      .catch(() => setCopied(null));
  };
  return [copied, copy];
}

export function PathRow({
  label,
  path,
  exists,
  size,
  onReveal,
  compact = false,
}: {
  label?: string;
  path: string;
  exists: boolean;
  size?: number | null;
  onReveal?: (path: string) => void;
  compact?: boolean;
}) {
  const [copied, copy] = useCopy();
  return (
    <div className="flex items-center gap-3 py-2">
      <StatusDot state={exists ? "ok" : "idle"} />
      <div className="min-w-0 flex-1">
        {label && <div className="text-xs font-medium text-slate-300">{label}</div>}
        <div className={`mono truncate ${exists ? "text-slate-400" : "text-slate-600"}`} title={path}>
          {path}
        </div>
      </div>
      {size != null && size > 0 && (
        <span className="hidden shrink-0 font-mono text-[11px] text-slate-500 sm:block">
          {Math.max(1, Math.round(size / 1024))} KB
        </span>
      )}
      {!compact && (
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={() => copy(path)}
            className="rounded-md border border-ink-700 px-2 py-1 text-[11px] text-slate-400 transition-colors hover:bg-ink-800 hover:text-slate-200"
          >
            {copied === path ? "已复制" : "复制"}
          </button>
          {onReveal && exists && (
            <button
              type="button"
              onClick={() => onReveal(path)}
              className="rounded-md border border-ink-700 p-1.5 text-slate-400 transition-colors hover:bg-ink-800 hover:text-slate-200"
              title="在资源管理器中打开"
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ 弹窗 */

export function Modal({
  open,
  onClose,
  title,
  subtitle,
  children,
  footer,
  width = "max-w-3xl",
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  footer?: React.ReactNode;
  width?: string;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <div className="overlay absolute inset-0" onClick={onClose} />
      <div
        className={`relative flex max-h-[88vh] w-full ${width} animate-fade-in flex-col overflow-hidden rounded-md border border-ink-700 bg-ink-900`}
      >
        <header className="flex items-start justify-between gap-4 border-b border-ink-700 px-5 py-4">
          <div className="min-w-0">
            <h3 className="truncate text-base font-semibold text-slate-100">{title}</h3>
            {subtitle && <p className="mono mt-0.5 truncate">{subtitle}</p>}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
          >
            <Icon name="close" className="h-4 w-4" />
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">{children}</div>
        {footer && (
          <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-ink-700 px-5 py-3">
            {footer}
          </footer>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ 抽屉 */

export function Drawer({
  open,
  onClose,
  title,
  subtitle,
  children,
  width = "max-w-2xl",
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  width?: string;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <div className="overlay absolute inset-0" onClick={onClose} />
      <div
        className={`relative flex h-full w-full ${width} animate-fade-in flex-col border-l border-ink-700 bg-ink-900`}
      >
        <header className="flex items-start justify-between gap-4 border-b border-ink-700 px-5 py-4">
          <div className="min-w-0">
            <h3 className="truncate text-base font-semibold text-slate-100">{title}</h3>
            {subtitle && <p className="mono mt-0.5 truncate">{subtitle}</p>}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
          >
            <Icon name="close" className="h-4 w-4" />
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">{children}</div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ 控件 */

export function Toggle({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <label
      className={`flex items-start justify-between gap-4 py-2.5 ${
        disabled ? "opacity-50" : "cursor-pointer"
      }`}
    >
      <span>
        <span className="block text-sm text-slate-200">{label}</span>
        {hint && <span className="mt-0.5 block text-xs text-slate-500">{hint}</span>}
      </span>
      <button
        type="button"
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative mt-0.5 h-5 w-9 shrink-0 rounded-full border transition-colors ${
          checked ? "border-brand-800 bg-brand-500" : "border-ink-600 bg-ink-800"
        }`}
      >
        <span
          className={`absolute top-0.5 h-3.5 w-3.5 rounded-full transition-all ${
            checked ? "left-[18px] bg-ink-950" : "left-0.5 bg-slate-500"
          }`}
        />
      </button>
    </label>
  );
}

export function SearchInput({
  value,
  onChange,
  placeholder = "搜索…",
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="relative">
      <Icon
        name="search"
        className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-500"
      />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="input pl-9"
      />
    </div>
  );
}

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string; count?: number }[];
  onChange: (next: T) => void;
}) {
  return (
    <div className="inline-flex rounded-md border border-ink-700 bg-ink-900 p-0.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          onClick={() => onChange(opt.value)}
          className={`rounded-sm px-3 py-1 text-xs font-medium transition-colors ${
            value === opt.value
              ? "bg-ink-800 text-slate-100"
              : "text-slate-400 hover:text-slate-200"
          }`}
        >
          {opt.label}
          {opt.count != null && (
            <span className="ml-1.5 font-mono text-[10px] text-slate-500">{opt.count}</span>
          )}
        </button>
      ))}
    </div>
  );
}

/* -------------------------------------------------------------- 计划中页 */

export function PlannedPage({
  icon,
  title,
  milestone,
  summary,
  bullets,
}: {
  icon: IconName;
  title: string;
  milestone: string;
  summary: string;
  bullets: { title: string; detail: string }[];
}) {
  return (
    <div className="mx-auto max-w-4xl space-y-4">
      <Card className="border-dashed">
        <div className="flex items-start gap-4">
          <span className="rounded-md border border-accent-800 bg-accent-900 p-3 text-accent-400">
            <Icon name={icon} className="h-5 w-5" />
          </span>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold text-slate-100">{title}</h2>
              <Badge tone="violet" icon="sparkle">
                {milestone}
              </Badge>
            </div>
            <p className="mt-1.5 text-sm leading-relaxed text-slate-400">{summary}</p>
          </div>
        </div>
      </Card>

      <div className="grid gap-3 sm:grid-cols-2">
        {bullets.map((b) => (
          <Card key={b.title} className="border-ink-700/60">
            <div className="flex items-start gap-3">
              <span className="mt-0.5 rounded-md border border-ink-600 bg-ink-800 p-1.5 text-slate-400">
                <Icon name="check" className="h-3.5 w-3.5" />
              </span>
              <div>
                <p className="text-sm font-medium text-slate-200">{b.title}</p>
                <p className="mt-0.5 text-xs leading-relaxed text-slate-500">{b.detail}</p>
              </div>
            </div>
          </Card>
        ))}
      </div>

      <p className="text-center text-xs text-slate-600">
        本页为规划中的能力，界面骨架已就位，将在对应里程碑中接入真实数据与操作。
      </p>
    </div>
  );
}