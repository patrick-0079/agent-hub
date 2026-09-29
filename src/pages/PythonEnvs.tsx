/** Python 环境：conda / uv / venv 统一视图（创建向导 M3）。 */

import React, { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { Badge, Card, Empty, SearchInput, SectionCard } from "../components/ui";
import { shortenPath } from "../lib/format";
import { MANAGER_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { PythonEnv } from "../lib/types";

const MANAGER_TONE: Record<string, "teal" | "violet" | "sky"> = {
  conda: "teal",
  uv: "violet",
  venv: "sky",
};

const MANAGER_ICON: Record<string, "python" | "sparkle" | "folder"> = {
  conda: "python",
  uv: "sparkle",
  venv: "folder",
};

export function PythonPage() {
  const snapshot = useApp((s) => s.snapshot);
  const reveal = useReveal();
  const [query, setQuery] = useState("");

  const envs = snapshot?.pythonEnvs ?? [];

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
        subtitle={`共 ${envs.length} 个环境（conda / uv / venv 三类统一视图）`}
        action={
          <div className="w-56">
            <SearchInput value={query} onChange={setQuery} placeholder="搜索名称 / 路径 / 版本…" />
          </div>
        }
        bodyClassName="space-y-5"
      >
        {envs.length === 0 ? (
          <Empty
            icon="python"
            title="未发现 Python 环境"
            description="AgentHub 会读取 conda env list、uv python list，并在主目录与自定义目录中搜索 pyvenv.cfg。可在设置里指定 uv / conda 的路径或扩大扫描范围。"
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
                {list.map((env) => (
                  <div
                    key={env.id}
                    className="rounded-lg border border-ink-800/70 bg-ink-900 px-3.5 py-3"
                  >
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm font-medium text-slate-100">{env.name}</span>
                      {env.pythonVersion && <Badge tone="teal">Python {env.pythonVersion}</Badge>}
                      {env.active && <Badge tone="amber">当前激活</Badge>}
                      <button
                        type="button"
                        onClick={() => reveal(env.path)}
                        className="ml-auto shrink-0 rounded-md border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                        title="打开目录"
                      >
                        <Icon name="folder" className="h-3.5 w-3.5" />
                      </button>
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
                ))}
              </div>
            </div>
          ))
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-3">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="plus" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">创建向导</span>
            <Badge tone="violet" className="ml-auto">
              M3
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            分步向导：选管理器 → 选 Python 版本 → 命名与位置 → 依赖来源 → 实时执行输出。
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
    </div>
  );
}