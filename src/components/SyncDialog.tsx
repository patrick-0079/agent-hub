/** 分发向导：选目标 → 看 diff → 执行（T2 三屏）。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "./Icon";
import { DiffView } from "./DiffView";
import { Badge, Card, Modal, SegmentedControl, Toggle } from "./ui";
import { ResultView } from "./SkillActions";
import { api, describeError } from "../lib/api";
import { shortenPath } from "../lib/format";
import { useApp } from "../lib/store";
import type { ActionResult, DefinitionsView, McpResource, SyncPlan } from "../lib/types";

const STRATEGY_LABEL: Record<string, string> = {
  "merge-keys": "结构合并",
  "managed-block": "托管块",
};

export function SyncDialog({
  open,
  onClose,
  kind = "mcp",
  profileId,
  resourceItems,
  onDone,
  onReveal,
}: {
  open: boolean;
  onClose: () => void;
  /** 分发哪一类资源：MCP 服务器 / 模型供应商 / 环境档案 */
  kind?: "mcp" | "provider" | "profile";
  /** kind = "profile" 时必填 */
  profileId?: number;
  resourceItems: { name: string; tag: string; enabled: boolean }[];
  onDone: () => void;
  onReveal: (path: string) => void;
}) {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);

  const [defs, setDefs] = useState<DefinitionsView | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [plan, setPlan] = useState<SyncPlan | null>(null);
  const [result, setResult] = useState<ActionResult | null>(null);
  const [activeFile, setActiveFile] = useState(0);
  const [busy, setBusy] = useState(false);
  const [overwrite, setOverwrite] = useState(false);

  useEffect(() => {
    if (open) {
      setPlan(null);
      setResult(null);
      setActiveFile(0);
      api
        .agentDefinitions()
        .then(setDefs)
        .catch((error) => setBanner(describeError(error)));
    }
  }, [open, setBanner]);

  /** 能接收 MCP 的目标：定义里声明了 [[mcp]] 且本机有安装痕迹 */
  const candidates = useMemo(() => {
    if (!defs) return [];
    return defs.definitions
      .filter((d) => d.file.mcp.length > 0 && d.file.capabilities.maxTier !== "observe")
      .map((d) => {
        const agent = snapshot?.agents.find((a) => a.id === d.file.agent.id);
        return {
          id: d.file.agent.id,
          name: d.file.agent.name || d.file.agent.id,
          accent: d.file.agent.accent,
          installed: agent?.status === "installed",
          status: agent?.status ?? "absent",
          files: d.file.mcp.map((m) => m.file),
          strategy: d.file.mcp[0]?.strategy ?? "",
          maxTier: d.file.capabilities.maxTier,
        };
      })
      .sort((a, b) => Number(b.installed) - Number(a.installed));
  }, [defs, snapshot]);

  useEffect(() => {
    if (open && selected.length === 0 && candidates.length > 0) {
      setSelected(candidates.filter((c) => c.installed).map((c) => c.id));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, candidates.length]);

  const planCall = (agentIds: string[], overwrite: boolean) =>
    kind === "profile"
      ? api.profileApplyPlan(profileId as number, agentIds, overwrite)
      : kind === "provider"
        ? api.providerSyncPlan(agentIds, overwrite)
        : api.mcpSyncPlan(agentIds, overwrite);
  const applyCall = (agentIds: string[], overwrite: boolean) =>
    kind === "profile"
      ? api.profileApplyRun(profileId as number, agentIds, overwrite)
      : kind === "provider"
        ? api.providerSyncApply(agentIds, overwrite)
        : api.mcpSyncApply(agentIds, overwrite);

  const makePlan = async () => {
    setBusy(true);
    try {
      setPlan(await planCall(selected, overwrite));
      setActiveFile(0);
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    setBusy(true);
    try {
      setResult(await applyCall(selected, overwrite));
      onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const writeTargets = plan?.targets.filter((t) => t.supported) ?? [];
  const changedTargets = writeTargets.filter((t) => t.added + t.updated + t.removed > 0);
  const skippedTotal = writeTargets.reduce((sum, t) => sum + t.skipped, 0);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={result ? "应用结果" : plan ? "确认写入（写入前请先看 diff）" : kind === "profile" ? "应用环境档案" : kind === "provider" ? "分发供应商到 Agent" : "分发 MCP 到 Agent"}
      subtitle={
        result
          ? result.summary
          : plan
            ? plan.summary
            : `${resourceItems.filter((r) => r.enabled).length} 个资源（{kind === "profile" ? "档案内容" : kind === "provider" ? "供应商" : "MCP"}）`
      }
      width="max-w-4xl"
      footer={
        result ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : plan ? (
          <>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setPlan(null)}
              disabled={busy}
            >
              返回选择
            </button>
            <button
              type="button"
              className="btn-primary"
              onClick={() => void apply()}
              disabled={busy || changedTargets.length === 0}
            >
              {busy ? "写入中…" : `确认写入 ${changedTargets.length} 个文件`}
            </button>
          </>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className="btn-primary"
              onClick={() => void makePlan()}
              disabled={busy || selected.length === 0}
            >
              {busy ? "生成中…" : "生成分发计划"}
            </button>
          </>
        )
      }
    >
      {result ? (
        <ResultView result={result} onReveal={onReveal} />
      ) : plan ? (
        /* ------------------------------------------------ 第二屏：diff */
        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone="violet">{plan.tierCode} · 部署</Badge>
            <Badge tone="slate" icon="mcp">
              {plan.servers.length} 个资源
            </Badge>
            <Badge tone={changedTargets.length > 0 ? "amber" : "teal"}>
              {changedTargets.length} 个文件需要写入
            </Badge>
            {skippedTotal > 0 && (
              <Badge tone="slate" icon="alert">
                {skippedTotal} 个同名非受管条目已跳过
              </Badge>
            )}
            <span className="text-[11px] text-slate-500">{plan.confirmHint}</span>
          </div>

          {plan.warnings.length > 0 && (
            <div className="space-y-1.5">
              {plan.warnings.map((w, i) => (
                <div
                  key={i}
                  className="flex items-start gap-2 rounded-lg border border-amber-500/25 bg-amber-950 px-3 py-2 text-xs text-amber-200"
                >
                  <Icon name="alert" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  {w}
                </div>
              ))}
            </div>
          )}

          {/* 文件切换 */}
          <div className="flex flex-wrap gap-1.5">
            {plan.targets.map((target, index) => (
              <button
                key={target.file + index}
                type="button"
                onClick={() => setActiveFile(index)}
                className={`rounded-lg border px-2.5 py-1.5 text-left text-[11px] transition-colors ${
                  index === activeFile
                    ? "border-brand-500/50 bg-brand-900 text-brand-300"
                    : "border-ink-700 bg-ink-900 text-slate-400 hover:bg-ink-800"
                }`}
              >
                <span className="block font-medium">{target.agentName}</span>
                <span className="block font-mono text-[10px] text-slate-500">
                  {target.supported
                    ? `+${target.added} ~${target.updated} -${target.removed}`
                    : "不支持"}
                </span>
              </button>
            ))}
          </div>

          {plan.targets[activeFile] && (
            <TargetDetail target={plan.targets[activeFile]} />
          )}
        </div>
      ) : (
        /* ---------------------------------------------- 第一屏：选目标 */
        <div className="space-y-4">
          <Card className="border-ink-700/60">
            <div className="text-xs text-slate-400">将要写入的资源</div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {resourceItems.filter((r) => r.enabled).map((r) => (
                <Badge key={r.name} tone="sky" icon="mcp">
                  {r.name}
                  {r.tag && <span className="ml-1 font-mono text-[10px] opacity-70">{r.tag}</span>}
                </Badge>
              ))}
              {resourceItems.filter((r) => r.enabled).length === 0 && (
                <span className="text-xs text-amber-300">
                  该档案里没有可分发的资源
                </span>
              )}
            </div>
          </Card>

          <Card className="border-ink-700/60">
            <div className="text-xs text-slate-400">写入行为</div>
            <div className="mt-1">
              <Toggle
                checked={overwrite}
                onChange={setOverwrite}
                label="覆盖同名但非 AgentHub 管理的条目"
                hint="默认关闭：只新增、只更新自己写入过的条目，你手写的同名 MCP 会被原样保留（避免丢掉我们渲染不出来的字段）。开启后会用资源库的定义覆盖它们。"
              />
            </div>
          </Card>

          <div>
            <div className="mb-2 flex items-center gap-2">
              <span className="text-sm font-medium text-slate-200">选择目标 Agent</span>
              <span className="text-[11px] text-slate-500">
                只列出「定义里声明了 MCP 来源」且能力上限高于 observe 的 Agent
              </span>
              <button
                type="button"
                className="ml-auto text-[11px] text-slate-500 hover:text-brand-400"
                onClick={() =>
                  setSelected(
                    selected.length === candidates.filter((c) => c.installed).length
                      ? []
                      : candidates.filter((c) => c.installed).map((c) => c.id),
                  )
                }
              >
                全选 / 全不选（已安装）
              </button>
            </div>
            <div className="space-y-1.5">
              {candidates.map((c) => (
                <label
                  key={c.id}
                  className={`flex cursor-pointer items-start gap-2.5 rounded-lg border px-3 py-2 transition-colors ${
                    selected.includes(c.id)
                      ? "border-brand-500/40 bg-brand-900"
                      : "border-ink-800 bg-ink-900 hover:bg-ink-800"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={selected.includes(c.id)}
                    onChange={(e) =>
                      setSelected((prev) =>
                        e.target.checked ? [...prev, c.id] : prev.filter((id) => id !== c.id),
                      )
                    }
                    className="mt-1 accent-teal-400"
                  />
                  <span className="h-6 w-1 shrink-0 rounded" style={{ background: c.accent }} />
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-[12.5px] text-slate-200">{c.name}</span>
                      {c.installed ? (
                        <Badge tone="teal" icon="check">
                          已安装
                        </Badge>
                      ) : (
                        <Badge tone="slate">{c.status}</Badge>
                      )}
                      <span className="font-mono text-[10px] text-slate-500">
                        {c.files.length} 个文件
                      </span>
                    </span>
                    <span className="mono mt-0.5 block truncate text-[10.5px]" title={c.files.join("\n")}>
                      {shortenPath(c.files[0] ?? "", 64)}
                    </span>
                  </span>
                </label>
              ))}
              {candidates.length === 0 && (
                <p className="text-xs text-slate-500">没有可用的目标 Agent。</p>
              )}
            </div>
          </div>
        </div>
      )}
    </Modal>
  );
}

function TargetDetail({ target }: { target: SyncPlan["targets"][number] }) {
  const [showChanges, setShowChanges] = useState(true);
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2">
        <span className="mono min-w-0 flex-1 truncate" title={target.file}>
          {target.file}
        </span>
        {target.supported ? (
          <>
            <Badge tone="slate">节点 {target.root || "(根)"}</Badge>
            <Badge tone="violet">{STRATEGY_LABEL[target.strategy] ?? target.strategy}</Badge>
            {!target.fileExists && <Badge tone="amber">文件不存在，将新建</Badge>}
          </>
        ) : (
          <Badge tone="rose" icon="alert">
            暂不支持
          </Badge>
        )}
      </div>

      {!target.supported ? (
        <div className="rounded-lg border border-rose-500/25 bg-rose-950 px-3 py-2.5 text-xs text-rose-200">
          {target.reason}
        </div>
      ) : target.kind === "skill" ? (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2 text-[11px]">
            <Badge tone="violet" icon="skills">
              {target.strategy === "skill-copy" ? "拷贝部署" : "链接部署"}
            </Badge>
            <span className="text-slate-400">
              新增 <span className="text-brand-400">{target.added}</span> · 已一致{" "}
              <span className="text-slate-500">{target.unchanged}</span>
              {target.skipped > 0 && (
                <>
                  {" "}
                  · 跳过 <span className="text-slate-400">{target.skipped}</span>
                </>
              )}
            </span>
            {target.reason && <span className="text-slate-500">{target.reason}</span>}
          </div>
          <div className="max-h-[420px] space-y-1 overflow-auto rounded-lg border border-ink-800 bg-ink-950 p-2">
            {target.changes.map((change, index) => (
              <div key={`${change.key}-${index}`} className="flex items-start gap-2 px-2 py-1.5">
                <span
                  className={`mt-0.5 w-12 shrink-0 rounded px-1 text-center font-mono text-[10px] ${
                    change.kind === "add"
                      ? "bg-brand-900 text-brand-300"
                      : change.kind === "skipped"
                        ? "bg-ink-800 text-slate-400"
                        : "bg-ink-800 text-slate-500"
                  }`}
                >
                  {change.kind === "add" ? "部署" : change.kind === "skipped" ? "跳过" : "未变"}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="mono block truncate text-[11.5px] text-slate-200">
                    {change.key}
                  </span>
                  <span className="block break-all text-[10.5px] text-slate-500">
                    {change.detail}
                  </span>
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <SegmentedControl
              value={showChanges ? "changes" : "diff"}
              onChange={(next) => setShowChanges(next === "changes")}
              options={[
                { value: "changes", label: "键级变更", count: target.changes.length },
                { value: "diff", label: "行级 diff" },
              ]}
            />
            <div className="ml-auto flex items-center gap-2 text-[11px]">
              <span className="text-brand-400">+{target.added}</span>
              <span className="text-amber-300">~{target.updated}</span>
              <span className="text-rose-300">-{target.removed}</span>
              {target.skipped > 0 && <span className="text-slate-400">跳过{target.skipped}</span>}
              <span className="text-slate-500">={target.unchanged}</span>
            </div>
          </div>

          {showChanges ? (
            <div className="max-h-[420px] space-y-1 overflow-auto rounded-lg border border-ink-800 bg-ink-950 p-2">
              {target.changes.length === 0 && (
                <p className="px-2 py-3 text-xs text-slate-500">无键级变更</p>
              )}
              {target.changes.map((change, index) => (
                <div key={`${change.key}-${index}`} className="flex items-start gap-2 px-2 py-1.5">
                  <span
                    className={`mt-0.5 w-14 shrink-0 rounded px-1 text-center font-mono text-[10px] ${
                      change.kind === "add"
                        ? "bg-brand-900 text-brand-300"
                        : change.kind === "update"
                          ? "bg-amber-950 text-amber-300"
                          : change.kind === "remove"
                            ? "bg-rose-950 text-rose-300"
                            : "bg-ink-800 text-slate-500"
                    }`}
                  >
                    {change.kind === "add"
                      ? "新增"
                      : change.kind === "update"
                        ? "更新"
                        : change.kind === "remove"
                          ? "清理"
                          : "未变"}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="mono block truncate text-[11.5px] text-slate-200">
                      {change.key}
                    </span>
                    <span className="mono block break-all text-[10.5px] text-slate-500">
                      {change.detail}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <DiffView
              diff={target.diff}
              emptyHint="该文件已与资源库一致，无需写入"
            />
          )}
        </>
      )}
    </div>
  );
}