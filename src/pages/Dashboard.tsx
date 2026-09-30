/** 仪表盘：KPI、拓扑图、工具链状态、供应商健康、告警、Agent 目标总览。 */

import React, { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { Topology } from "../components/Topology";
import {
  Badge,
  Card,
  Empty,
  Kpi,
  PathRow,
  SectionCard,
  StatusDot,
  type DotState,
} from "../components/ui";
import { api } from "../lib/api";
import { CATEGORY_LABEL, useReveal } from "../lib/hooks";
import { formatDuration, relativeTime } from "../lib/format";
import { useApp } from "../lib/store";
import type { ExecutableInfo, ProviderResource } from "../lib/types";

/** 供应商健康汇总：连通状态 + DeepSeek 余额（从资源库实时读取） */
function ProviderHealthCard({ onNavigate }: { onNavigate: () => void }) {
  const [providers, setProviders] = useState<ProviderResource[] | null>(null);

  useEffect(() => {
    api
      .providerResources()
      .then(setProviders)
      .catch(() => setProviders([]));
  }, []);

  if (providers == null) {
    return (
      <SectionCard title="供应商状态" subtitle="正在读取资源库…">
        <Empty icon="providers" title="读取中…" />
      </SectionCard>
    );
  }
  if (providers.length === 0) {
    return (
      <SectionCard title="供应商状态" subtitle="资源库还是空的">
        <Empty
          icon="providers"
          title="尚未登记任何供应商"
          description="把 API Key / Base URL 集中到 AgentHub，之后可以一键测试连通性、查询余额并分发到各 Agent。"
          action={
            <button type="button" className="btn-primary mt-2" onClick={onNavigate}>
              <Icon name="plus" className="h-4 w-4" />
              去新增供应商
            </button>
          }
        />
      </SectionCard>
    );
  }

  const ok = providers.filter((p) => p.health.status === "ok").length;
  const noKey = providers.filter((p) => p.health.status === "no_key").length;
  const failed = providers.filter((p) => p.health.status === "error").length;
  const untested = providers.length - ok - noKey - failed;
  const dot = (status: string): DotState =>
    status === "ok" ? "ok" : status === "no_key" ? "warn" : status === "error" ? "error" : "idle";

  return (
    <SectionCard
      title="供应商状态"
      subtitle={`共 ${providers.length} 个：${ok} 连通 · ${noKey} 缺 Key · ${failed} 失败 · ${untested} 未测`}
      action={
        <button type="button" className="btn-ghost btn-sm" onClick={onNavigate}>
          <Icon name="providers" className="h-3.5 w-3.5" />
          管理与测试
        </button>
      }
      bodyClassName="space-y-1.5"
    >
      {providers.slice(0, 6).map((p) => (
        <button
          key={p.id}
          type="button"
          onClick={onNavigate}
          className="flex w-full items-center gap-3 rounded-md border border-ink-800 bg-ink-900 px-3 py-2 text-left hover:border-ink-600"
        >
          <StatusDot state={dot(p.health.status)} />
          <span className="w-28 shrink-0 truncate text-sm text-slate-200">{p.name}</span>
          <Badge tone="slate">{p.kind}</Badge>
          <span className="min-w-0 flex-1 truncate text-[11px] text-slate-500" title={p.health.message}>
            {p.health.status === "ok"
              ? `连通 ${p.health.latencyMs} ms${p.health.models != null ? ` · ${p.health.models} 模型` : ""}`
              : p.health.status === "no_key"
                ? "可达，未保存 Key"
                : p.health.status === "error"
                  ? p.health.message
                  : "未测试"}
          </span>
          {p.balance.status === "ok" && (
            <Badge tone="teal" className="shrink-0">
              余额 {p.balance.currency} {p.balance.totalBalance}
            </Badge>
          )}
          {p.hasKey && (
            <Icon name="lock" className="h-3 w-3 shrink-0 text-slate-500" />
          )}
        </button>
      ))}
      {providers.length > 6 && (
        <p className="pt-1 text-center text-[11px] text-slate-500">
          其余 {providers.length - 6} 个见「模型供应商」页
        </p>
      )}
    </SectionCard>
  );
}

export function Dashboard() {
  const snapshot = useApp((s) => s.snapshot);
  const scanning = useApp((s) => s.scanning);
  const scan = useApp((s) => s.scan);
  const navigate = useApp((s) => s.navigate);
  const reveal = useReveal();

  if (!snapshot) {
    return (
      <Card className="mx-auto mt-10 max-w-2xl">
        <Empty
          icon="cpu"
          title={scanning ? "正在扫描本机环境…" : "还没有扫描数据"}
          description="AgentHub 会只读扫描本机的 Agent 配置目录、Skill 目录、MCP 配置、conda / uv 环境与 npm 全局包，全程不修改任何文件。"
          action={
            <button
              type="button"
              onClick={() => void scan()}
              disabled={scanning}
              className="btn-primary mt-2"
            >
              <Icon name="play" className="h-4 w-4" />
              {scanning ? "扫描中…" : "开始扫描"}
            </button>
          }
        />
      </Card>
    );
  }

  const realAgents = snapshot.agents.filter((a) => a.kind !== "host");
  const installedAgents = realAgents.filter((a) => a.installed);
  const attention = realAgents.filter(
    (a) => a.status === "configured" || a.status === "leftover",
  );
  const toolReady = snapshot.executables.filter((e) => e.found).length;
  const missing = snapshot.executables.filter((e) => !e.found);

  const grouped = snapshot.executables.reduce<Record<string, ExecutableInfo[]>>((acc, item) => {
    (acc[item.category] ??= []).push(item);
    return acc;
  }, {});

  return (
    <div className="space-y-4">
      {/* KPI */}
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-6">
        <Kpi
          label="已装 Agent"
          value={installedAgents.length}
          unit={`/ ${realAgents.length}`}
          icon="agents"
          hint={
            attention.length > 0
              ? `${attention.length} 个需注意（仅配置 / 残留）`
              : installedAgents.map((a) => a.name).slice(0, 2).join("、") || "未发现"
          }
          tone={attention.length > 0 ? "amber" : "teal"}
          onClick={() => navigate("agents")}
        />
        <Kpi
          label="Skills"
          value={snapshot.skills.length}
          icon="skills"
          tone="violet"
          hint={
            snapshot.skills.length > 0
              ? `最近更新 ${snapshot.skills[0]?.updatedAt ?? "—"}`
              : "尚未发现 Skill"
          }
          onClick={() => navigate("skills")}
        />
        <Kpi
          label="MCP 服务器"
          value={snapshot.mcpServers.length}
          icon="mcp"
          tone="sky"
          hint="配置条目（资源库可握手测试）"
          onClick={() => navigate("mcp")}
        />
        <Kpi
          label="Python 环境"
          value={snapshot.pythonEnvs.length}
          icon="python"
          tone="teal"
          hint={`${snapshot.pythonEnvs.filter((e) => e.manager === "conda").length} conda · ${
            snapshot.pythonEnvs.filter((e) => e.manager === "uv").length
          } uv`}
          onClick={() => navigate("python")}
        />
        <Kpi
          label="npm 全局包"
          value={snapshot.npmPackages.length}
          icon="npm"
          tone="amber"
          hint={`${snapshot.npmPackages.filter((p) => p.mcpCapable).length} 个与 MCP 相关`}
          onClick={() => navigate("npm")}
        />
        <Kpi
          label="工具链就绪"
          value={`${toolReady}/${snapshot.executables.length}`}
          icon="cpu"
          tone={missing.length === 0 ? "teal" : "amber"}
          hint={missing.length === 0 ? "全部就绪" : `缺失 ${missing.map((m) => m.name).join("、")}`}
          onClick={() => navigate("settings")}
        />
      </div>

      {/* 拓扑 */}
      <SectionCard
        title="环境拓扑"
        subtitle="Agent 目标 ⇄ Profile 层 ⇄ 资源池 的当前关系"
        action={
          <div className="flex items-center gap-2">
            <Badge tone="slate" icon="history">
              {relativeTime(snapshot.scannedAt)}扫描
            </Badge>
            <Badge tone="teal">耗时 {formatDuration(snapshot.durationMs)}</Badge>
          </div>
        }
      >
        <Topology snapshot={snapshot} onNavigate={navigate} />
      </SectionCard>

      <div className="grid gap-4 xl:grid-cols-2">
        {/* 工具链 */}
        <SectionCard
          title="工具链状态"
          subtitle={`已定位 ${toolReady} / ${snapshot.executables.length} 个组件`}
          action={
            <button type="button" className="btn-ghost btn-sm" onClick={() => navigate("settings")}>
              <Icon name="settings" className="h-3.5 w-3.5" />
              管理
            </button>
          }
          bodyClassName="space-y-3"
        >
          {Object.entries(grouped).map(([category, items]) => (
            <div key={category}>
              <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                {CATEGORY_LABEL[category] ?? category}
              </div>
              <div className="space-y-1">
                {items.map((item) => (
                  <div
                    key={item.name}
                    className="flex items-center gap-3 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2"
                  >
                    <StatusDot state={item.found ? "ok" : "idle"} />
                    <span className="w-24 shrink-0 text-sm text-slate-300">{item.displayName}</span>
                    <span className="mono min-w-0 flex-1 truncate" title={item.path ?? ""}>
                      {item.path ? item.path : item.hint ?? "未安装"}
                    </span>
                    {item.version && (
                      <span className="shrink-0 font-mono text-[11px] text-brand-400">
                        v{item.version}
                      </span>
                    )}
                    {item.source === "known-location" && (
                      <span className="shrink-0 font-mono text-[10px] text-amber-300">非 PATH</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </SectionCard>

        {/* 告警 */}
        <div className="space-y-4">
          <SectionCard
            title="告警与建议"
            subtitle={snapshot.warnings.length > 0 ? `${snapshot.warnings.length} 条需要关注` : "一切正常"}
            bodyClassName="space-y-2"
          >
            {snapshot.warnings.length === 0 ? (
              <div className="flex items-center gap-2 rounded-lg border border-brand-500/30 px-3 py-2.5 text-xs text-brand-400">
                <Icon name="check" className="h-4 w-4" />
                本次扫描未发现异常
              </div>
            ) : (
              snapshot.warnings.map((w, i) => (
                <div
                  key={i}
                  className="flex items-start gap-2.5 rounded-lg border border-amber-500/25 px-3 py-2 text-xs text-amber-200"
                >
                  <Icon name="alert" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span className="leading-relaxed">{w}</span>
                </div>
              ))
            )}
            {missing.length > 0 && (
              <div className="mt-1 space-y-1.5 border-t border-ink-800 pt-3">
                <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-600">
                  缺失组件的安装建议
                </div>
                {missing.map((m) => (
                  <div key={m.name} className="text-xs text-slate-400">
                    <span className="text-slate-300">{m.displayName}</span>
                    <span className="mx-1.5 text-slate-600">→</span>
                    <span className="font-mono text-[11px]">{m.hint}</span>
                  </div>
                ))}
              </div>
            )}
          </SectionCard>

          <SectionCard
            title="Agent 目标"
            subtitle={`${installedAgents.length} 个已安装${attention.length > 0 ? ` · ${attention.length} 个需注意` : ""}（宿主应用已单列，不计入 Agent）`}
            action={
              <button type="button" className="btn-ghost btn-sm" onClick={() => navigate("agents")}>
                查看详情
                <Icon name="chevronRight" className="h-3.5 w-3.5" />
              </button>
            }
            bodyClassName="space-y-2"
          >
            {installedAgents.slice(0, 5).map((agent) => (
              <div
                key={agent.id}
                className="flex items-center gap-3 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2.5"
              >
                <span className="h-7 w-1 rounded" style={{ background: agent.accent }} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm text-slate-200">{agent.name}</span>
                    <span className="text-[10px] text-slate-500">{agent.vendor}</span>
                  </div>
                  {agent.root && (
                    <div className="mono truncate" title={agent.root}>
                      {agent.root}
                    </div>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {agent.status !== "installed" && (
                    <Badge tone={agent.status === "configured" ? "amber" : "rose"}>
                      {agent.status === "configured" ? "仅配置" : "残留"}
                    </Badge>
                  )}
                  {agent.adapter && <Badge tone="teal">适配器</Badge>}
                  <Badge tone="slate">MCP {agent.mcpCount}</Badge>
                  <Badge tone="slate">Skill {agent.skillCount}</Badge>
                </div>
              </div>
            ))}
            {installedAgents.length === 0 && (
              <Empty
                icon="agents"
                title="未发现已安装的 Agent"
                description="AgentHub 目前探测 Claude Code、Codex CLI、DeepSeek Harness、Cursor、Cline 等 10 个目标。"
              />
            )}
          </SectionCard>

          <ProviderHealthCard onNavigate={() => navigate("providers")} />
        </div>
      </div>

      {/* 配置位置速览 */}
      <SectionCard
        title="配置位置速览"
        subtitle="扫描到的关键路径（点击可在资源管理器中打开）"
        bodyClassName="grid gap-x-6 md:grid-cols-2"
      >
        {installedAgents.flatMap((agent) =>
          agent.configs
            .filter((c) => c.exists)
            .slice(0, 3)
            .map((config) => (
              <PathRow
                key={`${agent.id}-${config.path}`}
                label={`${agent.name} · ${config.label}`}
                path={config.path}
                exists={config.exists}
                size={config.size}
                onReveal={reveal}
              />
            )),
        )}
      </SectionCard>
    </div>
  );
}