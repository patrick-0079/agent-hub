import React, { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { TaskConsole } from "./components/TaskConsole";
import { CommandPalette } from "./components/CommandPalette";
import { Icon, type IconName } from "./components/Icon";
import { api } from "./lib/api";
import { relativeTime } from "./lib/format";
import { useApp } from "./lib/store";
import type { Route } from "./lib/types";
import { Dashboard } from "./pages/Dashboard";
import { Onboarding } from "./pages/Onboarding";
import { Settings } from "./pages/Settings";
import { SkillsPage } from "./pages/Skills";
import { McpPage } from "./pages/McpPage";
import { NpmPage } from "./pages/NpmPackages";
import { PythonPage } from "./pages/PythonEnvs";
import { AgentsPage } from "./pages/Agents";
import { ProvidersPage } from "./pages/Providers";
import { HistoryPage } from "./pages/History";
import { TrashPage } from "./pages/Trash";
import { ProfilesPage } from "./pages/Profiles";
import { AgentDefsPage } from "./pages/AgentDefs";
import { VaultPage } from "./pages/Vault";

const PAGE_META: Record<Route, { title: string; subtitle: string; icon: IconName }> = {
  dashboard: {
    title: "仪表盘",
    subtitle: "本机 Agent 环境总览：已装目标、资源分布与工具链状态",
    icon: "dashboard",
  },
  providers: {
    title: "模型供应商",
    subtitle: "集中管理 API Key、Base URL 与模型清单，测试连通性",
    icon: "providers",
  },
  skills: { title: "Skills", subtitle: "统一管理 Skill 库并按需分发到各 Agent", icon: "skills" },
  mcp: { title: "MCP 服务器", subtitle: "集中维护 MCP 配置，检查运行时依赖", icon: "mcp" },
  npm: { title: "npm 包", subtitle: "全局工具清单与声明式安装", icon: "npm" },
  python: { title: "Python 环境", subtitle: "conda / uv / venv 环境的统一视图", icon: "python" },
  profiles: { title: "Profiles", subtitle: "把资源组合成环境档案，一键分发", icon: "profiles" },
  agents: { title: "Agent 目标", subtitle: "各 Agent 的配置位置、已同步资源与适配器状态", icon: "agents" },
  adapter: {
    title: "Agent 定义与能力",
    subtitle: "内核只提供分级基础能力；Agent 的位置、格式与授权全部来自可编辑的定义文件",
    icon: "adapter",
  },
  history: { title: "历史与审计", subtitle: "扫描记录、可恢复操作清单", icon: "history" },
  trash: {
    title: "回收站",
    subtitle: "删除的链接与目录都先到这里：可逐条或按对象恢复，永久删除不可撤销",
    icon: "trash",
  },
  vault: { title: "密钥保险库", subtitle: "API Key 加密存储，明文永不落库", icon: "vault" },
  settings: { title: "设置", subtitle: "工具链探测、扫描范围与外观", icon: "settings" },
};

const PAGE_COMPONENTS: Record<Route, React.ComponentType> = {
  dashboard: Dashboard,
  providers: ProvidersPage,
  skills: SkillsPage,
  mcp: McpPage,
  npm: NpmPage,
  python: PythonPage,
  profiles: ProfilesPage,
  agents: AgentsPage,
  adapter: AgentDefsPage,
  history: HistoryPage,
  trash: TrashPage,
  vault: VaultPage,
  settings: Settings,
};

export default function App() {
  const ready = useApp((s) => s.ready);
  const route = useApp((s) => s.route);
  const settings = useApp((s) => s.settings);
  const snapshot = useApp((s) => s.snapshot);
  const scanning = useApp((s) => s.scanning);
  const banner = useApp((s) => s.banner);
  const host = useApp((s) => s.host);
  const scan = useApp((s) => s.scan);
  const setBanner = useApp((s) => s.setBanner);
  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    void useApp.getState().init();
  }, []);

  // Ctrl+K / Cmd+K 打开命令面板
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (ready) void api.frontendReady();
  }, [ready]);

  if (!ready) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-slate-400">
        <Icon name="cat" className="h-9 w-9 animate-pulse-soft text-brand-400" />
        <p className="text-sm">正在连接 AgentHub 后端…</p>
      </div>
    );
  }

  if (settings && !settings.onboardingDone) {
    return <Onboarding />;
  }

  const meta = PAGE_META[route];
  const Page = PAGE_COMPONENTS[route];

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        {/* 顶栏 */}
        <header className="flex shrink-0 items-center gap-4 border-b border-ink-700 bg-ink-900 px-6 py-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold text-slate-100">{meta.title}</h1>
            <p className="truncate text-xs text-slate-500">{meta.subtitle}</p>
          </div>
          <div className="ml-auto flex items-center gap-3">
            {host && (
              <span className="hidden font-mono text-[11px] text-slate-500 lg:block">
                v{host.appVersion}
              </span>
            )}
            <button
              type="button"
              onClick={() => setPaletteOpen(true)}
              className="btn-ghost btn-sm hidden md:flex"
              title="命令面板（Ctrl+K）"
            >
              <Icon name="search" className="h-3.5 w-3.5" />
              搜索
              <span className="kbd ml-1">Ctrl K</span>
            </button>
            <span className="hidden text-[11px] text-slate-500 md:block">
              上次扫描 {relativeTime(snapshot?.scannedAt)}
            </span>
            <button
              type="button"
              onClick={() => void scan()}
              disabled={scanning}
              className="btn-ghost btn-sm"
            >
              <Icon
                name="refresh"
                className={`h-3.5 w-3.5 ${scanning ? "animate-spin" : ""}`}
                strokeWidth={2}
              />
              {scanning ? "扫描中" : "扫描"}
            </button>
          </div>
        </header>

        {/* 提示条 */}
        {banner && (
          <div className="flex shrink-0 items-center gap-3 border-b border-amber-800/70 px-6 py-2.5 text-xs text-amber-300">
            <Icon name="alert" className="h-4 w-4 shrink-0" />
            <span className="min-w-0 flex-1">{banner}</span>
            {!snapshot && (
              <button
                type="button"
                onClick={() => void scan()}
                className="rounded-sm border border-amber-700 px-2 py-0.5 hover:bg-amber-900"
              >
                重试扫描
              </button>
            )}
            <button
              type="button"
              onClick={() => setBanner(null)}
              className="rounded-sm p-1 hover:bg-amber-900"
            >
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
        )}

        {/* 内容区 */}
        <main className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          <div>
            <Page />
          </div>
        </main>

        <TaskConsole />
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </div>
  );
}