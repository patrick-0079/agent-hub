/** 历史与审计：扫描记录 + 可恢复操作清单（回收站已独立成页）。 */

import React, { useCallback, useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { Badge, Card, Empty, SectionCard } from "../components/ui";
import { ResultView } from "../components/SkillActions";
import { api, describeError } from "../lib/api";
import { formatBytes, formatDuration, formatTime, relativeTime, shortenPath } from "../lib/format";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { ActionResult, BackupInfo, ManifestInfo, SnapshotMeta, TrashStats } from "../lib/types";

const OP_LABEL: Record<string, string> = {
  "skill-import": "导入 Skill",
  "skill-cleanup-broken": "清理失效链接",
  "skill-relink": "重建链接",
  "skill-delete": "删除 Skill",
  "undo-import": "撤销导入",
};

export function HistoryPage() {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);
  const navigate = useApp((s) => s.navigate);
  const reveal = useReveal();

  const [items, setItems] = useState<SnapshotMeta[]>([]);
  const [manifests, setManifests] = useState<ManifestInfo[]>([]);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [trashStats, setTrashStats] = useState<TrashStats | null>(null);
  const [result, setResult] = useState<ActionResult | null>(null);
  const [loading, setLoading] = useState(true);

  const loadAll = useCallback(() => {
    setLoading(true);
    Promise.all([
      api.snapshotHistory(40),
      api.manifestsList(30),
      api.trashStats(),
      api.backupsList(30),
    ])
      .then(([history, manifestList, stats, backupList]) => {
        setItems(history);
        setManifests(manifestList);
        setTrashStats(stats);
        setBackups(backupList);
      })
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  }, [setBanner]);

  useEffect(() => {
    loadAll();
  }, [snapshot?.scannedAt, loadAll]);

  const restore = async (fn: Promise<ActionResult>) => {
    try {
      setResult(await fn);
      loadAll();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  return (
    <div className="space-y-4">
      {/* 恢复结果 */}
      {result && (
        <SectionCard
          title="恢复结果"
          action={
            <button type="button" className="btn-ghost btn-sm" onClick={() => setResult(null)}>
              <Icon name="close" className="h-3.5 w-3.5" />
              关闭
            </button>
          }
        >
          <ResultView result={result} onReveal={reveal} />
        </SectionCard>
      )}

      <div className="grid gap-4 xl:grid-cols-2">
        {/* 操作清单 */}
        <SectionCard
          title="操作清单（可恢复）"
          subtitle={`每次写入前都会记录清单，共 ${manifests.length} 份`}
          action={
            <button type="button" className="btn-ghost btn-sm" onClick={() => void api.manifestsList(30)}>
              <Icon name="refresh" className="h-3.5 w-3.5" />
              刷新
            </button>
          }
          bodyClassName="space-y-2"
        >
          {manifests.length === 0 ? (
            <Empty
              icon="shield"
              title={loading ? "正在读取…" : "暂无操作清单"}
              description="执行导入 / 清理 / 重建 / 删除后，这里会记录每一步的来源与目标，可依清单撤销或重建。"
            />
          ) : (
            manifests.map((item) => (
              <div
                key={item.path}
                className="rounded-lg border border-ink-800/70 bg-ink-900 px-3 py-2.5"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <Badge tone="violet" icon="shield">
                    {OP_LABEL[item.op] ?? item.op}
                  </Badge>
                  <span className="text-xs text-slate-300">{item.summary}</span>
                  <span className="ml-auto font-mono text-[10.5px] text-slate-500">
                    {item.entries} 项 · {item.createdAt}
                  </span>
                </div>
                <div className="mono mt-1 truncate text-[10.5px]" title={item.path}>
                  {shortenPath(item.path, 62)}
                </div>
                <div className="mt-1.5 flex items-center gap-2">
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => void restore(api.manifestRestore(item.path))}
                  >
                    <Icon name="history" className="h-3.5 w-3.5" />
                    依清单恢复
                  </button>
                  <button type="button" className="btn-ghost btn-sm" onClick={() => reveal(item.path)}>
                    打开目录
                  </button>
                </div>
              </div>
            ))
          )}
        </SectionCard>

        {/* 配置备份（T2 写入前自动生成） */}
        <SectionCard
          title="配置备份"
          subtitle={`每次写入 Agent 配置前自动备份，共 ${backups.length} 份 · 可一键回滚`}
          action={
            <button type="button" className="btn-ghost btn-sm" onClick={loadAll}>
              <Icon name="refresh" className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} strokeWidth={2} />
              刷新
            </button>
          }
          bodyClassName="space-y-2"
        >
          {backups.length === 0 ? (
            <Empty
              icon="shield"
              title="暂无备份"
              description="用「MCP → 分发到 Agent」写入配置文件时，会先把目标文件备份到这里；回滚前也会再备份一次当前内容。"
            />
          ) : (
            backups.map((item) => (
              <div
                key={item.id}
                className="rounded-lg border border-ink-800/70 bg-ink-900 px-3 py-2.5"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <Icon name="shield" className="h-3.5 w-3.5 shrink-0 text-brand-400" />
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-slate-300" title={item.target}>
                    {shortenPath(item.target, 46)}
                  </span>
                  <span className="font-mono text-[10.5px] text-slate-500">
                    {formatBytes(item.bytes)}
                  </span>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-2">
                  <span className="text-[10.5px] text-slate-500">{item.createdAt}</span>
                  {item.note && (
                    <span className="truncate text-[10.5px] text-slate-600">{item.note}</span>
                  )}
                  <div className="ml-auto flex items-center gap-2">
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() => reveal(item.backupPath)}
                    >
                      查看文件
                    </button>
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() => void restore(api.backupRestore(item.id))}
                    >
                      <Icon name="history" className="h-3.5 w-3.5" />
                      回滚到此备份
                    </button>
                  </div>
                </div>
              </div>
            ))
          )}
        </SectionCard>
      </div>

      {/* 回收站入口（详细管理在独立页面） */}
      <Card>
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-lg border border-ink-700 bg-ink-850 p-2 text-slate-400">
            <Icon name="trash" className="h-4 w-4" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-sm text-slate-200">
              回收站：{trashStats?.entries ?? 0} 个条目 · {trashStats?.items ?? 0} 个对象 ·{" "}
              {formatBytes(trashStats?.bytes ?? 0)}
            </div>
            <div className="mt-0.5 text-[11px] text-slate-500">
              删除的链接与目录都先进入回收站，可在独立页面里查看内容、按对象恢复、按期清理或永久删除。
            </div>
          </div>
          <button type="button" className="btn-ghost btn-sm" onClick={() => navigate("trash")}>
            <Icon name="trash" className="h-3.5 w-3.5" />
            管理回收站
          </button>
        </div>
      </Card>

      {/* 扫描历史 */}
      <SectionCard
        title="扫描历史"
        subtitle={`本地数据库保留最近 ${items.length} 次扫描记录`}
        action={
          <Badge tone="slate" icon="history">
            {loading ? "加载中…" : `${items.length} 条`}
          </Badge>
        }
      >
        {items.length === 0 ? (
          <Empty
            icon="history"
            title={loading ? "正在读取历史…" : "暂无历史记录"}
            description="每次扫描都会写入一条快照记录，包含当次发现的 Agent、Skill、MCP、Python 环境与全局包数量。"
          />
        ) : (
          <div className="relative pl-6">
            <span className="absolute bottom-2 left-[9px] top-2 w-px bg-ink-700" />
            <div className="space-y-3">
              {items.map((item, index) => (
                <div key={item.id} className="relative">
                  <span
                    className={`absolute -left-6 top-3 h-[9px] w-[9px] rounded-full border-2 ${
                      index === 0 ? "border-brand-500 bg-brand-900" : "border-ink-600 bg-ink-850"
                    }`}
                  />
                  <Card
                    className={`border-ink-800/70 ${index === 0 ? "border-brand-500/30" : ""}`}
                    padded={false}
                  >
                    <div className="flex flex-wrap items-center gap-3 px-4 py-2.5">
                      <span className="text-sm text-slate-200">{formatTime(item.scannedAt)}</span>
                      <span className="text-[11px] text-slate-500">{relativeTime(item.scannedAt)}</span>
                      {index === 0 && (
                        <Badge tone="teal" icon="check">
                          最新
                        </Badge>
                      )}
                      <Badge tone="slate">耗时 {formatDuration(item.durationMs)}</Badge>
                      <div className="ml-auto flex flex-wrap items-center gap-1.5">
                        <Badge tone="teal" icon="agents">
                          Agent {item.agents}
                        </Badge>
                        <Badge tone="violet" icon="skills">
                          Skill {item.skills}
                        </Badge>
                        <Badge tone="sky" icon="mcp">
                          MCP {item.mcpServers}
                        </Badge>
                        <Badge tone="slate" icon="python">
                          Python {item.pythonEnvs}
                        </Badge>
                        <Badge tone="amber" icon="npm">
                          npm {item.npmPackages}
                        </Badge>
                      </div>
                    </div>
                  </Card>
                </div>
              ))}
            </div>
          </div>
        )}
      </SectionCard>

      <div className="grid gap-3 md:grid-cols-2">
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="adapter" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">Agent 配置同步审计</span>
            <Badge tone="violet" className="ml-auto">
              M2
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            配置写入的 diff 与备份会并入同一条时间线（当前已覆盖 Skill 类操作）。
          </p>
        </Card>
        <Card className="border-dashed">
          <div className="flex items-center gap-2">
            <Icon name="search" className="h-4 w-4 text-accent-400" />
            <span className="text-sm text-slate-200">快照对比</span>
            <Badge tone="violet" className="ml-auto">
              M1
            </Badge>
          </div>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
            任意两次扫描结果可直接对比，看清环境这期间发生了哪些变化。
          </p>
        </Card>
      </div>
    </div>
  );
}