/** 全局任务控制台：折叠在底部，实时可视化长任务（扫描/安装/建环境）的过程输出。 */

import React, { useEffect, useRef, useState } from "react";
import { Icon } from "./Icon";
import { useApp } from "../lib/store";
import type { Progress } from "../lib/types";

const LEVEL_STYLE: Record<Progress["level"], string> = {
  info: "text-slate-400",
  warn: "text-amber-300",
  error: "text-rose-300",
  done: "text-brand-400",
};

const LEVEL_MARK: Record<Progress["level"], string> = {
  info: "·",
  warn: "!",
  error: "×",
  done: "✓",
};

export function TaskConsole() {
  const logs = useApp((s) => s.logs);
  const scanning = useApp((s) => s.scanning);
  const phase = useApp((s) => s.phase);
  const clearLogs = useApp((s) => s.clearLogs);
  const [open, setOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const last = logs[logs.length - 1];

  // 扫描开始自动展开，结束后保留面板供查看
  useEffect(() => {
    if (scanning) setOpen(true);
  }, [scanning]);

  useEffect(() => {
    if (open && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, open]);

  const progressPct =
    last && last.total > 0 ? Math.round((last.current / last.total) * 100) : scanning ? null : 100;

  return (
    <div className="shrink-0 border-t border-ink-700 bg-ink-900">
      {/* 状态条 */}
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-3 px-4 py-2 text-left transition-colors hover:bg-ink-850"
      >
        <span className="flex items-center gap-2 text-xs font-medium text-slate-300">
          <Icon name="terminal" className="h-3.5 w-3.5 text-slate-500" />
          任务控制台
        </span>

        {scanning ? (
          <span className="flex items-center gap-2 text-xs text-brand-400">
            <span className="h-1.5 w-1.5 animate-pulse-soft rounded-full bg-brand-500" />
            {phase || "执行中"}
          </span>
        ) : (
          <span className="text-xs text-slate-500">
            {logs.length > 0 ? `最近任务：${logs.length} 条日志` : "空闲"}
          </span>
        )}

        <span className="ml-auto flex items-center gap-3">
          {progressPct != null && last && last.total > 0 && (
            <span className="hidden items-center gap-2 sm:flex">
              <span className="h-1 w-28 overflow-hidden rounded-full bg-ink-800">
                <span
                  className="block h-full rounded-full bg-brand-500 transition-[width] duration-300"
                  style={{ width: `${progressPct}%` }}
                />
              </span>
              <span className="font-mono text-[10px] text-slate-500">
                {last.current}/{last.total}
              </span>
            </span>
          )}
          {last && (
            <span className="hidden max-w-md truncate text-[11px] text-slate-500 lg:block">
              {last.message}
            </span>
          )}
          <Icon
            name="chevronDown"
            className={`h-3.5 w-3.5 text-slate-500 transition-transform ${open ? "rotate-180" : ""}`}
          />
        </span>
      </button>

      {/* 日志面板 */}
      {open && (
        <div className="border-t border-ink-800">
          <div className="flex items-center justify-between px-4 py-1.5">
            <span className="text-[11px] text-slate-500">
              实时输出子进程与各阶段日志（stdout / 阶段事件）
            </span>
            <button
              type="button"
              onClick={clearLogs}
              className="rounded-md border border-ink-700 px-2 py-0.5 text-[11px] text-slate-400 hover:bg-ink-800 hover:text-slate-200"
            >
              清空
            </button>
          </div>
          <div ref={scrollRef} className="max-h-52 overflow-y-auto px-4 pb-3 font-mono text-[11px] leading-relaxed">
            {logs.length === 0 && (
              <p className="py-3 text-slate-600">暂无日志。点击「重新扫描本机」可查看完整执行过程。</p>
            )}
            {logs.map((log, index) => (
              <div key={index} className="flex gap-2 py-0.5">
                <span className="shrink-0 text-slate-600">{log.ts.slice(11)}</span>
                <span className="w-24 shrink-0 truncate text-slate-500">{log.phase}</span>
                <span className={`shrink-0 ${LEVEL_STYLE[log.level]}`}>{LEVEL_MARK[log.level]}</span>
                <span className={`min-w-0 flex-1 ${LEVEL_STYLE[log.level]}`}>{log.message}</span>
                {log.total > 0 && (
                  <span className="shrink-0 text-slate-600">
                    {log.current}/{log.total}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}