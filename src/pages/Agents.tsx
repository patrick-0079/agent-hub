/** Agent 目标：按类型分组，展示安装状态与完整证据链。 */

import React, { useMemo, useState } from "react";
import { Icon, type IconName } from "../components/Icon";
import {
  Badge,
  Card,
  Empty,
  PathRow,
  SearchInput,
  SectionCard,
  SegmentedControl,
  type Tone,
} from "../components/ui";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { AgentEvidence, AgentTarget } from "../lib/types";

const KIND_META: Record<string, { label: string; hint: string; icon: IconName }> = {
  cli: {
    label: "CLI / TUI Agent",
    hint: "以命令行或终端界面运行的 Agent",
    icon: "terminal",
  },
  ide: {
    label: "AI 原生编辑器",
    hint: "自带 Agent 能力的编辑器",
    icon: "adapter",
  },
  extension: {
    label: "编辑器扩展 Agent",
    hint: "以扩展形式运行在宿主编辑器内部",
    icon: "link",
  },
  host: {
    label: "宿主应用（不是 Agent）",
    hint: "通用编辑器宿主，本身不提供 Agent，仅承载上面的扩展",
    icon: "cpu",
  },
};

const STATUS_META: Record<string, { label: string; tone: Tone; icon: IconName }> = {
  installed: { label: "已安装", tone: "teal", icon: "check" },
  configured: { label: "仅发现配置", tone: "amber", icon: "info" },
  leftover: { label: "残留", tone: "rose", icon: "alert" },
  absent: { label: "未安装", tone: "slate", icon: "dot" },
};

const STRENGTH_META: Record<string, { label: string; tone: Tone; icon: IconName }> = {
  strong: { label: "强证据", tone: "teal", icon: "check" },
  medium: { label: "中证据", tone: "amber", icon: "info" },
  weak: { label: "弱证据", tone: "slate", icon: "dot" },
};

const SIGNAL_ICON: Record<string, IconName> = {
  cli: "terminal",
  npm: "npm",
  "install-dir": "folder",
  config: "settings",
  data: "folder",
  "skill-dir": "skills",
};

function AgentCard({ agent, reveal }: { agent: AgentTarget; reveal: (p: string) => void }) {
  const [showAll, setShowAll] = useState(false);
  const status = STATUS_META[agent.status] ?? STATUS_META.absent;
  const evidence = showAll ? agent.evidence : agent.evidence.slice(0, 4);
  const hidden = agent.evidence.length - evidence.length;

  return (
    <Card className="border-ink-700/60" padded={false}>
      {/* 头部 */}
      <div className="flex flex-wrap items-center gap-3 border-b border-ink-800/70 px-4 py-3">
        <span className="h-8 w-1.5 shrink-0 rounded" style={{ background: agent.accent }} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-semibold text-slate-100">{agent.name}</span>
            <span className="text-[11px] text-slate-500">{agent.vendor}</span>
            <Badge tone={status.tone} icon={status.icon}>
              {status.label}
            </Badge>
            {agent.adapter && (
              <Badge tone="violet" icon="adapter">
                适配器 {agent.adapter}
              </Badge>
            )}
          </div>
          <p className="mt-0.5 text-[11px] text-slate-500">{agent.summary}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {agent.mcpCount > 0 && (
            <Badge tone="sky" icon="mcp">
              MCP {agent.mcpCount}
            </Badge>
          )}
          {agent.skillCount > 0 && (
            <Badge tone="teal" icon="skills">
              Skill {agent.skillCount}
            </Badge>
          )}
        </div>
      </div>

      {/* 强信号速览 */}
      {(agent.cli || agent.npmPackage) && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-b border-ink-800/70 px-4 py-2 text-[11px]">
          {agent.cli && (
            <span className="flex min-w-0 items-center gap-1.5">
              <Icon name="terminal" className="h-3 w-3 shrink-0 text-brand-400" />
              <span className="mono truncate" title={agent.cli}>
                {agent.cli}
              </span>
              {agent.cliVersion && (
                <span className="shrink-0 font-mono text-brand-400">v{agent.cliVersion}</span>
              )}
            </span>
          )}
          {agent.npmPackage && (
            <span className="flex min-w-0 items-center gap-1.5">
              <Icon name="npm" className="h-3 w-3 shrink-0 text-amber-300" />
              <span className="mono truncate">{agent.npmPackage}</span>
            </span>
          )}
        </div>
      )}

      {/* 证据链 */}
      <div className="px-4 py-3">
        <div className="mb-1.5 flex items-center gap-2">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-600">
            判定证据
          </span>
          <span className="text-[10.5px] text-slate-600">
            强证据（程序本体）决定「已安装」；仅配置文件为「仅发现配置」；仅数据目录为「残留」
          </span>
        </div>
        <div className="space-y-1">
          {evidence.map((item, index) => (
            <EvidenceRow key={`${item.signal}-${index}`} item={item} />
          ))}
          {agent.evidence.length === 0 && (
            <p className="text-xs text-slate-600">未发现任何安装痕迹</p>
          )}
        </div>
        {hidden > 0 && (
          <button
            type="button"
            onClick={() => setShowAll(true)}
            className="mt-1.5 text-[11px] text-slate-500 hover:text-brand-400"
          >
            展开其余 {hidden} 条证据
          </button>
        )}
      </div>

      {/* 配置位置（仅在有痕迹时展示） */}
      {agent.status !== "absent" && (
        <div className="border-t border-ink-800/70 px-4 py-2">
          <div className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-slate-600">
            配置与数据位置
          </div>
          <div className="divide-y divide-ink-800/60">
            {agent.configs.map((config) => (
              <PathRow
                key={`${agent.id}-${config.path}`}
                label={config.label}
                path={config.path}
                exists={config.exists}
                size={config.size}
                onReveal={reveal}
              />
            ))}
          </div>
        </div>
      )}

      {agent.notes.length > 0 && (
        <div className="flex flex-col gap-1 border-t border-ink-800/70 px-4 py-2.5">
          {agent.notes.map((note) => (
            <span key={note} className="flex items-start gap-1.5 text-[11px] text-slate-500">
              <Icon name="info" className="mt-0.5 h-3 w-3 shrink-0" />
              {note}
            </span>
          ))}
        </div>
      )}
    </Card>
  );
}

function EvidenceRow({ item }: { item: AgentEvidence }) {
  const strength = STRENGTH_META[item.strength] ?? STRENGTH_META.weak;
  return (
    <div
      className={`flex items-center gap-2.5 rounded-md border px-2.5 py-1.5 ${
        item.found ? "border-ink-800 bg-ink-900" : "border-ink-800/50 bg-ink-950"
      }`}
    >
      <Icon
        name={SIGNAL_ICON[item.signal] ?? "dot"}
        className={`h-3 w-3 shrink-0 ${item.found ? "text-slate-400" : "text-slate-600"}`}
      />
      <span
        className={`w-40 shrink-0 truncate text-[11.5px] ${
          item.found ? "text-slate-300" : "text-slate-600 line-through"
        }`}
        title={item.label}
      >
        {item.label}
      </span>
      <span
        className={`mono min-w-0 flex-1 truncate ${item.found ? "" : "text-slate-600"}`}
        title={item.value}
      >
        {item.value}
      </span>
      <Badge tone={strength.tone} className="shrink-0">
        {strength.label}
      </Badge>
    </div>
  );
}

export function AgentsPage() {
  const snapshot = useApp((s) => s.snapshot);
  const reveal = useReveal();
  const [statusFilter, setStatusFilter] = useState("agents");
  const [query, setQuery] = useState("");

  const agents = snapshot?.agents ?? [];
  const realAgents = agents.filter((a) => a.kind !== "host");

  const counts = useMemo(
    () => ({
      installed: realAgents.filter((a) => a.status === "installed").length,
      configured: realAgents.filter((a) => a.status === "configured").length,
      leftover: realAgents.filter((a) => a.status === "leftover").length,
      absent: realAgents.filter((a) => a.status === "absent").length,
    }),
    [realAgents],
  );

  const list = useMemo(() => {
    const q = query.trim().toLowerCase();
    return agents
      .filter((a) => {
        if (statusFilter === "agents" && a.kind === "host") return false;
        if (statusFilter === "installed" && a.status !== "installed") return false;
        if (statusFilter === "attention" && !["configured", "leftover"].includes(a.status))
          return false;
        if (!q) return true;
        return (
          a.name.toLowerCase().includes(q) ||
          a.vendor.toLowerCase().includes(q) ||
          (a.cli ?? "").toLowerCase().includes(q) ||
          a.configs.some((c) => c.path.toLowerCase().includes(q))
        );
      })
      .sort((a, b) => {
        const order = ["installed", "configured", "leftover", "absent"];
        return order.indexOf(a.status) - order.indexOf(b.status);
      });
  }, [agents, statusFilter, query]);

  const grouped = useMemo(() => {
    const map = new Map<string, AgentTarget[]>();
    list.forEach((agent) => {
      const arr = map.get(agent.kind) ?? [];
      arr.push(agent);
      map.set(agent.kind, arr);
    });
    return ["cli", "ide", "extension", "host"]
      .map((kind) => [kind, map.get(kind) ?? []] as const)
      .filter(([, arr]) => arr.length > 0);
  }, [list]);

  return (
    <div className="space-y-4">
      <SectionCard
        title="Agent 目标"
        subtitle="按「程序本体 → 配置 → 数据目录」的证据强度判定状态，避免把卸载残留误判为已安装"
        action={
          <div className="w-52">
            <SearchInput value={query} onChange={setQuery} placeholder="搜索名称 / CLI / 路径…" />
          </div>
        }
        bodyClassName="space-y-4"
      >
        <div className="flex flex-wrap items-center gap-3">
          <SegmentedControl
            value={statusFilter}
            onChange={setStatusFilter}
            options={[
              { value: "agents", label: "全部 Agent", count: realAgents.length },
              { value: "installed", label: "已安装", count: counts.installed },
              {
                value: "attention",
                label: "需注意",
                count: counts.configured + counts.leftover,
              },
              { value: "all", label: "含宿主", count: agents.length },
            ]}
          />
          <div className="flex items-center gap-2 text-[11px] text-slate-500">
            <span>共探测 {agents.length} 个目标</span>
            <span className="text-slate-700">·</span>
            <span className="text-brand-400">已安装 {counts.installed}</span>
            <span className="text-amber-300">仅配置 {counts.configured}</span>
            <span className="text-rose-300">残留 {counts.leftover}</span>
          </div>
        </div>

        {list.length === 0 ? (
          <Empty icon="agents" title="没有匹配的 Agent 目标" description="换个关键词或筛选条件试试。" />
        ) : (
          grouped.map(([kind, items]) => {
            const meta = KIND_META[kind];
            return (
              <div key={kind}>
                <div className="mb-2 flex items-center gap-2">
                  <Icon name={meta.icon} className="h-4 w-4 text-slate-400" />
                  <span className="text-sm font-semibold text-slate-200">{meta.label}</span>
                  <Badge tone="slate">{items.length}</Badge>
                  <span className="text-[11px] text-slate-500">{meta.hint}</span>
                </div>
                <div className="space-y-3">
                  {items.map((agent) => (
                    <AgentCard key={agent.id} agent={agent} reveal={reveal} />
                  ))}
                </div>
              </div>
            );
          })
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-3">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="search" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">同步三屏</span>
            <Badge tone="violet" className="ml-auto">
              M2
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            diff 预览 → 确认（列出备份与将执行的命令）→ 执行进度，全程可视化。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="history" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">备份与回滚</span>
            <Badge tone="violet" className="ml-auto">
              M2
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            时间线展示历史版本，任意版本可 diff 对比并一键回滚（回滚同样先出 diff）。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="alert" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">外部修改检测</span>
            <Badge tone="violet" className="ml-auto">
              M2
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            用哈希比对识别用户手改的配置，避免同步时覆盖你的改动。
          </p>
        </Card>
      </div>
    </div>
  );
}