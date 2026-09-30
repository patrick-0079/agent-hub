/** 首次启动引导：欢迎 → 扫描本机（实时可视化）→ 结果总览 → Profile 预告 → 完成。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon, type IconName } from "../components/Icon";
import { Badge, Card, ProgressBar } from "../components/ui";
import { formatDuration } from "../lib/format";
import { useApp } from "../lib/store";

const PHASES = [
  "环境探测",
  "Agent 发现",
  "MCP 配置",
  "Skills",
  "Python 环境",
  "npm 全局包",
  "供应商线索",
  "完成",
];

const CAPABILITIES: { icon: IconName; title: string; detail: string }[] = [
  { icon: "providers", title: "模型供应商", detail: "API Key / Base URL / 模型清单集中管理，可测连通性" },
  { icon: "skills", title: "Skills", detail: "统一 Skill 库，按需拷贝或链接到各 Agent 目录" },
  { icon: "mcp", title: "MCP 服务器", detail: "一次配置多处生效，避免每个 Agent 各写一遍" },
  { icon: "npm", title: "npm 包", detail: "全局工具清单化，声明式安装与版本锁定" },
  { icon: "python", title: "Python 环境", detail: "conda / uv / venv 统一视图与创建向导" },
  { icon: "adapter", title: "自定义 Agent", detail: "模版语法描述任意 Agent 的配置格式" },
];

export function Onboarding() {
  const [step, setStep] = useState(0);
  const snapshot = useApp((s) => s.snapshot);
  const scanning = useApp((s) => s.scanning);
  const logs = useApp((s) => s.logs);
  const scan = useApp((s) => s.scan);
  const patchSettings = useApp((s) => s.patchSettings);

  // 进入扫描步骤即开始扫描
  useEffect(() => {
    if (step === 1 && !scanning) void scan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  const lastLog = logs[logs.length - 1];
  const currentPhaseIndex = useMemo(() => {
    if (!lastLog) return -1;
    return PHASES.indexOf(lastLog.phase);
  }, [lastLog]);

  const overall = useMemo(() => {
    if (!logs.length) return 0;
    if (lastLog?.phase === "完成") return 100;
    const idx = Math.max(currentPhaseIndex, 0);
    const inner = lastLog && lastLog.total > 0 ? lastLog.current / lastLog.total : 0;
    return Math.min(99, Math.round(((idx + inner) / PHASES.length) * 100));
  }, [logs.length, currentPhaseIndex, lastLog]);

  const installed = snapshot?.agents.filter((a) => a.installed) ?? [];

  const finish = () => void patchSettings({ onboardingDone: true });

  const steps = ["欢迎", "扫描本机", "发现结果", "Profile 预告", "完成"];

  return (
    <div className="flex h-full flex-col bg-ink-950">
      {/* 顶部步骤条 */}
      <header className="flex shrink-0 items-center gap-4 border-b border-ink-700 px-8 py-4">
        <div className="flex items-center gap-3">
          <span className="grid h-9 w-9 place-items-center rounded-md border border-brand-500/40 text-brand-400">
            <Icon name="mcp" className="h-5 w-5" />
          </span>
          <div>
            <div className="text-sm font-semibold text-slate-100">AgentHub 初始设置</div>
            <div className="text-[11px] text-slate-500">让一台电脑上的多个 Agent 共用同一套环境</div>
          </div>
        </div>

        <div className="mx-auto flex items-center gap-2">
          {steps.map((label, i) => (
            <React.Fragment key={label}>
              <div className="flex items-center gap-2">
                <span
                  className={`grid h-6 w-6 place-items-center rounded-full border text-[11px] font-semibold ${
                    i < step
                      ? "border-brand-500 text-brand-400"
                      : i === step
                        ? "border-brand-500 bg-brand-500 text-ink-950 on-accent"
                        : "border-ink-600 text-slate-500"
                  }`}
                >
                  {i < step ? <Icon name="check" className="h-3 w-3" strokeWidth={3} /> : i + 1}
                </span>
                <span className={`text-xs ${i === step ? "text-slate-200" : "text-slate-500"}`}>
                  {label}
                </span>
              </div>
              {i < steps.length - 1 && <span className="h-px w-8 bg-ink-700" />}
            </React.Fragment>
          ))}
        </div>

        <button type="button" onClick={finish} className="btn-ghost btn-sm">
          跳过引导
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-8 py-8">
        <div className="mx-auto max-w-5xl">
          {/* 步骤 0：欢迎 */}
          {step === 0 && (
            <div className="space-y-6">
              <div className="text-center">
                <Badge tone="teal" icon="sparkle">
                  只读扫描 · 不会修改任何文件
                </Badge>
                <h1 className="mt-4 text-3xl font-semibold tracking-tight text-slate-50">
                  先看清这台电脑上有什么
                </h1>
                <p className="mx-auto mt-3 max-w-2xl text-sm leading-relaxed text-slate-400">
                  AgentHub 会把散落在各个 Agent 里的模型供应商配置、Skill、MCP 服务器、npm
                  工具与 Python 环境统一管理，再按需分发回每个 Agent。第一步先做一次全盘侦察。
                </p>
              </div>

              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {CAPABILITIES.map((c) => (
                  <Card key={c.title} className="border-ink-700">
                    <div className="flex items-start gap-3">
                      <span className="text-brand-400">
                        <Icon name={c.icon} className="h-4 w-4" />
                      </span>
                      <div>
                        <p className="text-sm font-medium text-slate-200">{c.title}</p>
                        <p className="mt-0.5 text-xs leading-relaxed text-slate-500">{c.detail}</p>
                      </div>
                    </div>
                  </Card>
                ))}
              </div>

              <div className="flex justify-center pt-2">
                <button type="button" onClick={() => setStep(1)} className="btn-primary px-6 py-2.5">
                  <Icon name="play" className="h-4 w-4" />
                  开始扫描本机
                </button>
              </div>
            </div>
          )}

          {/* 步骤 1：扫描 */}
          {step === 1 && (
            <div className="space-y-5">
              <div className="text-center">
                <h1 className="text-2xl font-semibold text-slate-50">正在扫描本机环境</h1>
                <p className="mt-2 text-sm text-slate-400">
                  逐个探测 Agent 配置目录、MCP 配置、Skill 目录、Python 环境与全局包
                </p>
              </div>

              <Card>
                <ProgressBar
                  value={overall}
                  max={100}
                  label={lastLog ? `${lastLog.phase} · ${lastLog.message}` : "准备中…"}
                  indeterminate={!logs.length}
                />
                <div className="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                  {PHASES.map((phase, i) => {
                    const done = i < currentPhaseIndex || lastLog?.phase === "完成";
                    const active = i === currentPhaseIndex && lastLog?.phase !== "完成";
                    return (
                      <div
                        key={phase}
                        className={`flex items-center gap-2 rounded-lg border px-3 py-2 text-xs transition-colors ${
                          done
                            ? "border-brand-500/30 text-brand-400"
                            : active
                              ? "border-accent-500/40 text-accent-400"
                              : "border-ink-800 bg-ink-900 text-slate-600"
                        }`}
                      >
                        {done ? (
                          <Icon name="check" className="h-3.5 w-3.5" strokeWidth={2.5} />
                        ) : active ? (
                          <span className="h-2 w-2 animate-pulse-soft rounded-full bg-accent-400" />
                        ) : (
                          <span className="h-2 w-2 rounded-full bg-ink-600" />
                        )}
                        {phase}
                      </div>
                    );
                  })}
                </div>
              </Card>

              <Card padded={false} className="overflow-hidden">
                <div className="border-b border-ink-700 px-4 py-2 text-xs text-slate-400">
                  扫描日志
                </div>
                <div className="max-h-48 overflow-y-auto px-4 py-3 font-mono text-[11px] leading-relaxed">
                  {logs.length === 0 && <p className="text-slate-600">等待扫描启动…</p>}
                  {logs.slice(-40).map((log, i) => (
                    <div key={i} className="flex gap-2">
                      <span className="shrink-0 text-slate-600">{log.ts.slice(11)}</span>
                      <span
                        className={
                          log.level === "error"
                            ? "text-rose-300"
                            : log.level === "warn"
                              ? "text-amber-300"
                              : log.level === "done"
                                ? "text-brand-400"
                                : "text-slate-400"
                        }
                      >
                        {log.message}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>

              <div className="flex justify-center">
                <button
                  type="button"
                  onClick={() => setStep(2)}
                  disabled={scanning || !snapshot}
                  className="btn-primary px-6"
                >
                  {scanning ? "扫描中…" : "查看发现结果"}
                  <Icon name="chevronRight" className="h-4 w-4" />
                </button>
              </div>
            </div>
          )}

          {/* 步骤 2：结果 */}
          {step === 2 && snapshot && (
            <div className="space-y-5">
              <div className="text-center">
                <Badge tone="teal" icon="check">
                  扫描完成 · 耗时 {formatDuration(snapshot.durationMs)}
                </Badge>
                <h1 className="mt-3 text-2xl font-semibold text-slate-50">
                  在这台电脑上发现了这些
                </h1>
                <p className="mt-2 text-sm text-slate-400">
                  {snapshot.host.hostname} · {snapshot.host.os} · {snapshot.host.arch}
                </p>
              </div>

              <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
                {[
                  { label: "Agent 目标", value: `${installed.length}/${snapshot.agents.length}`, icon: "agents" as IconName },
                  { label: "Skills", value: snapshot.skills.length, icon: "skills" as IconName },
                  { label: "MCP 服务器", value: snapshot.mcpServers.length, icon: "mcp" as IconName },
                  { label: "Python 环境", value: snapshot.pythonEnvs.length, icon: "python" as IconName },
                  { label: "npm 全局包", value: snapshot.npmPackages.length, icon: "npm" as IconName },
                ].map((item) => (
                  <Card key={item.label} className="text-center">
                    <Icon name={item.icon} className="mx-auto h-5 w-5 text-brand-400" />
                    <div className="mt-2 text-2xl font-semibold text-slate-50">{item.value}</div>
                    <div className="text-[11px] uppercase tracking-wide text-slate-500">
                      {item.label}
                    </div>
                  </Card>
                ))}
              </div>

              <div className="grid gap-4 lg:grid-cols-2">
                <Card>
                  <h3 className="text-sm font-semibold text-slate-100">已安装的 Agent</h3>
                  <div className="mt-3 space-y-2">
                    {installed.map((agent) => (
                      <div
                        key={agent.id}
                        className="flex items-center gap-3 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2"
                      >
                        <span className="h-6 w-1 rounded" style={{ background: agent.accent }} />
                        <span className="flex-1 truncate text-sm text-slate-200">{agent.name}</span>
                        <span className="text-[11px] text-slate-500">
                          MCP {agent.mcpCount} · Skill {agent.skillCount}
                        </span>
                      </div>
                    ))}
                    {installed.length === 0 && (
                      <p className="text-xs text-slate-500">未发现已安装的 Agent。</p>
                    )}
                  </div>
                </Card>

                <Card>
                  <h3 className="text-sm font-semibold text-slate-100">值得关注</h3>
                  <div className="mt-3 space-y-2 text-xs">
                    {snapshot.warnings.length === 0 ? (
                      <div className="flex items-center gap-2 rounded-lg border border-brand-500/30 px-3 py-2 text-brand-400">
                        <Icon name="check" className="h-3.5 w-3.5" />
                        未发现异常
                      </div>
                    ) : (
                      snapshot.warnings.slice(0, 4).map((w, i) => (
                        <div
                          key={i}
                          className="flex items-start gap-2 rounded-lg border border-amber-500/25 px-3 py-2 text-amber-200"
                        >
                          <Icon name="alert" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                          <span className="leading-relaxed">{w}</span>
                        </div>
                      ))
                    )}
                    <div className="rounded-lg border border-ink-800 bg-ink-900 px-3 py-2 text-slate-400">
                      工具链：{snapshot.executables.filter((e) => e.found).length}/
                      {snapshot.executables.length} 个组件已就绪
                    </div>
                  </div>
                </Card>
              </div>

              <div className="flex justify-center gap-3">
                <button type="button" className="btn-ghost" onClick={() => void scan()}>
                  <Icon name="refresh" className="h-4 w-4" />
                  重新扫描
                </button>
                <button type="button" className="btn-primary px-6" onClick={() => setStep(3)}>
                  下一步
                  <Icon name="chevronRight" className="h-4 w-4" />
                </button>
              </div>
            </div>
          )}

          {/* 步骤 3：Profile 预告 */}
          {step === 3 && (
            <div className="space-y-5">
              <div className="text-center">
                <Badge tone="violet" icon="sparkle">
                  下一步能力 · M1
                </Badge>
                <h1 className="mt-3 text-2xl font-semibold text-slate-50">
                  把资源组合成 Profile，一键分发
                </h1>
                <p className="mx-auto mt-2 max-w-2xl text-sm leading-relaxed text-slate-400">
                  现在你看到的是只读清单。接下来 AgentHub 会把这些资源组合成「环境档案
                  Profile」，再按 Agent 生成对应配置文件 —— 全程先看 diff、自动备份、可回滚。
                </p>
              </div>

              <Card>
                <div className="grid items-center gap-4 lg:grid-cols-[1fr_auto_1fr_auto_1fr]">
                  <div className="space-y-2">
                    <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                      资源池
                    </div>
                    {["模型供应商", "Skills", "MCP 服务器", "npm 包", "Python 环境"].map((r) => (
                      <div
                        key={r}
                        className="rounded-lg border border-ink-700 bg-ink-900 px-3 py-1.5 text-xs text-slate-300"
                      >
                        {r}
                      </div>
                    ))}
                  </div>

                  <Icon name="chevronRight" className="mx-auto hidden h-5 w-5 text-slate-600 lg:block" />

                  <div className="space-y-2">
                    <div className="text-[11px] font-semibold uppercase tracking-wider text-violet-400">
                      Profile
                    </div>
                    <div className="rounded-md border border-dashed border-accent-500/50 p-3">
                      <div className="text-sm font-medium text-slate-200">日常开发</div>
                      <div className="mt-1 text-[11px] text-slate-500">
                        OpenRouter · filesystem + context7 · py312-ai
                      </div>
                    </div>
                    <div className="rounded-md border border-dashed border-accent-500/30 p-3">
                      <div className="text-sm font-medium text-slate-200">轻量问答</div>
                      <div className="mt-1 text-[11px] text-slate-500">Ollama(本地) · fetch</div>
                    </div>
                  </div>

                  <Icon name="chevronRight" className="mx-auto hidden h-5 w-5 text-slate-600 lg:block" />

                  <div className="space-y-2">
                    <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                      Agent 目标
                    </div>
                    {installed.slice(0, 3).map((a) => (
                      <div
                        key={a.id}
                        className="flex items-center gap-2 rounded-lg border border-ink-700 bg-ink-900 px-3 py-1.5 text-xs text-slate-300"
                      >
                        <span className="h-4 w-1 rounded" style={{ background: a.accent }} />
                        {a.name}
                      </div>
                    ))}
                    {installed.length === 0 && (
                      <div className="rounded-lg border border-ink-700 bg-ink-900 px-3 py-1.5 text-xs text-slate-500">
                        发现 Agent 后在此列出
                      </div>
                    )}
                  </div>
                </div>

                <div className="mt-4 grid gap-2 border-t border-ink-800 pt-4 sm:grid-cols-3">
                  {[
                    { icon: "search" as IconName, text: "写入前展示完整 diff" },
                    { icon: "shield" as IconName, text: "自动备份 + 一键回滚" },
                    { icon: "alert" as IconName, text: "检测外部手改，避免覆盖" },
                  ].map((item) => (
                    <div key={item.text} className="flex items-center gap-2 text-xs text-slate-400">
                      <Icon name={item.icon} className="h-3.5 w-3.5 text-brand-400" />
                      {item.text}
                    </div>
                  ))}
                </div>
              </Card>

              <div className="flex justify-center">
                <button type="button" className="btn-primary px-6" onClick={() => setStep(4)}>
                  下一步
                  <Icon name="chevronRight" className="h-4 w-4" />
                </button>
              </div>
            </div>
          )}

          {/* 步骤 4：完成 */}
          {step === 4 && (
            <div className="space-y-5 text-center">
              <span className="mx-auto grid h-14 w-14 place-items-center rounded-md border border-brand-500/40 text-brand-400">
                <Icon name="check" className="h-7 w-7" strokeWidth={2.4} />
              </span>
              <h1 className="text-2xl font-semibold text-slate-50">准备就绪</h1>
              <p className="mx-auto max-w-2xl text-sm leading-relaxed text-slate-400">
                扫描结果已存入本地数据库（SQLite），随时可以在仪表盘查看。所有数据都保存在本机，
                API Key 一类的敏感值只做脱敏展示，不会写入数据库。
              </p>

              <div className="mx-auto grid max-w-3xl gap-3 sm:grid-cols-3">
                {[
                  { title: "仪表盘", detail: "拓扑图 + 工具链状态 + 告警", route: "dashboard" },
                  { title: "资源页", detail: "Skills / MCP / npm / Python 只读清单", route: "skills" },
                  { title: "设置", detail: "指定可执行文件路径与扫描范围", route: "settings" },
                ].map((item) => (
                  <Card key={item.title} className="text-left">
                    <div className="text-sm font-medium text-slate-200">{item.title}</div>
                    <div className="mt-1 text-xs text-slate-500">{item.detail}</div>
                  </Card>
                ))}
              </div>

              <div className="flex justify-center pt-2">
                <button type="button" onClick={finish} className="btn-primary px-6 py-2.5">
                  进入仪表盘
                  <Icon name="chevronRight" className="h-4 w-4" />
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}