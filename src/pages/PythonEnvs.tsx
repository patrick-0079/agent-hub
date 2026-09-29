/** Python 环境：conda / uv / venv 统一视图 + 受管环境创建（uv venv）/ 删除（入回收站）。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { Badge, Card, Empty, Modal, SearchInput, SectionCard, StatusDot } from "../components/ui";
import { api, describeError } from "../lib/api";
import { shortenPath } from "../lib/format";
import { MANAGER_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { ActionResult, EnvCreatePlan, PythonEnv } from "../lib/types";

const MANAGER_TONE: Record<string, "teal" | "violet" | "sky" | "amber"> = {
  conda: "teal",
  uv: "violet",
  venv: "sky",
};

const MANAGER_ICON: Record<string, "python" | "sparkle" | "folder"> = {
  conda: "python",
  uv: "sparkle",
  venv: "folder",
};

/* ------------------------------------------------------------ 创建向导 */

function CreateEnvDialog({
  open,
  onClose,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  onDone: () => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [path, setPath] = useState("");
  const [python, setPython] = useState("");
  const [plan, setPlan] = useState<EnvCreatePlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ActionResult | null>(null);

  useEffect(() => {
    if (open) {
      setPath("");
      setPython("");
      setPlan(null);
      setResult(null);
    }
  }, [open]);

  // 输入变化时刷新计划（先看将执行什么）
  const refreshPlan = (nextPath: string, nextPython: string) => {
    if (!nextPath.trim()) {
      setPlan(null);
      return;
    }
    api
      .pythonEnvCreatePlan(nextPath.trim(), nextPython.trim() || null)
      .then(setPlan)
      .catch((error) => setBanner(describeError(error)));
  };

  const create = async () => {
    setBusy(true);
    setResult(null);
    try {
      const r = await api.pythonEnvCreateRun(path.trim(), python.trim() || null);
      setResult(r);
      if (r.ok) onDone();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const blocked =
    busy || !path.trim() || (plan != null && (!plan.uvFound || plan.targetExists));

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="创建 Python 环境（uv venv）"
      subtitle="T3 变更操作：执行前先看计划；创建只写目标目录，删除时整体入回收站"
      width="max-w-2xl"
      footer={
        result?.ok ? (
          <button type="button" className="btn-primary" onClick={onClose}>
            完成
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button type="button" className="btn-primary" onClick={() => void create()} disabled={blocked}>
              {busy ? "创建中…" : "执行创建"}
            </button>
          </>
        )
      }
    >
      <div className="space-y-3.5">
        <div className="grid gap-3 sm:grid-cols-[1fr_180px]">
          <div>
            <label className="text-xs text-slate-400">目标目录（绝对路径，父目录需已存在）</label>
            <input
              value={path}
              onChange={(e) => {
                setPath(e.target.value);
                refreshPlan(e.target.value, python);
              }}
              placeholder="C:\envs\my-agent-env"
              className="input mt-1.5 font-mono text-xs"
            />
          </div>
          <div>
            <label className="text-xs text-slate-400">Python 版本（可选）</label>
            <input
              value={python}
              onChange={(e) => {
                setPython(e.target.value);
                refreshPlan(path, e.target.value);
              }}
              placeholder="如 3.12（留空用 uv 默认）"
              className="input mt-1.5 font-mono text-xs"
            />
          </div>
        </div>

        {plan && (
          <div className="rounded-md border border-ink-800 bg-ink-900 p-3">
            <div className="flex items-center gap-2">
              <StatusDot state={plan.uvFound && !plan.targetExists ? "ok" : plan.uvFound ? "warn" : "error"} />
              <span className="text-xs font-medium text-slate-200">将执行（{plan.tierCode}）</span>
              <span className="mono ml-auto truncate text-[10.5px]" title={plan.command}>
                {shortenPath(plan.command, 30)}
              </span>
            </div>
            <pre className="mono mt-1.5 whitespace-pre-wrap break-all text-[10.5px] text-slate-400">
              {plan.command} {plan.args.join(" ")}
            </pre>
            <p className="mt-1.5 text-[11px] text-slate-500">
              {plan.pythonNote} · {plan.message}
            </p>
          </div>
        )}

        {result && (
          <div
            className={`rounded-md border p-3 ${
              result.ok ? "border-brand-800 bg-brand-900" : "border-rose-800/70 bg-rose-950"
            }`}
          >
            <div className="flex items-center gap-2 text-xs">
              <StatusDot state={result.ok ? "ok" : "error"} />
              <span className={result.ok ? "text-brand-400" : "text-rose-300"}>
                {result.summary || result.title}
              </span>
            </div>
            {result.steps.map((step, i) => (
              <div key={i} className="mono mt-1.5 whitespace-pre-wrap break-all text-[10.5px]">
                <span className={step.ok ? "text-slate-400" : "text-rose-300"}>
                  {step.ok ? "✓" : "✗"} {step.target}
                </span>
                <span className="text-slate-500"> — {step.message}</span>
              </div>
            ))}
            {result.restoreHint && (
              <p className="mt-1.5 text-[11px] text-slate-500">{result.restoreHint}</p>
            )}
          </div>
        )}

        <p className="text-[11px] leading-relaxed text-slate-500">
          conda 创建暂未启用（参数形态随发行版差异大）；uv 不在 PATH 时到「设置 → 工具链」确认。
          创建成功后环境会标记为「AgentHub 受管」，即使放在扫描目录之外也会出现在列表里。
        </p>
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- 页面 */

export function PythonPage() {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();
  const [query, setQuery] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [managed, setManaged] = useState<PythonEnv[]>([]);

  const loadManaged = () => {
    api
      .pythonEnvManaged()
      .then(setManaged)
      .catch(() => setManaged([]));
  };

  useEffect(() => {
    loadManaged();
  }, []);

  const remove = async (env: PythonEnv) => {
    try {
      const r = await api.pythonEnvRemove(env.path);
      if (r.ok) {
        loadManaged();
        setBanner(`已删除「${env.name}」：${r.summary}`);
      } else {
        setBanner(r.steps.map((s) => s.message).join("；"));
      }
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  // 扫描结果 + 受管记录合并（按路径去重，受管优先显示标记）
  const envs = useMemo(() => {
    const scanned = snapshot?.pythonEnvs ?? [];
    const seen = new Set(
      scanned.map((e) => e.path.toLowerCase().replace(/[/\\]+$/, "")),
    );
    const extras = managed.filter(
      (m) => !seen.has(m.path.toLowerCase().replace(/[/\\]+$/, "")),
    );
    return [...scanned, ...extras];
  }, [snapshot?.pythonEnvs, managed]);

  const managedPaths = useMemo(
    () =>
      new Set(
        managed.map((m) => m.path.toLowerCase().replace(/[/\\]+$/, "")),
      ),
    [managed],
  );

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = envs.filter(
      (e) =>
        !q ||
        e.name.toLowerCase().includes(q) ||
        e.path.toLowerCase().includes(q) ||
        (e.pythonVersion ?? "").includes(q),
    );
    const map = new Map<string, PythonEnv[]>();
    filtered.forEach((env) => {
      const list = map.get(env.manager) ?? [];
      list.push(env);
      map.set(env.manager, list);
    });
    return Array.from(map.entries());
  }, [envs, query]);

  return (
    <div className="space-y-4">
      <SectionCard
        title="Python 环境"
        subtitle={`共 ${envs.length} 个环境（conda / uv / venv 统一视图${managed.length > 0 ? `，含 ${managed.length} 个受管` : ""}）`}
        action={
          <div className="flex items-center gap-2">
            <div className="w-48">
              <SearchInput value={query} onChange={setQuery} placeholder="搜索名称 / 路径…" />
            </div>
            <button
              type="button"
              className="btn-primary btn-sm"
              onClick={() => setCreateOpen(true)}
              title="通过 uv venv 创建新的虚拟环境（T3：先看计划再执行）"
            >
              <Icon name="plus" className="h-3.5 w-3.5" />
              创建环境
            </button>
          </div>
        }
        bodyClassName="space-y-5"
      >
        {envs.length === 0 ? (
          <Empty
            icon="python"
            title="未发现 Python 环境"
            description="AgentHub 会读取 conda env list、uv python list，并在主目录与自定义目录中搜索 pyvenv.cfg。也可以直接在这里用 uv 创建一个。"
            action={
              <button type="button" className="btn-primary mt-2" onClick={() => setCreateOpen(true)}>
                <Icon name="plus" className="h-4 w-4" />
                创建环境
              </button>
            }
          />
        ) : grouped.length === 0 ? (
          <Empty icon="search" title="没有匹配的环境" />
        ) : (
          grouped.map(([manager, list]) => (
            <div key={manager}>
              <div className="mb-2 flex items-center gap-2">
                <Icon
                  name={MANAGER_ICON[manager] ?? "python"}
                  className="h-4 w-4 text-slate-400"
                />
                <span className="text-sm font-semibold text-slate-200">
                  {MANAGER_LABEL[manager] ?? manager}
                </span>
                <Badge tone={MANAGER_TONE[manager] ?? "slate"}>{list.length}</Badge>
              </div>
              <div className="grid gap-2.5 lg:grid-cols-2">
                {list.map((env) => {
                  const isManaged = managedPaths.has(
                    env.path.toLowerCase().replace(/[/\\]+$/, ""),
                  );
                  return (
                    <div
                      key={env.id}
                      className="rounded-md border border-ink-800/70 bg-ink-900 px-3.5 py-3"
                    >
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium text-slate-100">{env.name}</span>
                        {env.pythonVersion && <Badge tone="teal">Python {env.pythonVersion}</Badge>}
                        {env.active && <Badge tone="amber">当前激活</Badge>}
                        {isManaged && (
                          <Badge tone="sky" icon="shield">
                            受管
                          </Badge>
                        )}
                        <button
                          type="button"
                          onClick={() => reveal(env.path)}
                          className="ml-auto shrink-0 rounded-sm border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                          title="打开目录"
                        >
                          <Icon name="folder" className="h-3.5 w-3.5" />
                        </button>
                        {isManaged && (
                          <button
                            type="button"
                            className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                            onClick={() => void remove(env)}
                            title="整个目录移入回收站（可恢复），并清除受管记录"
                          >
                            <Icon name="close" className="h-3.5 w-3.5" />
                          </button>
                        )}
                      </div>
                      <div className="mono mt-1 truncate" title={env.path}>
                        {shortenPath(env.path, 58)}
                      </div>
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-slate-500">
                        <span className="flex items-center gap-1">
                          <Icon name="npm" className="h-3 w-3" />
                          {env.packageCount != null ? `${env.packageCount} 个包` : "包数量未知"}
                        </span>
                        {env.detail && (
                          <span className="flex items-center gap-1">
                            <Icon name="info" className="h-3 w-3" />
                            {env.detail}
                          </span>
                        )}
                        <span className="ml-auto font-mono">{MANAGER_LABEL[env.manager] ?? env.manager}</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ))
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-3">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="plus" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">创建 / 删除（uv）</span>
            <Badge tone="teal" className="ml-auto">
              已实现
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            「创建环境」先展示将执行的 <span className="font-mono">uv venv</span> 命令（T3
            计划确认），创建后读 pyvenv.cfg 记录版本并标记受管；删除时整个目录移入回收站，可一键恢复。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="shield" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">依赖锁定</span>
            <Badge tone="violet" className="ml-auto">
              M3
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            记录 requirements / pyproject 锁定状态，导出 lock 文件并可对比差异。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="link" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">绑定到 Profile</span>
            <Badge tone="violet" className="ml-auto">
              M2
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            把环境绑定到 Profile，同步时写入 Agent 的环境变量或生成激活脚本。
          </p>
        </Card>
      </div>

      <CreateEnvDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onDone={loadManaged}
      />
    </div>
  );
}
