/** 回收站管理：统计、逐条详情、按对象/整条恢复、按期清理与永久删除。 */

import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import {
  Badge,
  Card,
  Drawer,
  Empty,
  Kpi,
  SectionCard,
  SegmentedControl,
} from "../components/ui";
import { ActionDialog, ResultView } from "../components/SkillActions";
import { api, describeError } from "../lib/api";
import { formatBytes, shortenPath } from "../lib/format";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type {
  ActionPlan,
  ActionResult,
  TrashDetail,
  TrashEntry,
  TrashStats,
} from "../lib/types";

const KIND_META: Record<string, { label: string; tone: "violet" | "teal" | "amber" | "slate" }> = {
  link: { label: "链接", tone: "violet" },
  dir: { label: "目录", tone: "teal" },
  mixed: { label: "混合", tone: "amber" },
  unknown: { label: "未知", tone: "slate" },
};

/** 条目名形如 20260928-192740-xxx → 2026-09-28 19:27 */
function entryTime(name: string): string {
  const stamp = name.split("-")[0];
  if (stamp.length < 13) return name;
  return `${stamp.slice(0, 4)}-${stamp.slice(4, 6)}-${stamp.slice(6, 8)} ${stamp.slice(
    8,
    10,
  )}:${stamp.slice(10, 12)}`;
}

export function TrashPage() {
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [entries, setEntries] = useState<TrashEntry[]>([]);
  const [stats, setStats] = useState<TrashStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [kindFilter, setKindFilter] = useState("all");
  const [selected, setSelected] = useState<string[]>([]);
  const [detail, setDetail] = useState<TrashDetail | null>(null);
  const [detailBusy, setDetailBusy] = useState(false);
  const [pickedItems, setPickedItems] = useState<string[]>([]);
  const [quickResult, setQuickResult] = useState<ActionResult | null>(null);

  // 永久删除 / 清空 的确认流程
  const [purgeTarget, setPurgeTarget] = useState<{ title: string; names: string[] } | null>(null);
  const [purgePlan, setPurgePlan] = useState<ActionPlan | null>(null);
  const [purgeError, setPurgeError] = useState<string | null>(null);
  const [purgeResult, setPurgeResult] = useState<ActionResult | null>(null);
  const [purgeBusy, setPurgeBusy] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    Promise.all([api.trashList(), api.trashStats()])
      .then(([list, stat]) => {
        setEntries(list);
        setStats(stat);
        setSelected((prev) => prev.filter((n) => list.some((e) => e.name === n)));
      })
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  }, [setBanner]);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = useMemo(
    () => (kindFilter === "all" ? entries : entries.filter((e) => e.kind === kindFilter)),
    [entries, kindFilter],
  );

  const allSelected = filtered.length > 0 && filtered.every((e) => selected.includes(e.name));

  const toggleAll = () =>
    setSelected(allSelected ? [] : filtered.map((e) => e.name));

  const toggleOne = (name: string) =>
    setSelected((prev) =>
      prev.includes(name) ? prev.filter((n) => n !== name) : [...prev, name],
    );

  const openDetail = async (entry: TrashEntry) => {
    setDetailBusy(true);
    setPickedItems([]);
    try {
      setDetail(await api.trashDetail(entry.name));
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setDetailBusy(false);
    }
  };

  const quickRestore = async (name: string) => {
    try {
      setQuickResult(await api.trashRestore(name));
      setDetail(null);
      load();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const restorePicked = async () => {
    if (!detail) return;
    try {
      setQuickResult(await api.trashRestoreItems(detail.entry.name, pickedItems));
      setDetail(null);
      load();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const beginPurge = async (title: string, names: string[]) => {
    setPurgeTarget({ title, names });
    setPurgePlan(null);
    setPurgeError(null);
    setPurgeResult(null);
    setPurgeBusy(true);
    try {
      setPurgePlan(await api.trashPurgePlan(names));
    } catch (error) {
      setPurgeError(describeError(error));
    } finally {
      setPurgeBusy(false);
    }
  };

  const beginOlderThan = async (days: number) => {
    setPurgeTarget({
      title: days === 0 ? "清空回收站" : `清理 ${days} 天前的条目`,
      names: [],
    });
    setPurgePlan(null);
    setPurgeError(null);
    setPurgeResult(null);
    setPurgeBusy(true);
    try {
      setPurgePlan(await api.trashPurgeOlderPlan(days));
    } catch (error) {
      setPurgeError(describeError(error));
    } finally {
      setPurgeBusy(false);
    }
  };

  const confirmPurge = async () => {
    if (!purgeTarget) return;
    setPurgeBusy(true);
    try {
      const outcome = purgeTarget.names.length
        ? await api.trashPurgeApply(purgeTarget.names)
        : await api.trashPurgeOlderApply(0);
      setPurgeResult(outcome);
      setSelected([]);
      load();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setPurgeBusy(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* 统计 */}
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Kpi label="回收站条目" value={stats?.entries ?? 0} icon="trash" hint="每次删除产生一条记录" />
        <Kpi
          label="可恢复对象"
          value={stats?.items ?? 0}
          icon="folder"
          tone="violet"
          hint="链接与目录都保留完整"
        />
        <Kpi
          label="占用空间"
          value={formatBytes(stats?.bytes ?? 0)}
          icon="cpu"
          tone="amber"
          hint="永久删除后才会释放"
        />
        <Kpi
          label="最早条目"
          value={stats?.oldest ? entryTime(stats.oldest).slice(5, 10) : "—"}
          icon="history"
          tone="slate"
          hint={stats?.newest ? `最新 ${entryTime(stats.newest).slice(5)}` : "回收站为空"}
        />
      </div>

      <SectionCard
        title="回收站管理"
        subtitle="删除的 Skill 链接与目录都先到这里；恢复保留原始内容与链接指向，永久删除不可撤销"
        action={
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => stats && reveal(stats.dir)}
              disabled={!stats}
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开目录
            </button>
            <button type="button" className="btn-ghost btn-sm" onClick={load} disabled={loading}>
              <Icon
                name="refresh"
                className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`}
                strokeWidth={2}
              />
              刷新
            </button>
          </div>
        }
        bodyClassName="space-y-3"
      >
        {/* 工具条 */}
        <div className="flex flex-wrap items-center gap-3">
          <SegmentedControl
            value={kindFilter}
            onChange={setKindFilter}
            options={[
              { value: "all", label: "全部", count: entries.length },
              { value: "link", label: "链接", count: entries.filter((e) => e.kind === "link").length },
              { value: "dir", label: "目录", count: entries.filter((e) => e.kind === "dir").length },
              {
                value: "mixed",
                label: "混合",
                count: entries.filter((e) => e.kind === "mixed").length,
              },
            ]}
          />

          {selected.length > 0 && (
            <div className="flex flex-wrap items-center gap-2 rounded-lg border border-ink-700 bg-ink-900 px-3 py-1.5">
              <span className="text-xs text-slate-300">已选 {selected.length} 条</span>
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={async () => {
                  try {
                    for (const name of selected) {
                      await api.trashRestore(name);
                    }
                    setSelected([]);
                    load();
                  } catch (error) {
                    setBanner(describeError(error));
                  }
                }}
              >
                <Icon name="history" className="h-3.5 w-3.5" />
                恢复选中
              </button>
              <button
                type="button"
                className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                onClick={() => void beginPurge(`永久删除选中的 ${selected.length} 个条目`, selected)}
              >
                <Icon name="trash" className="h-3.5 w-3.5" />
                永久删除选中
              </button>
            </div>
          )}

          <div className="ml-auto flex flex-wrap items-center gap-2">
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => void beginOlderThan(30)}
              disabled={entries.length === 0}
            >
              清理 30 天前
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => void beginOlderThan(7)}
              disabled={entries.length === 0}
            >
              清理 7 天前
            </button>
            <button
              type="button"
              className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
              onClick={() => void beginOlderThan(0)}
              disabled={entries.length === 0}
            >
              <Icon name="trash" className="h-3.5 w-3.5" />
              清空回收站
            </button>
          </div>
        </div>

        {/* 一次性恢复结果 */}
        {quickResult && (
          <div className="rounded-lg border border-ink-700 bg-ink-900 p-3">
            <div className="mb-2 flex items-center gap-2">
              <span className="text-xs font-medium text-slate-300">上次操作结果</span>
              <button
                type="button"
                className="btn-ghost btn-sm ml-auto"
                onClick={() => setQuickResult(null)}
              >
                关闭
              </button>
            </div>
            <ResultView result={quickResult} onReveal={reveal} />
          </div>
        )}

        {/* 列表 */}
        {filtered.length === 0 ? (
          <Empty
            icon="trash"
            title={entries.length === 0 ? "回收站是空的" : "没有匹配的条目"}
            description={
              entries.length === 0
                ? "删除 Skill 链接或目录、批量清理失效链接时，被删对象都会先移入这里 —— 链接保留原有指向，目录保留完整内容。"
                : "换个筛选条件试试。"
            }
          />
        ) : (
          <div className="space-y-2">
            <label className="flex cursor-pointer items-center gap-2.5 px-1 text-xs text-slate-400">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={toggleAll}
                className="accent-teal-400"
              />
              全选当前筛选结果（{filtered.length} 条）
            </label>

            {filtered.map((entry) => {
              const meta = KIND_META[entry.kind] ?? KIND_META.unknown;
              return (
                <div
                  key={entry.name}
                  className="rounded-lg border border-ink-800/70 bg-ink-900 px-3 py-2.5 transition-colors hover:bg-ink-800"
                >
                  <div className="flex flex-wrap items-center gap-2.5">
                    <input
                      type="checkbox"
                      checked={selected.includes(entry.name)}
                      onChange={() => toggleOne(entry.name)}
                      className="accent-teal-400"
                    />
                    <Badge tone={meta.tone} icon="folder">
                      {meta.label}
                    </Badge>
                    <span className="min-w-0 flex-1 truncate text-xs text-slate-300">
                      {entry.original ? shortenPath(entry.original, 58) : entry.summary || entry.name}
                    </span>
                    <span className="font-mono text-[10.5px] text-slate-500">
                      {entry.itemCount} 个对象 · {formatBytes(entry.size)}
                    </span>
                    <span className="font-mono text-[10.5px] text-slate-500">
                      {entryTime(entry.name)}
                    </span>
                  </div>

                  {entry.original && entry.summary && (
                    <div className="mt-0.5 pl-7 text-[10.5px] text-slate-500">{entry.summary}</div>
                  )}

                  <div className="mt-1.5 flex flex-wrap items-center gap-2 pl-7">
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() => void openDetail(entry)}
                      disabled={detailBusy}
                    >
                      <Icon name="search" className="h-3.5 w-3.5" />
                      查看内容
                    </button>
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() => void quickRestore(entry.name)}
                    >
                      <Icon name="history" className="h-3.5 w-3.5" />
                      恢复{entry.itemCount > 1 ? `这 ${entry.itemCount} 项` : "到原位"}
                    </button>
                    <button
                      type="button"
                      className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                      onClick={() => void beginPurge("永久删除回收站条目", [entry.name])}
                    >
                      <Icon name="trash" className="h-3.5 w-3.5" />
                      永久删除
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </SectionCard>

      {/* 条目详情抽屉 */}
      <Drawer
        open={detail != null}
        onClose={() => setDetail(null)}
        title="回收站条目内容"
        subtitle={detail?.entry.name}
        width="max-w-3xl"
      >
        {detail && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={(KIND_META[detail.entry.kind] ?? KIND_META.unknown).tone} icon="folder">
                {(KIND_META[detail.entry.kind] ?? KIND_META.unknown).label}
              </Badge>
              <span className="text-xs text-slate-300">{detail.entry.summary}</span>
              <span className="font-mono text-[10.5px] text-slate-500">
                {detail.items.length} 个对象 · {formatBytes(detail.entry.size)} ·{" "}
                {entryTime(detail.entry.name)}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => void restorePicked()}
                >
                  <Icon name="history" className="h-3.5 w-3.5" />
                  {pickedItems.length > 0 ? `恢复选中的 ${pickedItems.length} 项` : "恢复全部"}
                </button>
              </div>
            </div>

            <div className="overflow-hidden rounded-lg border border-ink-800/70">
              <table className="w-full border-collapse">
                <thead>
                  <tr>
                    <th className="table-head w-10 px-3 py-2"> </th>
                    <th className="table-head px-3 py-2">原位置</th>
                    <th className="table-head w-20 px-3 py-2">类型</th>
                    <th className="table-head w-24 px-3 py-2">大小</th>
                    <th className="table-head w-24 px-3 py-2">可恢复</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.items.map((item) => (
                    <tr key={item.stored} className="hover:bg-ink-800">
                      <td className="table-cell px-3">
                        <input
                          type="checkbox"
                          checked={pickedItems.includes(item.stored)}
                          onChange={(e) =>
                            setPickedItems((prev) =>
                              e.target.checked
                                ? [...prev, item.stored]
                                : prev.filter((s) => s !== item.stored),
                            )
                          }
                          className="accent-teal-400"
                        />
                      </td>
                      <td className="table-cell px-3">
                        <span className="mono block truncate" title={item.original}>
                          {shortenPath(item.original, 62)}
                        </span>
                        {item.linkTarget && (
                          <span className="mono block truncate text-[10.5px] text-slate-600">
                            → {item.linkTarget}
                          </span>
                        )}
                      </td>
                      <td className="table-cell px-3">
                        <Badge tone={item.kind === "link" ? "violet" : "teal"}>
                          {item.kind === "link" ? "链接" : "目录"}
                        </Badge>
                      </td>
                      <td className="table-cell px-3 font-mono text-[11px] text-slate-400">
                        {formatBytes(item.size)}
                      </td>
                      <td className="table-cell px-3">
                        {item.restorable ? (
                          <Badge tone="teal" icon="check">
                            可恢复
                          </Badge>
                        ) : (
                          <Badge tone="amber" icon="alert">
                            原位置被占用
                          </Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <Card className="border-ink-700/60">
              <div className="flex items-start gap-2 text-[11px] leading-relaxed text-slate-500">
                <Icon name="info" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>
                  勾选后点「恢复选中的 N 项」可只还原其中一部分；整条恢复会把全部对象放回原位置。
                  永久删除需要回到列表操作。
                </span>
              </div>
            </Card>
          </div>
        )}
      </Drawer>

      {/* 永久删除确认 */}
      <ActionDialog
        open={purgeTarget != null}
        onClose={() => setPurgeTarget(null)}
        title={purgeTarget?.title ?? ""}
        subtitle="此操作不可恢复"
        plan={purgePlan}
        result={purgeResult}
        error={purgeError}
        busy={purgeBusy}
        danger
        confirmLabel="永久删除"
        onConfirm={() => void confirmPurge()}
        onReveal={reveal}
      />
    </div>
  );
}