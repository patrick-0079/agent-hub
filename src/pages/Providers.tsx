/** 模型供应商：受管资源库（含 DPAPI 保险库密钥） + 扫描到的线索（只读）。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { SyncDialog } from "../components/SyncDialog";
import {
  Badge,
  Card,
  Empty,
  Modal,
  SectionCard,
  SegmentedControl,
  StatusDot,
  Toggle,
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { KIND_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type {
  ProviderHint,
  ProviderResource,
  ProviderTestResult,
  VaultStatus,
} from "../lib/types";

const KIND_TONE: Record<string, "rose" | "teal" | "violet" | "slate"> = {
  "api-key": "rose",
  "base-url": "teal",
  model: "violet",
  credential: "slate",
};

const PROVIDER_KINDS = [
  { value: "openai-compatible", label: "OpenAI 兼容" },
  { value: "anthropic", label: "Anthropic" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "ollama", label: "Ollama（本地）" },
  { value: "azure", label: "Azure OpenAI" },
];

function emptyProvider(): ProviderResource {
  return {
    id: 0,
    name: "",
    kind: "openai-compatible",
    baseUrl: "",
    models: [],
    keyRef: "",
    hasKey: false,
    maskedKey: null,
    enabled: true,
    notes: "",
    health: {
      status: "",
      httpStatus: null,
      latencyMs: 0,
      models: null,
      message: "",
      testedAt: "",
      endpoint: "",
    },
  };
}

/** 连通性测试结果在卡片里的一行展示 */
function HealthLine({ health }: { health: ProviderResource["health"] }) {
  if (!health || !health.status) return null;
  const dot = health.status === "ok" ? "ok" : health.status === "no_key" ? "warn" : "error";
  return (
    <div
      className="mt-1 flex items-center gap-2 text-[11px] leading-relaxed"
      title={`${health.endpoint || "—"}\n${health.message}`}
    >
      <StatusDot state={dot} />
      <span
        className={
          health.status === "ok"
            ? "text-slate-400"
            : health.status === "no_key"
              ? "text-amber-300"
              : "text-rose-300"
        }
      >
        {health.status === "ok" && (
          <>
            连通 {health.latencyMs} ms
            {health.models != null && ` · ${health.models} 个模型`}
            {health.testedAt && ` · ${health.testedAt}`}
          </>
        )}
        {health.status === "no_key" && `端点可达，但尚未保存 API Key（HTTP ${health.httpStatus ?? "?"}）`}
        {health.status === "error" && health.message}
      </span>
    </div>
  );
}

/** 把扫描线索映射成受管供应商（密钥不在线索里，需要重新录入） */
function toProvider(hint: ProviderHint): ProviderResource {
  const base = emptyProvider();
  const raw = (hint.envVar || hint.label).replace(/[^\w.-]+/g, "-").toLowerCase();
  return {
    ...base,
    name: raw || "provider",
    kind: hint.label.toLowerCase().includes("anthropic") ? "anthropic" : "openai-compatible",
    baseUrl: hint.kind === "base-url" ? hint.valueMasked : "",
    notes: `从 ${hint.source} 导入（密钥需重新录入）`,
  };
}

/* ------------------------------------------------------------ 供应商编辑器 */

function ProviderForm({
  open,
  onClose,
  initial,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  initial: ProviderResource | null;
  onSaved: (list: ProviderResource[]) => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [draft, setDraft] = useState<ProviderResource>(emptyProvider());
  const [modelsText, setModelsText] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      const base = initial ?? emptyProvider();
      setDraft(base);
      setModelsText(base.models.join("\n"));
      setApiKey("");
    }
  }, [open, initial]);

  const save = async () => {
    setBusy(true);
    try {
      const payload: ProviderResource = {
        ...draft,
        models: modelsText
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
      };
      onSaved(await api.providerSave(payload, apiKey.trim() ? apiKey : null));
      onClose();
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
      title={draft.id > 0 ? `编辑供应商：${draft.name}` : "新增供应商"}
      subtitle="密钥写入系统级加密的保险库（DPAPI），数据库里只存引用"
      width="max-w-2xl"
      footer={
        <>
          <button type="button" className="btn-ghost" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={() => void save()}
            disabled={busy || !draft.name.trim()}
          >
            {busy ? "保存中…" : "保存"}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="text-xs text-slate-400">名称</label>
            <input
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              placeholder="例如 openrouter"
              className="input mt-1.5 font-mono text-xs"
            />
          </div>
          <div>
            <label className="text-xs text-slate-400">类型</label>
            <select
              value={draft.kind}
              onChange={(e) => setDraft({ ...draft, kind: e.target.value })}
              className="input mt-1.5 text-xs"
            >
              {PROVIDER_KINDS.map((k) => (
                <option key={k.value} value={k.value}>
                  {k.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div>
          <label className="text-xs text-slate-400">Base URL</label>
          <input
            value={draft.baseUrl}
            onChange={(e) => setDraft({ ...draft, baseUrl: e.target.value })}
            placeholder="https://api.example.com/v1"
            className="input mt-1.5 font-mono text-xs"
          />
        </div>

        <div>
          <label className="text-xs text-slate-400">
            API Key
            {draft.hasKey && (
              <span className="ml-2 text-brand-400">
                保险库中已存：{draft.maskedKey ?? "••••"}
              </span>
            )}
          </label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={draft.hasKey ? "留空则保持现有密钥不变" : "粘贴密钥（DPAPI 加密存储）"}
            className="input mt-1.5 font-mono text-xs"
            autoComplete="off"
          />
          <p className="mt-1 text-[10.5px] leading-relaxed text-slate-500">
            密钥不会写入数据库，只以密文存入 <span className="font-mono">vault.json</span>（与当前
            Windows 用户绑定）；分发到 Agent 时才会解密并注入配置。
          </p>
        </div>

        <div>
          <label className="text-xs text-slate-400">模型列表（一行一个，可选）</label>
          <textarea
            value={modelsText}
            onChange={(e) => setModelsText(e.target.value)}
            placeholder={"gpt-4o-mini\nclaude-sonnet-4"}
            spellCheck={false}
            className="input mt-1.5 h-20 font-mono text-xs"
          />
        </div>

        <div className="border-t border-ink-800 pt-2">
          <Toggle
            checked={draft.enabled}
            onChange={(next) => setDraft({ ...draft, enabled: next })}
            label="启用（仅启用中的供应商会被分发）"
            hint="平铺式目标（如 Claude Code 的 env 段）一次只能承载一个供应商"
          />
        </div>
        <div>
          <label className="text-xs text-slate-400">备注</label>
          <input
            value={draft.notes}
            onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
            className="input mt-1.5 text-xs"
          />
        </div>
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- 页面 */

export function ProvidersPage() {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [tab, setTab] = useState("managed");
  const [providers, setProviders] = useState<ProviderResource[]>([]);
  const [vault, setVault] = useState<VaultStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ProviderResource | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const [revealed, setRevealed] = useState<{ id: number; value: string } | null>(null);
  const [query, setQuery] = useState("");
  const [kindFilter, setKindFilter] = useState("all");
  const [testing, setTesting] = useState<Set<number>>(new Set());

  const hints = snapshot?.providerHints ?? [];

  const load = () => {
    setLoading(true);
    Promise.all([api.providerResources(), api.vaultStatus()])
      .then(([list, status]) => {
        setProviders(list);
        setVault(status);
      })
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const enabledCount = providers.filter((p) => p.enabled).length;

  /** 测试单个供应商：结果就地更新（后端同时已落库） */
  const testOne = async (item: ProviderResource) => {
    setTesting((prev) => new Set(prev).add(item.id));
    try {
      const result: ProviderTestResult = await api.providerTest(item.id);
      setProviders((prev) =>
        prev.map((p) =>
          p.id === item.id
            ? {
                ...p,
                health: {
                  status: result.status,
                  httpStatus: result.httpStatus,
                  latencyMs: result.latencyMs,
                  models: result.models,
                  message: result.message,
                  testedAt: result.testedAt,
                  endpoint: result.endpoint,
                },
              }
            : p,
        ),
      );
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setTesting((prev) => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
    }
  };

  /** 逐个测试全部启用的供应商（串行，避免同时打满代理） */
  const testAll = async () => {
    for (const item of providers.filter((p) => p.enabled)) {
      await testOne(item);
    }
  };

  const toggleEnabled = async (item: ProviderResource) => {
    try {
      setProviders(await api.providerSave({ ...item, enabled: !item.enabled }, null));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const remove = async (item: ProviderResource) => {
    try {
      setProviders(await api.providerRemove(item.id, true));
      load();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const revealKey = async (item: ProviderResource) => {
    try {
      setRevealed({ id: item.id, value: await api.providerRevealKey(item.id) });
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const doImport = async () => {
    try {
      const items = hints.filter((h) => picked.includes(h.id)).map(toProvider);
      setProviders(await api.providerImport(items));
      setImportOpen(false);
      setPicked([]);
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const counts = useMemo(() => {
    const map = new Map<string, number>();
    hints.forEach((h) => map.set(h.kind, (map.get(h.kind) ?? 0) + 1));
    return map;
  }, [hints]);

  const filteredHints = useMemo(() => {
    const q = query.trim().toLowerCase();
    return hints.filter((h) => {
      if (kindFilter !== "all" && h.kind !== kindFilter) return false;
      if (!q) return true;
      return (
        h.label.toLowerCase().includes(q) ||
        h.source.toLowerCase().includes(q) ||
        (h.envVar ?? "").toLowerCase().includes(q)
      );
    });
  }, [hints, query, kindFilter]);

  return (
    <div className="space-y-4">
      {/* 保险库状态 */}
      {vault && (
        <Card className={vault.healthy ? "border-brand-500/30" : "border-rose-500/40"}>
          <div className="flex flex-wrap items-center gap-3">
            <span
              className={`rounded-lg border p-2 ${
                vault.healthy
                  ? "border-brand-500/40 bg-brand-900 text-brand-400"
                  : "border-rose-500/40 bg-rose-950 text-rose-300"
              }`}
            >
              <Icon name="vault" className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium text-slate-200">
                密钥保险库 · {vault.count} 个密钥
                {vault.healthy ? " · 状态正常" : " · 存在无法解密的条目"}
              </div>
              <div className="mt-0.5 text-[11px] leading-relaxed text-slate-500">
                {vault.message}
              </div>
            </div>
            <button type="button" className="btn-ghost btn-sm shrink-0" onClick={() => reveal(vault.path)}>
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开保险库目录
            </button>
          </div>
        </Card>
      )}

      <SectionCard
        title={tab === "managed" ? "供应商资源库（单一事实源）" : "扫描到的供应商线索"}
        subtitle={
          tab === "managed"
            ? `共 ${providers.length} 个（启用 ${enabledCount}）—— 密钥存保险库，分发时按各 Agent 定义注入`
            : `从环境变量与各 Agent 配置中汇总到 ${hints.length} 条线索（只读，密钥仅掩码）`
        }
        action={
          <div className="flex flex-wrap items-center gap-2">
            <SegmentedControl
              value={tab}
              onChange={setTab}
              options={[
                { value: "managed", label: "受管供应商", count: providers.length },
                { value: "discovered", label: "扫描线索", count: hints.length },
              ]}
            />
            {tab === "managed" ? (
              <>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => void testAll()}
                  disabled={enabledCount === 0 || testing.size > 0}
                  title="对全部启用的供应商各发起一次最小只读请求（GET models）"
                >
                  <Icon
                    name="play"
                    className={`h-3.5 w-3.5 ${testing.size > 0 ? "animate-pulse-soft" : ""}`}
                  />
                  {testing.size > 0 ? `测试中（${testing.size}）` : "测试连接"}
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => setImportOpen(true)}
                  disabled={hints.length === 0}
                >
                  <Icon name="plus" className="h-3.5 w-3.5" />
                  从线索导入
                </button>
                <button
                  type="button"
                  className="btn-primary btn-sm"
                  onClick={() => {
                    setEditing(null);
                    setFormOpen(true);
                  }}
                >
                  <Icon name="plus" className="h-3.5 w-3.5" />
                  新增供应商
                </button>
                <button
                  type="button"
                  className="btn border border-accent-500/40 btn-sm text-accent-400 hover:bg-accent-900"
                  onClick={() => setSyncOpen(true)}
                  disabled={enabledCount === 0}
                >
                  <Icon name="link" className="h-3.5 w-3.5" />
                  分发到 Agent
                </button>
              </>
            ) : (
              <SegmentedControl
                value={kindFilter}
                onChange={setKindFilter}
                options={[
                  { value: "all", label: "全部", count: hints.length },
                  ...Array.from(counts.entries()).map(([k, count]) => ({
                    value: k,
                    label: KIND_LABEL[k] ?? k,
                    count,
                  })),
                ]}
              />
            )}
          </div>
        }
        bodyClassName="space-y-3"
      >
        {tab === "managed" ? (
          providers.length === 0 ? (
            <Empty
              icon="providers"
              title={loading ? "正在读取资源库…" : "资源库是空的"}
              description="「新增供应商」手工录入，或「从线索导入」把扫描到的 Base URL 收编为受管资源（密钥需要重新录入 —— AgentHub 从不读取明文密钥）。"
              action={
                <div className="mt-2 flex gap-2">
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={() => setImportOpen(true)}
                    disabled={hints.length === 0}
                  >
                    从线索导入
                  </button>
                  <button
                    type="button"
                    className="btn-primary"
                    onClick={() => {
                      setEditing(null);
                      setFormOpen(true);
                    }}
                  >
                    新增供应商
                  </button>
                </div>
              }
            />
          ) : (
            <div className="space-y-2">
              {providers.map((item) => (
                <div
                  key={item.id}
                  className={`rounded-lg border px-3.5 py-3 ${
                    item.enabled
                      ? "border-ink-800/70 bg-ink-900"
                      : "border-ink-800/50 bg-ink-950 opacity-70"
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-2.5">
                    <span className="rounded-md border border-brand-500/30 bg-brand-900 p-1.5 text-brand-400">
                      <Icon name="providers" className="h-3.5 w-3.5" />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="truncate text-sm font-medium text-slate-100">
                          {item.name}
                        </span>
                        <Badge tone="slate">{item.kind}</Badge>
                        {!item.enabled && <Badge tone="slate">已停用</Badge>}
                        {item.hasKey ? (
                          <Badge tone="teal" icon="lock">
                            密钥已存
                          </Badge>
                        ) : (
                          <Badge tone="amber" icon="alert">
                            无密钥
                          </Badge>
                        )}
                        {item.models.length > 0 && (
                          <Badge tone="violet">{item.models.length} 个模型</Badge>
                        )}
                      </span>
                      <span className="mono mt-0.5 block truncate">
                        {item.baseUrl || "（未设置 Base URL）"}
                      </span>
                      {item.hasKey && (
                        <span className="mono mt-0.5 block truncate text-slate-500">
                          {revealed?.id === item.id ? (
                            <span className="text-amber-300">{revealed.value}</span>
                          ) : (
                            item.maskedKey
                          )}
                        </span>
                      )}
                      <HealthLine health={item.health} />
                    </span>
                    <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => void testOne(item)}
                        disabled={testing.has(item.id)}
                        title="对 Base URL 发起一次最小只读请求（GET models）"
                      >
                        <Icon
                          name="play"
                          className={`h-3.5 w-3.5 ${testing.has(item.id) ? "animate-pulse-soft" : ""}`}
                        />
                        {testing.has(item.id) ? "测试中" : "测试"}
                      </button>
                      {item.hasKey && (
                        <button
                          type="button"
                          className="btn-ghost btn-sm"
                          onClick={() => void revealKey(item)}
                        >
                          {revealed?.id === item.id ? "已显示" : "显示密钥"}
                        </button>
                      )}
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => void toggleEnabled(item)}
                      >
                        {item.enabled ? "停用" : "启用"}
                      </button>
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => {
                          setEditing(item);
                          setFormOpen(true);
                        }}
                      >
                        <Icon name="settings" className="h-3.5 w-3.5" />
                        编辑
                      </button>
                      <button
                        type="button"
                        className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                        onClick={() => void remove(item)}
                      >
                        <Icon name="close" className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                  {item.notes && (
                    <div className="mt-1 pl-9 text-[10.5px] text-slate-500">{item.notes}</div>
                  )}
                </div>
              ))}
            </div>
          )
        ) : filteredHints.length === 0 ? (
          <Empty
            icon="providers"
            title={hints.length === 0 ? "未发现供应商线索" : "没有匹配的线索"}
            description={
              hints.length === 0
                ? "AgentHub 会检查进程环境变量（OPENAI_API_KEY、ANTHROPIC_BASE_URL 等）以及各 Agent 定义里声明的 [[provider]] 来源。"
                : "调整筛选条件试试。"
            }
          />
        ) : (
          <div className="overflow-hidden rounded-lg border border-ink-800/70">
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  <th className="table-head px-3 py-2">标签</th>
                  <th className="table-head w-24 px-3 py-2">类型</th>
                  <th className="table-head px-3 py-2">值（脱敏）</th>
                  <th className="table-head w-56 px-3 py-2">来源</th>
                </tr>
              </thead>
              <tbody>
                {filteredHints.map((hint) => {
                  const masked = hint.kind === "api-key" || hint.kind === "credential";
                  return (
                    <tr key={hint.id} className="hover:bg-ink-800">
                      <td className="table-cell px-3">
                        <div className="flex items-center gap-2">
                          {masked ? (
                            <Icon name="lock" className="h-3.5 w-3.5 shrink-0 text-rose-300" />
                          ) : (
                            <Icon name="providers" className="h-3.5 w-3.5 shrink-0 text-brand-400" />
                          )}
                          <span className="truncate text-slate-200">{hint.label}</span>
                        </div>
                      </td>
                      <td className="table-cell px-3">
                        <Badge tone={KIND_TONE[hint.kind] ?? "slate"}>
                          {KIND_LABEL[hint.kind] ?? hint.kind}
                        </Badge>
                      </td>
                      <td className="table-cell px-3">
                        <span
                          className={`font-mono text-[11px] ${
                            masked ? "text-slate-500" : "text-slate-300"
                          }`}
                        >
                          {hint.valueMasked}
                        </span>
                      </td>
                      <td className="table-cell px-3">
                        <span className="mono truncate" title={hint.source}>
                          {hint.source}
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </SectionCard>

      {tab === "managed" && (
        <div className="grid gap-3 md:grid-cols-3">
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="vault" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">密钥系统级加密</span>
              <Badge tone="violet" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              Windows DPAPI：密文与当前用户绑定，换用户或换机器都无法解密；数据库只存引用名。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="link" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">分发到 Agent</span>
              <Badge tone="violet" className="ml-auto">
                T2
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              按定义里声明的 <span className="font-mono">[[providerWrite]]</span> 注入字段；diff
              与变更详情全程掩码，写前自动备份。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="search" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">连通性测试</span>
              <Badge tone="violet" className="ml-auto">
                M1
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              发一个最小请求验证 Key 与 Base URL，可视化展示延迟、可用模型与错误原因。
            </p>
          </Card>
        </div>
      )}

      <ProviderForm
        open={formOpen}
        onClose={() => setFormOpen(false)}
        initial={editing}
        onSaved={(list) => {
          setProviders(list);
          load();
        }}
      />

      <Modal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title="从扫描线索导入供应商"
        subtitle="线索里只有掩码值，密钥需要导入后重新录入"
        footer={
          <>
            <button type="button" className="btn-ghost" onClick={() => setImportOpen(false)}>
              取消
            </button>
            <button
              type="button"
              className="btn-primary"
              onClick={() => void doImport()}
              disabled={picked.length === 0}
            >
              导入选中的 {picked.length} 条
            </button>
          </>
        }
      >
        <div className="space-y-1.5">
          <label className="flex cursor-pointer items-center gap-2.5 px-1 text-xs text-slate-400">
            <input
              type="checkbox"
              checked={picked.length === hints.length && hints.length > 0}
              onChange={(e) => setPicked(e.target.checked ? hints.map((h) => h.id) : [])}
              className="accent-teal-400"
            />
            全选
          </label>
          {hints.map((hint) => (
            <label
              key={hint.id}
              className="flex cursor-pointer items-start gap-2.5 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2 hover:bg-ink-800"
            >
              <input
                type="checkbox"
                checked={picked.includes(hint.id)}
                onChange={(e) =>
                  setPicked((prev) =>
                    e.target.checked ? [...prev, hint.id] : prev.filter((id) => id !== hint.id),
                  )
                }
                className="mt-1 accent-teal-400"
              />
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-2">
                  <span className="truncate text-[12.5px] text-slate-200">{hint.label}</span>
                  <Badge tone={KIND_TONE[hint.kind] ?? "slate"}>
                    {KIND_LABEL[hint.kind] ?? hint.kind}
                  </Badge>
                  <span className="text-[10.5px] text-slate-500">{hint.source}</span>
                </span>
                <span className="mono mt-0.5 block truncate text-[10.5px] text-slate-500">
                  {hint.valueMasked}
                </span>
              </span>
            </label>
          ))}
        </div>
      </Modal>

      <SyncDialog
        open={syncOpen}
        onClose={() => setSyncOpen(false)}
        kind="provider"
        resourceItems={providers.map((p) => ({
          name: p.name,
          tag: p.kind,
          enabled: p.enabled,
        }))}
        onDone={load}
        onReveal={reveal}
      />
    </div>
  );
}