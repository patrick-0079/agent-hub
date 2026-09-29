/** 命令面板（M4）：Ctrl+K 全局搜索直达任意页面与常用操作。 */

import React, { useEffect, useMemo, useRef, useState } from "react";
import { Icon, type IconName } from "./Icon";
import { useApp } from "../lib/store";
import type { Route } from "../lib/types";

interface Command {
  id: string;
  title: string;
  subtitle?: string;
  icon: IconName;
  group: "页面" | "操作";
  run: () => void;
}

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useApp((s) => s.navigate);
  const route = useApp((s) => s.route);
  const scan = useApp((s) => s.scan);
  const scanning = useApp((s) => s.scanning);

  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // 打开时重置状态并聚焦
  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
      window.setTimeout(() => inputRef.current?.focus(), 10);
    }
  }, [open]);

  const commands = useMemo<Command[]>(() => {
    const page = (id: Route, title: string, subtitle: string, icon: IconName): Command => ({
      id: `page:${id}`,
      title,
      subtitle,
      icon,
      group: "页面",
      run: () => navigate(id),
    });
    return [
      page("dashboard", "仪表盘", "本机 Agent 环境总览", "dashboard"),
      page("providers", "模型供应商", "API Key / Base URL / 连通性测试", "providers"),
      page("skills", "Skills", "统一技能库管理与分发", "skills"),
      page("mcp", "MCP 服务器", "资源库 / 握手检查 / 分发", "mcp"),
      page("npm", "npm 包", "全局工具清单", "npm"),
      page("python", "Python 环境", "conda / uv / venv 视图", "python"),
      page("profiles", "环境档案", "组合资源一键应用 / 导出导入", "profiles"),
      page("agents", "Agent 目标", "配置位置与同步状态", "agents"),
      page("adapter", "Agent 定义与能力", "能力目录与定义编辑", "adapter"),
      page("history", "历史与审计", "操作清单 / 备份 / 快照对比", "history"),
      page("trash", "回收站", "删除内容恢复与清理", "trash"),
      page("vault", "密钥保险库", "DPAPI 加密密钥状态", "vault"),
      page("settings", "设置", "工具链探测 / 扫描范围 / 代理", "settings"),
      {
        id: "action:scan",
        title: scanning ? "扫描进行中…" : "重新扫描本机",
        subtitle: "只读侦察：Agent / Skill / MCP / Python / npm",
        icon: "refresh",
        group: "操作",
        run: () => {
          if (!scanning) void scan();
        },
      },
    ];
  }, [navigate, scan, scanning]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        (c.subtitle ?? "").toLowerCase().includes(q) ||
        c.id.toLowerCase().includes(q),
    );
  }, [commands, query]);

  // 光标跟随键盘
  useEffect(() => {
    if (cursor >= filtered.length) setCursor(Math.max(0, filtered.length - 1));
  }, [filtered.length, cursor]);

  if (!open) return null;

  const activate = (cmd: Command) => {
    cmd.run();
    onClose();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(filtered.length - 1, c + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(0, c - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = filtered[cursor];
      if (cmd) activate(cmd);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  // 滚动让选中项可见
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${cursor}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  let lastGroup = "";

  return (
    <div className="fixed inset-0 z-[60] flex items-start justify-center pt-[12vh]">
      <div className="overlay absolute inset-0" onClick={onClose} />
      <div className="relative flex w-full max-w-xl animate-fade-in flex-col overflow-hidden rounded-md border border-ink-700 bg-ink-900">
        <div className="flex items-center gap-2 border-b border-ink-700 px-3 py-2.5">
          <Icon name="search" className="h-4 w-4 shrink-0 text-slate-500" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            onKeyDown={onKeyDown}
            placeholder="搜索页面或操作…（↑↓ 选择，Enter 直达，Esc 关闭）"
            className="w-full bg-transparent text-sm text-slate-200 outline-none placeholder:text-slate-500"
          />
          <span className="kbd shrink-0">Esc</span>
        </div>
        <div ref={listRef} className="max-h-[52vh] overflow-auto p-1.5">
          {filtered.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-slate-500">
              没有匹配「{query}」的命令
            </p>
          ) : (
            filtered.map((cmd, idx) => {
              const showGroup = cmd.group !== lastGroup;
              lastGroup = cmd.group;
              const active = idx === cursor;
              const current = cmd.id === `page:${route}`;
              return (
                <React.Fragment key={cmd.id}>
                  {showGroup && (
                    <div className="px-2.5 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wider text-slate-600">
                      {cmd.group}
                    </div>
                  )}
                  <button
                    type="button"
                    data-idx={idx}
                    onMouseEnter={() => setCursor(idx)}
                    onClick={() => activate(cmd)}
                    className={`flex w-full items-center gap-3 rounded-sm px-2.5 py-2 text-left ${
                      active ? "bg-brand-900 text-brand-400" : "text-slate-300 hover:bg-ink-800"
                    }`}
                  >
                    <Icon name={cmd.icon} className="h-4 w-4 shrink-0" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm">{cmd.title}</span>
                      {cmd.subtitle && (
                        <span className="block truncate text-[11px] text-slate-500">
                          {cmd.subtitle}
                        </span>
                      )}
                    </span>
                    {current && (
                      <span className="shrink-0 text-[10px] uppercase tracking-wide text-slate-600">
                        当前
                      </span>
                    )}
                    {active && <span className="kbd shrink-0">Enter</span>}
                  </button>
                </React.Fragment>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
