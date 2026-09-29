/** Agent 定义与能力：内核只提供分级基础能力，Agent 的一切都来自可编辑的定义文件。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon, type IconName } from "../components/Icon";
import {
  Badge,
  Card,
  Drawer,
  Empty,
  SearchInput,
  SectionCard,
  SegmentedControl,
  type Tone,
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type {
  CapabilitySpec,
  CapabilityTier,
  DefinitionsView,
  LoadedDef,
  PathRule,
} from "../lib/types";

const TIER_META: Record<CapabilityTier, { label: string; code: string; tone: Tone; icon: IconName; detail: string }> = {
  observe: {
    label: "观察",
    code: "T0",
    tone: "teal",
    icon: "search",
    detail: "只读元数据，无副作用，扫描时自动执行",
  },
  parse: {
    label: "解析",
    code: "T1",
    tone: "sky",
    icon: "terminal",
    detail: "只读文件内容并解析，扫描时自动执行",
  },
  deploy: {
    label: "部署",
    code: "T2",
    tone: "violet",
    icon: "link",
    detail: "写磁盘（部署 Skill / 写托管块），写前备份、可回滚，需用户确认",
  },
  mutate: {
    label: "变更",
    code: "T3",
    tone: "rose",
    icon: "alert",
    detail: "调外部程序改变系统状态（装包 / 建环境），需二次确认",
  },
};

const TIER_ORDER: CapabilityTier[] = ["observe", "parse", "deploy", "mutate"];

const ROLE_META: Record<string, { label: string; tone: Tone }> = {
  install: { label: "程序本体", tone: "teal" },
  config: { label: "配置", tone: "sky" },
  data: { label: "数据", tone: "slate" },
  skills: { label: "Skills", tone: "violet" },
  mcp: { label: "MCP", tone: "amber" },
  provider: { label: "供应商", tone: "rose" },
};

const STRENGTH_META: Record<string, { label: string; tone: Tone }> = {
  strong: { label: "强证据", tone: "teal" },
  medium: { label: "中证据", tone: "amber" },
  weak: { label: "弱证据", tone: "slate" },
};

export function AgentDefsPage() {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [catalog, setCatalog] = useState<CapabilitySpec[]>([]);
  const [view, setView] = useState<DefinitionsView | null>(null);
  const [loading, setLoading] = useState(true);
  const [tierFilter, setTierFilter] = useState<string>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<LoadedDef | null>(null);
  const [fileText, setFileText] = useState<string>("");
  const [draft, setDraft] = useState<string>("");
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const [caps, defs] = await Promise.all([api.capabilityCatalog(), api.agentDefinitions()]);
      setCatalog(caps);
      setView(defs);
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 每个能力被多少个 Agent 定义引用 */
  const usage = useMemo(() => {
    const map = new Map<string, { agents: number; refs: number }>();
    view?.definitions.forEach((def) => {
      def.usedCapabilities.forEach((used) => {
        const entry = map.get(used.id) ?? { agents: 0, refs: 0 };
        entry.agents += 1;
        entry.refs += used.count;
        map.set(used.id, entry);
      });
    });
    return map;
  }, [view]);

  const filteredCatalog = useMemo(() => {
    const q = query.trim().toLowerCase();
    return catalog.filter((cap) => {
      if (tierFilter !== "all" && cap.tier !== tierFilter) return false;
      if (!q) return true;
      return (
        cap.id.toLowerCase().includes(q) ||
        cap.label.toLowerCase().includes(q) ||
        cap.description.toLowerCase().includes(q)
      );
    });
  }, [catalog, tierFilter, query]);

  const definitions = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return view?.definitions ?? [];
    return (view?.definitions ?? []).filter(
      (def) =>
        def.file.agent.id.toLowerCase().includes(q) ||
        def.file.agent.name.toLowerCase().includes(q) ||
        def.file.agent.vendor.toLowerCase().includes(q),
    );
  }, [view, query]);

  const openDefinition = async (def: LoadedDef) => {
    setSelected(def);
    setEditing(false);
    try {
      const path = `${view?.userDir}\\${def.file.agent.id}.toml`;
      const preview = await api.readTextPreview(path, 256 * 1024);
      setFileText(preview.exists ? preview.text : "");
      setDraft(preview.text);
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const save = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      const next = await api.saveAgentDefinition(selected.file.agent.id, draft);
      setView(next);
      setEditing(false);
      const saved = next.definitions.find((d) => d.file.agent.id === selected.file.agent.id);
      if (saved) setSelected(saved);
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      const next = await api.resetAgentDefinition(selected.file.agent.id);
      setView(next);
      setEditing(false);
      const restored = next.definitions.find((d) => d.file.agent.id === selected.file.agent.id);
      if (restored) setSelected(restored);
      const preview = await api.readTextPreview(
        `${next.userDir}\\${selected.file.agent.id}.toml`,
        256 * 1024,
      );
      setFileText(preview.exists ? preview.text : "");
      setDraft(preview.text);
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const exportBuiltins = async () => {
    try {
      const count = await api.seedAgentDefinitions();
      setBanner(
        count > 0
          ? `已导出 ${count} 个内置定义到用户目录，可直接编辑`
          : "所有内置定义都已存在于用户目录",
      );
      await load();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const implementedCount = catalog.filter((c) => c.implemented).length;

  return (
    <div className="space-y-4">
      {/* 分级概览 */}
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {TIER_ORDER.map((tier) => {
          const meta = TIER_META[tier];
          const items = catalog.filter((c) => c.tier === tier);
          const implemented = items.filter((c) => c.implemented).length;
          return (
            <button
              key={tier}
              type="button"
              onClick={() => setTierFilter(tierFilter === tier ? "all" : tier)}
              className={`card p-4 text-left transition-colors ${
                tierFilter === tier ? "border-brand-500/50 bg-ink-800" : "hover:bg-ink-800"
              }`}
            >
              <div className="flex items-center gap-2">
                <span className={`rounded-lg border p-1.5 ${
                  meta.tone === "teal"
                    ? "border-brand-500/40 bg-brand-900 text-brand-400"
                    : meta.tone === "sky"
                      ? "border-sky-500/40 bg-sky-950 text-sky-300"
                      : meta.tone === "violet"
                        ? "border-accent-500/40 bg-accent-900 text-accent-400"
                        : "border-rose-500/40 bg-rose-950 text-rose-300"
                }`}>
                  <Icon name={meta.icon} className="h-4 w-4" />
                </span>
                <div>
                  <div className="text-sm font-semibold text-slate-100">
                    {meta.code} · {meta.label}
                  </div>
                  <div className="text-[11px] text-slate-500">
                    {items.length} 个能力 · 已实现 {implemented}
                  </div>
                </div>
              </div>
              <p className="mt-2 text-[11px] leading-relaxed text-slate-500">{meta.detail}</p>
            </button>
          );
        })}
      </div>

      {/* 能力目录 */}
      <SectionCard
        title="内核能力目录"
        subtitle={`共 ${catalog.length} 个基础能力（已实现 ${implementedCount} 个），所有 Agent 定义都只能引用这里的原语`}
        action={
          <div className="flex items-center gap-2">
            <SegmentedControl
              value={tierFilter}
              onChange={setTierFilter}
              options={[
                { value: "all", label: "全部", count: catalog.length },
                ...TIER_ORDER.map((t) => ({
                  value: t,
                  label: TIER_META[t].code,
                  count: catalog.filter((c) => c.tier === t).length,
                })),
              ]}
            />
            <div className="w-48">
              <SearchInput value={query} onChange={setQuery} placeholder="搜索能力…" />
            </div>
          </div>
        }
        bodyClassName="p-0"
      >
        <div className="overflow-auto">
          <table className="w-full border-collapse">
            <thead>
              <tr>
                <th className="table-head w-20 px-3 py-2">级别</th>
                <th className="table-head w-52 px-3 py-2">能力</th>
                <th className="table-head px-3 py-2">说明</th>
                <th className="table-head w-32 px-3 py-2">参数</th>
                <th className="table-head w-24 px-3 py-2">状态</th>
                <th className="table-head w-24 px-3 py-2">被引用</th>
              </tr>
            </thead>
            <tbody>
              {filteredCatalog.map((cap) => {
                const meta = TIER_META[cap.tier];
                const used = usage.get(cap.id);
                return (
                  <tr key={cap.id} className="hover:bg-ink-800">
                    <td className="table-cell px-3">
                      <Badge tone={meta.tone}>{meta.code}</Badge>
                    </td>
                    <td className="table-cell px-3">
                      <div className="font-mono text-xs text-slate-200">{cap.id}</div>
                      <div className="text-[11px] text-slate-500">{cap.label}</div>
                    </td>
                    <td className="table-cell px-3 text-xs leading-relaxed text-slate-400">
                      {cap.description}
                    </td>
                    <td className="table-cell px-3">
                      <div className="flex flex-wrap gap-1">
                        {cap.params.map((p) => (
                          <span
                            key={p}
                            className="rounded border border-ink-700 bg-ink-950 px-1.5 py-0.5 font-mono text-[10px] text-slate-500"
                          >
                            {p}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td className="table-cell px-3">
                      {cap.implemented ? (
                        <Badge tone="teal" icon="check">
                          已实现
                        </Badge>
                      ) : (
                        <Badge tone="slate">待实现</Badge>
                      )}
                    </td>
                    <td className="table-cell px-3">
                      {used ? (
                        <span className="font-mono text-[11px] text-slate-400">
                          {used.agents} 定义 / {used.refs} 处
                        </span>
                      ) : (
                        <span className="text-[11px] text-slate-600">—</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </SectionCard>

      {/* Agent 定义 */}
      <SectionCard
        title="Agent 定义"
        subtitle={`共 ${view?.definitions.length ?? 0} 个定义（内置 ${view?.builtinCount ?? 0} · 用户 ${view?.userCount ?? 0}）—— 内核不含任何 Agent 专属逻辑`}
        action={
          <div className="flex items-center gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={() => void exportBuiltins()}>
              <Icon name="plus" className="h-3.5 w-3.5" />
              导出内置定义
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => view && reveal(view.userDir)}
              disabled={!view}
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开定义目录
            </button>
            <button type="button" className="btn-ghost btn-sm" onClick={() => void load()} disabled={loading}>
              <Icon name="refresh" className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} strokeWidth={2} />
              重新加载
            </button>
          </div>
        }
        bodyClassName="space-y-3"
      >
        {view && (
          <div className="flex flex-wrap items-center gap-3 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2">
            <Icon name="folder" className="h-3.5 w-3.5 shrink-0 text-slate-500" />
            <span className="mono min-w-0 flex-1 truncate" title={view.userDir}>
              {view.userDir}
            </span>
            <span className="text-[11px] text-slate-500">
              同名文件会覆盖内置定义；改动后重新扫描即生效
            </span>
          </div>
        )}

        {view && view.warnings.length > 0 && (
          <div className="space-y-1.5">
            {view.warnings.map((w, i) => (
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

        {definitions.length === 0 ? (
          <Empty icon="adapter" title={loading ? "正在加载定义…" : "没有匹配的定义"} />
        ) : (
          <div className="grid gap-3 xl:grid-cols-2">
            {definitions.map((def) => (
              <DefinitionCard
                key={def.file.agent.id}
                def={def}
                status={snapshot?.agents.find((a) => a.id === def.file.agent.id)?.status}
                onOpen={() => void openDefinition(def)}
              />
            ))}
          </div>
        )}
      </SectionCard>

      {/* 定义详情 / 编辑器 */}
      <Drawer
        open={selected != null}
        onClose={() => setSelected(null)}
        title={selected?.file.agent.name ?? ""}
        subtitle={`${selected?.file.agent.id ?? ""} · ${selected?.source ?? ""}`}
        width="max-w-3xl"
      >
        {selected && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={selected.fromUserDir ? "violet" : "slate"} icon="adapter">
                {selected.fromUserDir ? "用户定义" : "内置定义"}
              </Badge>
              <Badge tone="slate">{selected.file.agent.kind}</Badge>
              <Badge tone="teal">
                能力上限 {TIER_META[selected.file.capabilities.maxTier]?.code ?? "—"} ·{" "}
                {TIER_META[selected.file.capabilities.maxTier]?.label ?? "未知"}
              </Badge>
              <div className="ml-auto flex items-center gap-2">
                {selected.fromUserDir && (
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => void reset()}
                    disabled={busy}
                  >
                    恢复内置
                  </button>
                )}
                {editing ? (
                  <>
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() => {
                        setEditing(false);
                        setDraft(fileText);
                      }}
                    >
                      取消
                    </button>
                    <button
                      type="button"
                      className="btn-primary btn-sm"
                      onClick={() => void save()}
                      disabled={busy || draft === fileText}
                    >
                      保存
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => setEditing(true)}
                    disabled={!draft}
                  >
                    <Icon name="settings" className="h-3.5 w-3.5" />
                    编辑
                  </button>
                )}
              </div>
            </div>

            {editing ? (
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                spellCheck={false}
                className="h-[520px] w-full rounded-lg border border-ink-700 bg-ink-950 p-3 font-mono text-[11.5px] leading-relaxed text-slate-300 outline-none focus:border-brand-500"
              />
            ) : (
              <>
                <DefinitionDetails def={selected} />
                <div>
                  <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                    定义源码
                  </div>
                  <pre className="max-h-72 overflow-auto rounded-lg border border-ink-800 bg-ink-950 p-3 text-[11px] leading-relaxed text-slate-300">
                    {fileText || "（尚未导出到用户目录，点击「导出内置定义」后即可查看与编辑）"}
                  </pre>
                </div>
              </>
            )}
          </div>
        )}
      </Drawer>
    </div>
  );
}

function DefinitionCard({
  def,
  status,
  onOpen,
}: {
  def: LoadedDef;
  status?: string;
  onOpen: () => void;
}) {
  const skills = def.file.paths.filter((p) => p.role === "skills");
  const policy = def.file.capabilities;
  return (
    <button
      type="button"
      onClick={onOpen}
      className="card p-4 text-left transition-colors hover:border-brand-500/40 hover:bg-ink-800"
    >
      <div className="flex items-start gap-3">
        <span className="h-8 w-1.5 shrink-0 rounded" style={{ background: def.file.agent.accent }} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-semibold text-slate-100">
              {def.file.agent.name || def.file.agent.id}
            </span>
            <span className="text-[11px] text-slate-500">{def.file.agent.vendor}</span>
            <Badge tone={def.fromUserDir ? "violet" : "slate"}>
              {def.fromUserDir ? "用户" : "内置"}
            </Badge>
            <Badge tone="slate">{def.file.agent.kind}</Badge>
            {status && (
              <Badge
                tone={
                  status === "installed"
                    ? "teal"
                    : status === "configured"
                      ? "amber"
                      : status === "leftover"
                        ? "rose"
                        : "slate"
                }
              >
                {status === "installed"
                  ? "已安装"
                  : status === "configured"
                    ? "仅配置"
                    : status === "leftover"
                      ? "残留"
                      : "未安装"}
              </Badge>
            )}
          </div>
          <div className="mono mt-0.5 truncate">{def.file.agent.id}.toml</div>
        </div>
        <Icon name="chevronRight" className="mt-1 h-4 w-4 shrink-0 text-slate-600" />
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        <Badge tone="teal">{def.file.evidence.length} 条显式证据</Badge>
        <Badge tone="slate">{def.file.paths.length} 条路径</Badge>
        {def.file.mcp.length > 0 && <Badge tone="amber">MCP × {def.file.mcp.length}</Badge>}
        {def.file.provider.length > 0 && (
          <Badge tone="rose">供应商 × {def.file.provider.length}</Badge>
        )}
        {skills.length > 0 && (
          <Badge tone="violet">
            Skills × {skills.length}
            {skills[0].deploy ? ` · ${skills[0].deploy}` : ""}
          </Badge>
        )}
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-1.5 border-t border-ink-800/70 pt-2">
        <span className="text-[10.5px] text-slate-600">能力上限</span>
        {TIER_ORDER.filter(
          (t) => TIER_ORDER.indexOf(t) <= TIER_ORDER.indexOf(policy.maxTier),
        ).map((t) => (
          <span
            key={t}
            className="rounded border border-ink-700 bg-ink-950 px-1.5 py-0.5 font-mono text-[10px] text-slate-500"
          >
            {TIER_META[t].code}
          </span>
        ))}
        {policy.skillMethods.length > 0 && (
          <span className="ml-2 text-[10.5px] text-slate-600">
            Skill 部署：{policy.skillMethods.join(" / ")}
          </span>
        )}
      </div>
    </button>
  );
}

function DefinitionDetails({ def }: { def: LoadedDef }) {
  const roles = useMemo(() => {
    const map = new Map<string, PathRule[]>();
    def.file.paths.forEach((p) => {
      const arr = map.get(p.role) ?? [];
      arr.push(p);
      map.set(p.role, arr);
    });
    return Array.from(map.entries());
  }, [def]);

  return (
    <div className="space-y-3">
      {/* 证据规则 */}
      <div>
        <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-slate-500">
          显式证据规则（{def.file.evidence.length}）
        </div>
        <div className="space-y-1">
          {def.file.evidence.map((rule, i) => (
            <div
              key={`${rule.capability}-${i}`}
              className="rounded-md border border-ink-800 bg-ink-900 px-2.5 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-[11.5px] text-brand-400">{rule.capability}</span>
                <Badge tone={STRENGTH_META[rule.strength]?.tone ?? "slate"}>
                  {STRENGTH_META[rule.strength]?.label ?? rule.strength}
                </Badge>
                <span className="text-[11.5px] text-slate-300">{rule.label}</span>
              </div>
              <div className="mono mt-1 break-all text-[10.5px]">
                {rule.args.length > 0 && <span>args={JSON.stringify(rule.args)} </span>}
                {rule.paths.length > 0 && <span>paths={JSON.stringify(rule.paths)} </span>}
                {rule.root && <span>root={rule.root} </span>}
              </div>
            </div>
          ))}
          {def.file.evidence.length === 0 && (
            <p className="text-xs text-slate-600">无显式证据，判定完全由路径角色派生</p>
          )}
        </div>
      </div>

      {/* 路径角色 */}
      <div>
        <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-slate-500">
          路径与角色（{def.file.paths.length}）
        </div>
        <div className="space-y-1.5">
          {roles.map(([role, items]) => (
            <div key={role}>
              <div className="mb-1 flex items-center gap-1.5">
                <Badge tone={ROLE_META[role]?.tone ?? "slate"}>
                  {ROLE_META[role]?.label ?? role}
                </Badge>
                <span className="text-[10.5px] text-slate-600">
                  {role === "install" && "→ 强证据"}
                  {role === "config" && "→ 中证据"}
                  {(role === "data" || role === "skills") && "→ 弱证据"}
                  {(role === "mcp" || role === "provider") && "→ 仅用于提取"}
                </span>
              </div>
              <div className="space-y-0.5">
                {items.map((p) => (
                  <div key={p.path} className="flex items-center gap-2 pl-1">
                    <span className="w-32 shrink-0 truncate text-[11px] text-slate-400">
                      {p.label}
                    </span>
                    <span className="mono min-w-0 flex-1 truncate" title={p.path}>
                      {p.path}
                    </span>
                    {p.format && <span className="font-mono text-[10px] text-slate-600">{p.format}</span>}
                    {p.deploy && (
                      <Badge tone="violet">{p.deploy === "link" ? "链接部署" : "拷贝部署"}</Badge>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* MCP / 供应商来源 */}
      {(def.file.mcp.length > 0 || def.file.provider.length > 0) && (
        <div className="grid gap-3 sm:grid-cols-2">
          {def.file.mcp.length > 0 && (
            <Card className="border-ink-700/60">
              <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                MCP 来源
              </div>
              <div className="mt-1.5 space-y-1">
                {def.file.mcp.map((m) => (
                  <div key={m.file + m.root} className="text-[11px]">
                    <span className="mono text-slate-300">root = {m.root || "(根)"}</span>
                    <div className="mono truncate text-slate-500" title={m.file}>
                      {m.file}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}
          {def.file.provider.length > 0 && (
            <Card className="border-ink-700/60">
              <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                供应商来源
              </div>
              <div className="mt-1.5 space-y-1">
                {def.file.provider.map((p) => (
                  <div key={p.file + p.root} className="text-[11px]">
                    <span className="mono text-slate-300">
                      {p.label || "—"} · root = {p.root || "(根)"}
                    </span>
                    {p.keysOnly && (
                      <Badge tone="rose" icon="lock" className="ml-1.5">
                        只读键名
                      </Badge>
                    )}
                    <div className="mono truncate text-slate-500" title={p.file}>
                      {p.file}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>
      )}

      {/* 能力授权 */}
      <Card className="border-ink-700/60">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
          能力分级授权
        </div>
        <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11.5px] text-slate-300">
          <span>
            上限：
            <span className="text-brand-400">
              {TIER_META[def.file.capabilities.maxTier]?.code} ·{" "}
              {TIER_META[def.file.capabilities.maxTier]?.label}
            </span>
          </span>
          {def.file.capabilities.skillMethods.length > 0 && (
            <span>Skill 方式：{def.file.capabilities.skillMethods.join(" / ")}</span>
          )}
          {def.file.capabilities.configWrite.length > 0 && (
            <span>配置写入：{def.file.capabilities.configWrite.join(" / ")}</span>
          )}
        </div>
        {def.file.capabilities.notes && (
          <p className="mt-1.5 text-[11px] leading-relaxed text-slate-500">
            {def.file.capabilities.notes}
          </p>
        )}
      </Card>
    </div>
  );
}