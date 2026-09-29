/** 左侧导航：12 个功能页入口 + 本机状态与扫描入口。 */

import React from "react";
import { Icon, type IconName } from "./Icon";
import { useApp } from "../lib/store";
import { relativeTime } from "../lib/format";
import type { Route } from "../lib/types";

interface NavItem {
  route: Route;
  label: string;
  icon: IconName;
  badge?: number;
  tag?: string;
}

interface NavGroup {
  title: string;
  items: NavItem[];
}

export function Sidebar() {
  const route = useApp((s) => s.route);
  const navigate = useApp((s) => s.navigate);
  const host = useApp((s) => s.host);
  const snapshot = useApp((s) => s.snapshot);
  const scanning = useApp((s) => s.scanning);
  const scan = useApp((s) => s.scan);

  const installedAgents =
    snapshot?.agents.filter((a) => a.installed && a.kind !== "host").length ?? 0;

  const groups: NavGroup[] = [
    {
      title: "总览",
      items: [{ route: "dashboard", label: "仪表盘", icon: "dashboard" }],
    },
    {
      title: "资源",
      items: [
        {
          route: "providers",
          label: "模型供应商",
          icon: "providers",
          badge: snapshot?.providerHints.length,
        },
        { route: "skills", label: "Skills", icon: "skills", badge: snapshot?.skills.length },
        { route: "mcp", label: "MCP 服务器", icon: "mcp", badge: snapshot?.mcpServers.length },
        { route: "npm", label: "npm 包", icon: "npm", badge: snapshot?.npmPackages.length },
        {
          route: "python",
          label: "Python 环境",
          icon: "python",
          badge: snapshot?.pythonEnvs.length,
        },
      ],
    },
    {
      title: "环境与同步",
      items: [
        { route: "profiles", label: "环境档案", icon: "profiles" },
        { route: "agents", label: "Agent 目标", icon: "agents", badge: installedAgents },
        { route: "adapter", label: "Agent 定义与能力", icon: "adapter" },
      ],
    },
    {
      title: "系统",
      items: [
        { route: "history", label: "历史与审计", icon: "history" },
        { route: "trash", label: "回收站", icon: "trash" },
        { route: "vault", label: "密钥保险库", icon: "vault" },
        { route: "settings", label: "设置", icon: "settings" },
      ],
    },
  ];

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-ink-700 bg-ink-900">
      {/* 品牌 */}
      <div className="flex items-center gap-3 border-b border-ink-700 px-4 py-4">
        <span className="grid h-9 w-9 place-items-center rounded-md border border-brand-800 bg-brand-900 text-brand-400">
          <Icon name="mcp" className="h-5 w-5" />
        </span>
        <div>
          <div className="text-sm font-semibold tracking-wide text-slate-100">AgentHub</div>
          <div className="text-[11px] text-slate-500">统一 Agent 环境管理器</div>
        </div>
      </div>

      {/* 导航 */}
      <nav className="min-h-0 flex-1 overflow-y-auto px-2.5 py-3">
        {groups.map((group) => (
          <div key={group.title} className="mb-3">
            <div className="px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-slate-600">
              {group.title}
            </div>
            <div className="space-y-0.5">
              {group.items.map((item) => {
                const active = route === item.route;
                return (
                  <button
                    key={item.route}
                    type="button"
                    onClick={() => navigate(item.route)}
                    className={`nav-item ${
                      active
                        ? "border-brand-500 bg-brand-900 text-brand-400"
                        : "text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                    }`}
                  >
                    <Icon name={item.icon} className="h-4 w-4 shrink-0" />
                    <span className="flex-1 truncate">{item.label}</span>
                    {item.tag && (
                      <span className="rounded-sm border border-accent-800 bg-accent-900 px-1.5 py-0.5 font-mono text-[9px] text-accent-400">
                        {item.tag}
                      </span>
                    )}
                    {!item.tag && item.badge != null && item.badge > 0 && (
                      <span
                        className={`rounded-sm px-1.5 py-0.5 font-mono text-[10px] ${
                          active ? "bg-brand-800 text-brand-400" : "bg-ink-800 text-slate-500"
                        }`}
                      >
                        {item.badge}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </nav>

      {/* 底部：本机 + 扫描 */}
      <div className="border-t border-ink-700 px-3 py-3">
        <div className="mb-2.5 rounded-md border border-ink-700 bg-ink-850 px-3 py-2.5">
          <div className="flex items-center gap-2 text-xs text-slate-300">
            <Icon name="cpu" className="h-3.5 w-3.5 text-slate-500" />
            <span className="truncate font-medium">{host?.hostname ?? "未连接"}</span>
          </div>
          <div className="mt-1 flex items-center gap-2 text-[11px] text-slate-500">
            <span className="truncate">
              {host ? `${host.os} · ${host.arch} · v${host.appVersion}` : "—"}
            </span>
          </div>
          <div className="mt-1 text-[11px] text-slate-500">
            上次扫描：{relativeTime(snapshot?.scannedAt)}
          </div>
        </div>
        <button
          type="button"
          onClick={() => void scan()}
          disabled={scanning}
          className="btn-primary w-full"
        >
          <Icon
            name="refresh"
            className={`h-4 w-4 ${scanning ? "animate-spin" : ""}`}
            strokeWidth={2}
          />
          {scanning ? "扫描中…" : "重新扫描本机"}
        </button>
      </div>
    </aside>
  );
}