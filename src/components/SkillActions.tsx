/** Skill 的 T2/T3 操作界面：快捷导入、一键清理失效、重建链接、删除。
 *
 * 交互遵循三条不变式：先出计划 → 用户确认 → 执行并展示逐步结果（附可恢复清单）。
 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "./Icon";
import { Badge, Card, Modal, SegmentedControl, type Tone } from "./ui";
import { api, describeError } from "../lib/api";
import { formatBytes, shortenPath } from "../lib/format";
import { useApp } from "../lib/store";
import type {
  ActionPlan,
  ActionResult,
  BrokenRef,
  DiscoveredSkill,
  SkillEnv,
  SkillFound,
} from "../lib/types";

const TIER_TONE: Record<string, Tone> = { T0: "teal", T1: "sky", T2: "violet", T3: "rose" };

/* --------------------------------------------------------------- 通用视图 */

export function PlanView({ plan }: { plan: ActionPlan }) {
  const destructive = plan.items.filter((i) => i.risk === "destructive").length;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={TIER_TONE[plan.tierCode] ?? "violet"}>
          {plan.tierCode} · {plan.tier === "deploy" ? "部署" : plan.tier === "mutate" ? "变更" : plan.tier}
        </Badge>
        <span className="font-mono text-[11px] text-slate-500">{plan.capability}</span>
        {destructive > 0 && (
          <Badge tone="rose" icon="alert">
            {destructive} 项会改变磁盘
          </Badge>
        )}
      </div>

      <p className="text-sm text-slate-200">{plan.summary}</p>

      {plan.warnings.length > 0 && (
        <div className="space-y-1.5">
          {plan.warnings.map((w, i) => (
            <div
              key={i}
              className="flex items-start gap-2 rounded-lg border border-amber-500/25 bg-amber-500/5 px-3 py-2 text-xs leading-relaxed text-amber-200"
            >
              <Icon name="alert" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              {w}
            </div>
          ))}
        </div>
      )}

      <div className="max-h-72 space-y-1 overflow-auto rounded-lg border border-ink-800 bg-ink-950/40 p-2">
        {plan.items.slice(0, 200).map((item, i) => (
          <div key={`${item.target}-${i}`} className="flex items-start gap-2 rounded px-2 py-1.5 text-[11.5px]">
            <span
              className={`mt-1 h-1.5 w-1.5 shrink-0 rounded-full ${
                item.risk === "destructive" ? "bg-rose-400" : item.action === "skip" ? "bg-ink-500" : "bg-brand-400"
              }`}
            />
            <span className="w-24 shrink-0 font-mono text-[10.5px] text-slate-500">{item.action}</span>
            <span className="min-w-0 flex-1">
              <span className="mono block truncate text-slate-300" title={item.target}>
                {item.target}
              </span>
              <span className="text-slate-500">{item.detail}</span>
            </span>
          </div>
        ))}
        {plan.items.length > 200 && (
          <p className="px-2 py-1 text-[11px] text-slate-600">
            仅显示前 200 项，共 {plan.items.length} 项
          </p>
        )}
      </div>

      <p className="text-[11px] text-slate-500">{plan.confirmHint}</p>
    </div>
  );
}

export function ResultView({
  result,
  onReveal,
}: {
  result: ActionResult;
  onReveal: (path: string) => void;
}) {
  const okCount = result.steps.filter((s) => s.ok).length;
  return (
    <div className="space-y-3">
      <div
        className={`flex items-center gap-2 rounded-lg border px-3 py-2.5 text-sm ${
          result.ok
            ? "border-brand-500/30 bg-brand-500/5 text-brand-400"
            : "border-rose-500/30 bg-rose-500/5 text-rose-300"
        }`}
      >
        <Icon name={result.ok ? "check" : "alert"} className="h-4 w-4" />
        {result.summary || result.title}
      </div>

      {result.warnings.map((w, i) => (
        <div
          key={i}
          className="flex items-start gap-2 rounded-lg border border-amber-500/25 bg-amber-500/5 px-3 py-2 text-xs text-amber-200"
        >
          <Icon name="alert" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          {w}
        </div>
      ))}

      <div className="max-h-72 space-y-1 overflow-auto rounded-lg border border-ink-800 bg-ink-950/40 p-2">
        <div className="px-2 pb-1 text-[10.5px] uppercase tracking-wider text-slate-600">
          执行明细（{okCount} / {result.steps.length} 成功）
        </div>
        {result.steps.map((step, i) => (
          <div key={`${step.target}-${i}`} className="flex items-start gap-2 px-2 py-1 text-[11.5px]">
            <Icon
              name={step.ok ? "check" : "close"}
              className={`mt-0.5 h-3 w-3 shrink-0 ${step.ok ? "text-brand-400" : "text-rose-400"}`}
            />
            <span className="min-w-0 flex-1">
              <span className="mono block truncate text-slate-400" title={step.target}>
                {step.target}
              </span>
              <span className={step.ok ? "text-slate-500" : "text-rose-300"}>{step.message}</span>
            </span>
          </div>
        ))}
      </div>

      {result.manifest && (
        <div className="flex items-center gap-2 rounded-lg border border-ink-700 bg-ink-900/40 px-3 py-2">
          <Icon name="shield" className="h-3.5 w-3.5 shrink-0 text-brand-400" />
          <span className="mono min-w-0 flex-1 truncate" title={result.manifest}>
            {result.manifest}
          </span>
          <button type="button" className="btn-ghost btn-sm" onClick={() => onReveal(result.manifest as string)}>
            打开清单目录
          </button>
        </div>
      )}
      {result.restoreHint && <p className="text-[11px] text-slate-500">{result.restoreHint}</p>}
    </div>
  );
}

/* ------------------------------------------------------------ 导入对话框 */

const SUGGESTED_REPO = "https://github.com/anthropics/skills";
const KNOWN_REPOS = [
  { url: SUGGESTED_REPO, label: "anthropics/skills（官方示例技能集）" },
];

export function ImportDialog({
  open,
  onClose,
  env,
  onDone,
  onReveal,
}: {
  open: boolean;
  onClose: () => void;
  env: SkillEnv | null;
  onDone: () => void;
  onReveal: (path: string) => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [tab, setTab] = useState("local");
  const [source, setSource] = useState("");
  const [url, setUrl] = useState(SUGGESTED_REPO);
  const [clonePath, setClonePath] = useState<string | null>(null);
  const [cloneResult, setCloneResult] = useState<ActionResult | null>(null);
  const [library, setLibrary] = useState("");
  const [mode, setMode] = useState("link");
  const [discovered, setDiscovered] = useState<DiscoveredSkill[] | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [plan, setPlan] = useState<ActionPlan | null>(null);
  const [result, setResult] = useState<ActionResult | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) {
      setDiscovered(null);
      setPlan(null);
      setResult(null);
      setPicked([]);
      setClonePath(null);
      setCloneResult(null);
    }
  }, [open]);

  useEffect(() => {
    if (env && !library) {
      const preferred = env.libraries.find((l) => l.exists) ?? env.libraries[0];
      if (preferred) setLibrary(preferred.path);
    }
  }, [env, library]);

  const effectiveSource = tab === "git" ? (clonePath ?? "") : source;

  const doClone = async () => {
    setBusy(true);
    setCloneResult(null);
    try {
      const outcome = await api.gitCloneRepo(url);
      setCloneResult(outcome.result);
      if (outcome.path) {
        setClonePath(outcome.path);
        const list = await api.skillDiscover(outcome.path, library || undefined);
        setDiscovered(list);
        setPicked(list.filter((s) => !s.conflict).map((s) => s.name));
        setPlan(null);
      }
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const doDiscover = async () => {
    setBusy(true);
    try {
      const list = await api.skillDiscover(source, library || undefined);
      setDiscovered(list);
      setPicked(list.filter((s) => !s.conflict).map((s) => s.name));
      setPlan(null);
    } catch (error) {
      setBanner(describeError(error));
      setDiscovered(null);
    } finally {
      setBusy(false);
    }
  };

  const doPlan = async () => {
    setBusy(true);
    try {
      setPlan(await api.skillImportPlan(effectiveSource, library || null, mode, picked));
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const doApply = async () => {
    setBusy(true);
    try {
      const outcome = await api.skillImportApply(effectiveSource, library || null, mode, picked);
      setResult(outcome);
      onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const closeAndCleanTmp = async () => {
    if (clonePath) {
      await api.tmpCleanup(clonePath).catch(() => undefined);
    }
    onClose();
  };

  return (
    <Modal
      open={open}
      onClose={() => void closeAndCleanTmp()}
      title="快捷导入 Skill"
      subtitle="从本地目录或 Git 仓库导入到技能库"
      footer={
        <>
          <button type="button" className="btn-ghost" onClick={() => void closeAndCleanTmp()} disabled={busy}>
            取消
          </button>
          {result ? (
            <button
              type="button"
              className="btn-primary"
              onClick={() => {
                void closeAndCleanTmp();
              }}
            >
              完成
            </button>
          ) : plan ? (
            <button type="button" className="btn-primary" onClick={() => void doApply()} disabled={busy}>
              {busy ? "执行中…" : "确认导入"}
            </button>
          ) : (
            <button
              type="button"
              className="btn-primary"
              onClick={() => void doPlan()}
              disabled={busy || !discovered || picked.length === 0}
            >
              预览导入计划
            </button>
          )}
        </>
      }
    >
      {result ? (
        <ResultView result={result} onReveal={onReveal} />
      ) : plan ? (
        <PlanView plan={plan} />
      ) : (
        <div className="space-y-4">
          <SegmentedControl
            value={tab}
            onChange={(next) => {
              setTab(next);
              setDiscovered(null);
              setPlan(null);
            }}
            options={[
              { value: "local", label: "本地目录" },
              { value: "git", label: "Git 仓库" },
            ]}
          />

          {tab === "local" ? (
            <div>
              <label className="text-xs text-slate-400">
                来源目录（可含多个 Skill 子目录，也支持 <span className="font-mono">skills/</span> 容器）
              </label>
              <div className="mt-1.5 flex gap-2">
                <input
                  value={source}
                  onChange={(e) => setSource(e.target.value)}
                  placeholder="例如 A:\Downloads\my-skills"
                  className="input font-mono text-xs"
                />
                <button
                  type="button"
                  className="btn-ghost shrink-0"
                  onClick={() => void doDiscover()}
                  disabled={busy || !source.trim()}
                >
                  <Icon name="search" className="h-4 w-4" />
                  扫描预览
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <label className="text-xs text-slate-400">仓库地址（浅克隆到临时目录）</label>
              <div className="flex gap-2">
                <input
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  className="input font-mono text-xs"
                />
                <button
                  type="button"
                  className="btn-ghost shrink-0"
                  onClick={() => void doClone()}
                  disabled={busy || !url.trim()}
                >
                  <Icon name="link" className="h-4 w-4" />
                  {busy ? "克隆中…" : "克隆并预览"}
                </button>
              </div>
              <div className="flex flex-wrap gap-1.5">
                {KNOWN_REPOS.map((repo) => (
                  <button
                    key={repo.url}
                    type="button"
                    onClick={() => setUrl(repo.url)}
                    className="rounded-md border border-ink-700 px-2 py-1 text-[11px] text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                  >
                    {repo.label}
                  </button>
                ))}
              </div>
              <p className="text-[11px] text-slate-500">
                代理：{env?.proxy || "未设置"} ·{" "}
                {env?.git ? `git 可用（${env.git}）` : "未检测到 git，无法从仓库导入"}
                <br />
                克隆只下载到 AgentHub 临时目录，不改动任何 Agent 配置。
              </p>
              {cloneResult && !cloneResult.ok && <ResultView result={cloneResult} onReveal={onReveal} />}
              {clonePath && cloneResult?.ok && (
                <div className="flex items-center gap-2 rounded-lg border border-ink-800 bg-ink-900/40 px-3 py-2">
                  <Icon name="folder" className="h-3.5 w-3.5 shrink-0 text-slate-500" />
                  <span className="mono min-w-0 flex-1 truncate">{clonePath}</span>
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => {
                      void api.tmpCleanup(clonePath).then(() => {
                        setClonePath(null);
                        setCloneResult(null);
                        setDiscovered(null);
                      });
                    }}
                  >
                    清理临时目录
                  </button>
                </div>
              )}
            </div>
          )}

          {/* 目标库与模式 */}
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-xs text-slate-400">目标技能库</label>
              <select
                value={library}
                onChange={(e) => {
                  setLibrary(e.target.value);
                  setDiscovered(null);
                  setPlan(null);
                }}
                className="input mt-1.5 font-mono text-xs"
              >
                {(env?.libraries ?? []).map((lib) => (
                  <option key={lib.path} value={lib.path}>
                    {lib.path}
                    {lib.exists ? `（已有 ${lib.skillCount} 个）` : "（不存在，将创建）"}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-slate-400">部署方式</label>
              <div className="mt-1.5">
                <SegmentedControl
                  value={mode}
                  onChange={setMode}
                  options={[
                    { value: "link", label: "链接（省空间）" },
                    { value: "copy", label: "拷贝（独立副本）" },
                  ]}
                />
              </div>
            </div>
          </div>

          {/* 发现结果 */}
          {discovered && (
            <Card className="border-ink-700/60">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-medium text-slate-200">
                  发现 {discovered.length} 个 Skill
                </span>
                <button
                  type="button"
                  className="text-[11px] text-slate-500 hover:text-brand-400"
                  onClick={() => setPicked(discovered.map((s) => s.name))}
                >
                  全选
                </button>
                <button
                  type="button"
                  className="text-[11px] text-slate-500 hover:text-brand-400"
                  onClick={() => setPicked([])}
                >
                  全不选
                </button>
                <span className="ml-auto text-[11px] text-slate-500">已选 {picked.length} 个</span>
              </div>
              <div className="mt-2 max-h-64 space-y-1 overflow-auto">
                {discovered.map((skill) => (
                  <label
                    key={skill.path}
                    className="flex cursor-pointer items-start gap-2.5 rounded-md border border-ink-800 bg-ink-900/40 px-2.5 py-2 hover:bg-ink-800/50"
                  >
                    <input
                      type="checkbox"
                      checked={picked.includes(skill.name)}
                      onChange={(e) =>
                        setPicked((prev) =>
                          e.target.checked
                            ? [...prev, skill.name]
                            : prev.filter((n) => n !== skill.name),
                        )
                      }
                      className="mt-0.5 accent-teal-400"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="truncate text-[12.5px] text-slate-200">{skill.name}</span>
                        {skill.conflict && (
                          <Badge tone="amber" icon="alert">
                            库中已存在，将跳过
                          </Badge>
                        )}
                        <span className="font-mono text-[10.5px] text-slate-500">
                          {skill.fileCount} 文件 · {formatBytes(skill.bytes)}
                        </span>
                      </span>
                      {skill.description && (
                        <span className="mt-0.5 line-clamp-2 block text-[11px] leading-relaxed text-slate-500">
                          {skill.description}
                        </span>
                      )}
                    </span>
                  </label>
                ))}
              </div>
            </Card>
          )}
        </div>
      )}
    </Modal>
  );
}

/* ------------------------------------------------------- 清理失效链接 */

export function CleanupDialog({
  open,
  onClose,
  broken,
  env,
  onDone,
  onReveal,
}: {
  open: boolean;
  onClose: () => void;
  broken: BrokenRef[];
  env: SkillEnv | null;
  onDone: () => void;
  onReveal: (path: string) => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [plan, setPlan] = useState<ActionPlan | null>(null);
  const [result, setResult] = useState<ActionResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [relinkTo, setRelinkTo] = useState("");
  const [showRelink, setShowRelink] = useState(false);

  useEffect(() => {
    if (!open) {
      setPlan(null);
      setResult(null);
      setShowRelink(false);
    }
  }, [open]);

  useEffect(() => {
    if (open && broken.length > 0 && !plan) {
      api.skillCleanupPlan(broken).then(setPlan).catch((e) => setBanner(describeError(e)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, broken]);

  useEffect(() => {
    if (env && !relinkTo) {
      const preferred = env.libraries.find((l) => l.exists && l.skillCount > 0) ?? env.libraries[0];
      if (preferred) setRelinkTo(preferred.path);
    }
  }, [env, relinkTo]);

  const applyCleanup = async () => {
    setBusy(true);
    try {
      setResult(await api.skillCleanupApply(broken));
      onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const applyRelink = async () => {
    if (!relinkTo) return;
    setBusy(true);
    try {
      setResult(await api.skillRelinkApply(broken, relinkTo));
      onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const switchRelink = async () => {
    setShowRelink(true);
    setBusy(true);
    try {
      setPlan(await api.skillRelinkPlan(broken, relinkTo || env?.libraries[0]?.path || ""));
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const onRelinkTargetChange = async (next: string) => {
    setRelinkTo(next);
    setBusy(true);
    try {
      setPlan(await api.skillRelinkPlan(broken, next));
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="失效链接处理"
      subtitle={`共 ${broken.length} 个失效 Skill 链接`}
      footer={
        result ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            {!showRelink && (
              <button type="button" className="btn-ghost" onClick={() => void switchRelink()} disabled={busy}>
                <Icon name="link" className="h-4 w-4" />
                改为重建链接
              </button>
            )}
            {showRelink ? (
              <button
                type="button"
                className="btn-primary"
                onClick={() => void applyRelink()}
                disabled={busy || !relinkTo}
              >
                确认重建
              </button>
            ) : (
              <button
                type="button"
                className="btn bg-rose-600 text-white hover:bg-rose-500"
                onClick={() => void applyCleanup()}
                disabled={busy || !plan}
              >
                <Icon name="folder" className="h-4 w-4" />
                移入回收站并清理
              </button>
            )}
          </>
        )
      }
    >
      {result ? (
        <ResultView result={result} onReveal={onReveal} />
      ) : (
        <div className="space-y-4">
          {showRelink && (
            <Card className="border-ink-700/60">
              <div className="text-xs text-slate-400">
                选择要把这些链接重新指向哪个技能库（库中不存在的会被跳过）
              </div>
              <select
                value={relinkTo}
                onChange={(e) => void onRelinkTargetChange(e.target.value)}
                className="input mt-1.5 font-mono text-xs"
              >
                {(env?.libraries ?? []).map((lib) => (
                  <option key={lib.path} value={lib.path}>
                    {lib.path}（{lib.skillCount} 个 Skill）
                  </option>
                ))}
              </select>
            </Card>
          )}
          {plan && <PlanView plan={plan} />}
        </div>
      )}
    </Modal>
  );
}

/* ------------------------------------------------------------ 删除单个 */

export function DeleteDialog({
  open,
  onClose,
  skill,
  impact,
  onDone,
  onReveal,
}: {
  open: boolean;
  onClose: () => void;
  skill: SkillFound | null;
  impact: number;
  onDone: () => void;
  onReveal: (path: string) => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [plan, setPlan] = useState<ActionPlan | null>(null);
  const [result, setResult] = useState<ActionResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open || !skill) {
      setPlan(null);
      setResult(null);
      setError(null);
      return;
    }
    setBusy(true);
    api
      .skillDeletePlan(skill.path, impact)
      .then((p) => {
        setPlan(p);
        setError(null);
      })
      .catch((e) => {
        setPlan(null);
        setError(describeError(e));
      })
      .finally(() => setBusy(false));
  }, [open, skill, impact]);

  const apply = async () => {
    if (!skill) return;
    setBusy(true);
    try {
      setResult(await api.skillDeleteApply(skill.path));
      onDone();
    } catch (e) {
      setBanner(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="删除 Skill"
      subtitle={skill?.path}
      width="max-w-2xl"
      footer={
        result ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className="btn bg-rose-600 text-white hover:bg-rose-500"
              onClick={() => void apply()}
              disabled={busy || !plan}
            >
              <Icon name="folder" className="h-4 w-4" />
              移入回收站
            </button>
          </>
        )
      }
    >
      {result ? (
        <ResultView result={result} onReveal={onReveal} />
      ) : error ? (
        <div className="flex items-start gap-2 rounded-lg border border-rose-500/30 bg-rose-500/5 px-3 py-2.5 text-xs text-rose-200">
          <Icon name="alert" className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="leading-relaxed">{error}</span>
        </div>
      ) : plan ? (
        <PlanView plan={plan} />
      ) : (
        <p className="text-sm text-slate-400">正在生成删除计划…</p>
      )}
    </Modal>
  );
}

/* ------------------------------------------------------- 通用确认弹窗 */

/** 计划 → 确认 → 结果 的通用弹窗（恢复站永久删除、清空等复用） */
export function ActionDialog({
  open,
  onClose,
  title,
  subtitle,
  plan,
  result,
  error,
  busy,
  onConfirm,
  confirmLabel = "确认",
  danger = false,
  onReveal,
  extraFooter,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  plan: ActionPlan | null;
  result?: ActionResult | null;
  error?: string | null;
  busy?: boolean;
  onConfirm: () => void;
  confirmLabel?: string;
  danger?: boolean;
  onReveal: (path: string) => void;
  extraFooter?: React.ReactNode;
}) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      subtitle={subtitle}
      footer={
        result ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            {extraFooter}
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              type="button"
              className={
                danger
                  ? "btn bg-rose-600 text-white hover:bg-rose-500"
                  : "btn-primary"
              }
              onClick={onConfirm}
              disabled={busy || !plan}
            >
              {busy ? "执行中…" : confirmLabel}
            </button>
          </>
        )
      }
    >
      {result ? (
        <ResultView result={result} onReveal={onReveal} />
      ) : error ? (
        <div className="flex items-start gap-2 rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2.5 text-xs text-amber-200">
          <Icon name="info" className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="leading-relaxed">{error}</span>
        </div>
      ) : plan ? (
        <PlanView plan={plan} />
      ) : (
        <p className="text-sm text-slate-400">正在生成计划…</p>
      )}
    </Modal>
  );
}

/* -------------------------------------------------------- 由扫描结果构造 */

export function toBrokenRefs(skills: SkillFound[]): BrokenRef[] {
  return skills
    .filter((s) => s.broken)
    .map((s) => ({
      path: s.path,
      target: s.linkTarget ?? "",
      kind: s.linkKind ?? "symlink",
      owner: s.sourceAgent,
      name: s.dirName || s.name,
    }));
}

export function useBrokenGroups(broken: BrokenRef[]) {
  return useMemo(() => {
    const map = new Map<string, BrokenRef[]>();
    broken.forEach((item) => {
      const key = item.target.replace(/[\\/][^\\/]+$/, "");
      const arr = map.get(key) ?? [];
      arr.push(item);
      map.set(key, arr);
    });
    return Array.from(map.entries()).sort((a, b) => b[1].length - a[1].length);
  }, [broken]);
}

export function BrokenSummary({ broken }: { broken: BrokenRef[] }) {
  const groups = useBrokenGroups(broken);
  return (
    <div className="space-y-2">
      {groups.map(([target, items]) => (
        <div key={target} className="rounded-lg border border-rose-500/25 bg-rose-500/5 px-3 py-2">
          <div className="flex items-center gap-2 text-xs">
            <Icon name="alert" className="h-3.5 w-3.5 shrink-0 text-rose-300" />
            <span className="text-rose-200">{items.length} 个链接指向缺失目录</span>
          </div>
          <div className="mono mt-1 truncate text-[11px] text-slate-400" title={target}>
            {shortenPath(target, 64)}
          </div>
        </div>
      ))}
    </div>
  );
}