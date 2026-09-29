/** MCP：受管资源库（可编辑、可分发） + 扫描到的现有配置（只读）。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { SyncDialog } from "../components/SyncDialog";
import {
  Badge,
  Card,
  Empty,
  Modal,
  SearchInput,
  SectionCard,
  SegmentedControl,
  StatusDot,
  Toggle,
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { shortenPath } from "../lib/format";
import { TRANSPORT_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { EnvPair, McpResource } from "../lib/types";

const TRANSPORT_TONE: Record<string, "teal" | "sky" | "violet" | "slate"> = {
  stdio: "teal",
  http: "sky",
  sse: "violet",
  unknown: "slate",
};

/* ------------------------------------------- 内置 MCP 模板（一键添加） */

interface McpTemplate {
  id: string;
  name: string;
  description: string;
  command: string;
  args: string[];
  runtime: "node" | "uvx";
  docs: string;
}

const MCP_TEMPLATES: McpTemplate[] = [
  {
    id: "filesystem",
    name: "filesystem",
    description: "为本机目录提供读写访问（添加后把参数里的目录改成你要放行的路径）",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "C:\\Users\\你的用户名"],
    runtime: "node",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
  },
  {
    id: "fetch",
    name: "fetch",
    description: "抓取网页并转成 Markdown（搜索、爬文档、读在线资料）",
    command: "uvx",
    args: ["mcp-server-fetch"],
    runtime: "uvx",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
  },
  {
    id: "playwright",
    name: "playwright",
    description: "用 Playwright 驱动真实浏览器：导航、点击、截图、自动化测试",
    command: "npx",
    args: ["-y", "@playwright/mcp"],
    runtime: "node",
    docs: "https://github.com/executeautomation/mcp-playwright-server",
  },
  {
    id: "context7",
    name: "context7",
    description: "为代码问题提供最新版本的库文档（对抗模型知识过时）",
    command: "npx",
    args: ["-y", "@upstash/context7-mcp"],
    runtime: "node",
    docs: "https://github.com/upstash/context7",
  },
  {
    id: "sequential-thinking",
    name: "sequential-thinking",
    description: "让 Agent 分步骤记录与修正推理过程（复杂任务的思维链）",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    runtime: "node",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
  },
  {
    id: "memory",
    name: "memory",
    description: "基于知识图谱的跨会话记忆（人物/事件/偏好）",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
    runtime: "node",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
  },
  {
    id: "time",
    name: "time",
    description: "获取当前时间与时区转换（LLM 自身没有可靠的时钟）",
    command: "uvx",
    args: ["mcp-server-time"],
    runtime: "uvx",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/time",
  },
  {
    id: "everything",
    name: "everything",
    description: "MCP 官方测试服务器：覆盖全部协议特性，用来验证客户端兼容性",
    command: "uvx",
    args: ["mcp-server-everything"],
    runtime: "uvx",
    docs: "https://github.com/modelcontextprotocol/servers/tree/main/src/everything",
  },
];

/** 模板 → 受管资源草稿 */
function fromTemplate(tpl: McpTemplate): McpResource {
  return {
    ...emptyResource(),
    name: tpl.name,
    transport: "stdio",
    command: tpl.command,
    args: tpl.args,
    notes: `来自内置模板（${tpl.docs}）`,
  };
}

function emptyResource(): McpResource {
  return {
    id: 0,
    name: "",
    transport: "stdio",
    command: "",
    args: [],
    env: [],
    headers: [],
    url: "",
    enabled: true,
    notes: "",
    health: {
      status: "",
      latencyMs: 0,
      protocolVersion: null,
      serverName: null,
      tools: null,
      message: "",
      testedAt: "",
    },
  };
}

/** 把扫描到的条目映射成受管资源 */
function toResource(found: {
  name: string;
  transport: string;
  command: string | null;
  args: string[];
  url: string | null;
  envKeys: string[];
  headerKeys: string[];
  sourceAgent: string;
}): McpResource {
  return {
    id: 0,
    name: found.name,
    transport: found.transport === "unknown" ? "stdio" : found.transport,
    command: found.command ?? "",
    args: found.args,
    env: found.envKeys.map((key) => ({ key, value: "" })),
    headers: found.headerKeys.map((key) => ({ key, value: "" })),
    url: found.url ?? "",
    enabled: true,
    notes: `从 ${found.sourceAgent} 导入`,
    health: emptyResource().health,
  };
}

/* ------------------------------------------------------------ 资源编辑器 */

function ResourceForm({
  open,
  onClose,
  initial,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  initial: McpResource | null;
  onSaved: (list: McpResource[]) => void;
}) {
  const setBanner = useApp((s) => s.setBanner);
  const [draft, setDraft] = useState<McpResource>(emptyResource());
  const [argsText, setArgsText] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      const base = initial ?? emptyResource();
      setDraft(base);
      setArgsText(base.args.join("\n"));
    }
  }, [open, initial]);

  const save = async () => {
    setBusy(true);
    try {
      const payload: McpResource = {
        ...draft,
        args: argsText
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
        env: draft.env.filter((pair) => pair.key.trim()),
        headers: draft.headers.filter((pair) => pair.key.trim()),
      };
      onSaved(await api.mcpSave(payload));
      onClose();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const updateEnv = (index: number, patch: Partial<EnvPair>) =>
    setDraft((prev) => ({
      ...prev,
      env: prev.env.map((pair, i) => (i === index ? { ...pair, ...patch } : pair)),
    }));

  const updateHeader = (index: number, patch: Partial<EnvPair>) =>
    setDraft((prev) => ({
      ...prev,
      headers: prev.headers.map((pair, i) => (i === index ? { ...pair, ...patch } : pair)),
    }));

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={draft.id > 0 ? `编辑 MCP：${draft.name}` : "新增 MCP 资源"}
      subtitle="资源的定义是单一事实源，分发时由各 Agent 的定义决定写入位置与形状"
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
            <label className="text-xs text-slate-400">名称（写入各 Agent 配置时的键名）</label>
            <input
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              placeholder="例如 context7"
              className="input mt-1.5 font-mono text-xs"
            />
          </div>
          <div>
            <label className="text-xs text-slate-400">传输方式</label>
            <div className="mt-1.5">
              <SegmentedControl
                value={draft.transport}
                onChange={(next) => setDraft({ ...draft, transport: next })}
                options={[
                  { value: "stdio", label: "stdio" },
                  { value: "http", label: "http" },
                  { value: "sse", label: "sse" },
                ]}
              />
            </div>
          </div>
        </div>

        {draft.transport === "stdio" ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-xs text-slate-400">启动命令</label>
              <input
                value={draft.command}
                onChange={(e) => setDraft({ ...draft, command: e.target.value })}
                placeholder="npx"
                className="input mt-1.5 font-mono text-xs"
              />
            </div>
            <div>
              <label className="text-xs text-slate-400">参数（一行一个）</label>
              <textarea
                value={argsText}
                onChange={(e) => setArgsText(e.target.value)}
                placeholder={"-y\n@upstash/context7-mcp"}
                spellCheck={false}
                className="input mt-1.5 h-24 font-mono text-xs"
              />
            </div>
          </div>
        ) : (
          <div>
            <label className="text-xs text-slate-400">URL</label>
            <input
              value={draft.url}
              onChange={(e) => setDraft({ ...draft, url: e.target.value })}
              placeholder="https://example.com/mcp"
              className="input mt-1.5 font-mono text-xs"
            />
          </div>
        )}

        <div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-slate-400">环境变量</label>
            <span className="text-[10.5px] text-slate-500">
              值可写成 <span className="font-mono">%VAR%</span> 或{" "}
              <span className="font-mono">$VAR</span> 引用已有环境变量，避免把密钥写进配置文件
            </span>
            <button
              type="button"
              className="btn-ghost btn-sm ml-auto"
              onClick={() => setDraft({ ...draft, env: [...draft.env, { key: "", value: "" }] })}
            >
              <Icon name="plus" className="h-3.5 w-3.5" />
              添加
            </button>
          </div>
          <div className="mt-2 space-y-1.5">
            {draft.env.map((pair, index) => (
              <div key={index} className="flex items-center gap-2">
                <input
                  value={pair.key}
                  onChange={(e) => updateEnv(index, { key: e.target.value })}
                  placeholder="KEY"
                  className="input font-mono text-xs"
                />
                <input
                  value={pair.value}
                  onChange={(e) => updateEnv(index, { value: e.target.value })}
                  placeholder="value 或 %ENV_VAR%"
                  className="input font-mono text-xs"
                />
                <button
                  type="button"
                  className="rounded-md p-1.5 text-slate-500 hover:bg-ink-800 hover:text-rose-300"
                  onClick={() =>
                    setDraft({ ...draft, env: draft.env.filter((_, i) => i !== index) })
                  }
                >
                  <Icon name="close" className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
            {draft.env.length === 0 && (
              <p className="text-xs text-slate-600">未设置环境变量</p>
            )}
          </div>
        </div>

        {draft.transport !== "stdio" && (
          <div>
            <div className="flex items-center gap-2">
              <label className="text-xs text-slate-400">请求头（http/sse）</label>
              <span className="text-[10.5px] text-slate-500">
                认证等自定义头，值可写 <span className="font-mono">%VAR%</span> 引用环境变量；握手测试会带上它们
              </span>
              <button
                type="button"
                className="btn-ghost btn-sm ml-auto"
                onClick={() =>
                  setDraft({ ...draft, headers: [...draft.headers, { key: "", value: "" }] })
                }
              >
                <Icon name="plus" className="h-3.5 w-3.5" />
                添加
              </button>
            </div>
            <div className="mt-2 space-y-1.5">
              {draft.headers.map((pair, index) => (
                <div key={index} className="flex items-center gap-2">
                  <input
                    value={pair.key}
                    onChange={(e) => updateHeader(index, { key: e.target.value })}
                    placeholder="Header-Name"
                    className="input font-mono text-xs"
                  />
                  <input
                    value={pair.value}
                    onChange={(e) => updateHeader(index, { value: e.target.value })}
                    placeholder="value 或 %ENV_VAR%"
                    className="input font-mono text-xs"
                  />
                  <button
                    type="button"
                    className="rounded-md p-1.5 text-slate-500 hover:bg-ink-800 hover:text-rose-300"
                    onClick={() =>
                      setDraft({ ...draft, headers: draft.headers.filter((_, i) => i !== index) })
                    }
                  >
                    <Icon name="close" className="h-3.5 w-3.5" />
                  </button>
                </div>
              ))}
              {draft.headers.length === 0 && (
                <p className="text-xs text-slate-600">
                  未设置请求头（需要认证的远端 MCP 在这里加 Authorization 等）
                </p>
              )}
            </div>
          </div>
        )}

        <div className="border-t border-ink-800 pt-2">
          <Toggle
            checked={draft.enabled}
            onChange={(next) => setDraft({ ...draft, enabled: next })}
            label="启用（仅启用中的资源会被分发）"
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

export function McpPage() {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [tab, setTab] = useState("managed");
  const [resources, setResources] = useState<McpResource[]>([]);
  const [loading, setLoading] = useState(true);
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<McpResource | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [templateOpen, setTemplateOpen] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const [importError, setImportError] = useState<string | null>(null);
  const [importSummary, setImportSummary] = useState<string | null>(null);

  const [query, setQuery] = useState("");
  const [source, setSource] = useState("all");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [testing, setTesting] = useState<Set<number>>(new Set());

  const load = () => {
    setLoading(true);
    api
      .mcpResources()
      .then(setResources)
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const servers = snapshot?.mcpServers ?? [];
  const enabledCount = resources.filter((r) => r.enabled).length;

  /** 对单个 MCP 服务器做一次真实握手（stdio 起进程 / http 发 initialize） */
  const testOne = async (res: McpResource) => {
    setTesting((prev) => new Set(prev).add(res.id));
    try {
      const result = await api.mcpTest(res.id);
      setResources((prev) =>
        prev.map((r) =>
          r.id === res.id
            ? {
                ...r,
                health: {
                  status: result.status,
                  latencyMs: result.latencyMs,
                  protocolVersion: result.protocolVersion,
                  serverName: result.serverName,
                  tools: result.tools,
                  message: result.message,
                  testedAt: result.testedAt,
                },
              }
            : r,
        ),
      );
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setTesting((prev) => {
        const next = new Set(prev);
        next.delete(res.id);
        return next;
      });
    }
  };

  /** 批量握手（串行：stdio 型会真实起进程，逐个来） */
  const testAll = async () => {
    for (const res of resources.filter((r) => r.enabled)) {
      await testOne(res);
    }
  };

  const toggleEnabled = async (res: McpResource) => {
    try {
      setResources(await api.mcpSave({ ...res, enabled: !res.enabled }));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const remove = async (res: McpResource) => {
    try {
      setResources(await api.mcpRemove(res.id));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const doImport = async () => {
    setImportError(null);
    try {
      const items = servers
        .filter((s) => picked.includes(s.id))
        .map((s) => toResource(s));
      if (items.length === 0) {
        setImportError("没有选中任何条目");
        return;
      }
      const before = resources.length;
      const list = await api.mcpImport(items);
      setResources(list);
      setPicked([]);
      setImportSummary(
        `已导入 ${items.length} 个条目，资源库从 ${before} 个增加到 ${list.length} 个`,
      );
    } catch (error) {
      // 关键：把失败原因显示在弹窗内，而不是只丢到页面顶部横幅
      setImportError(describeError(error));
    }
  };

  const sources = useMemo(() => {
    const map = new Map<string, number>();
    servers.forEach((s) => map.set(s.sourceAgentId, (map.get(s.sourceAgentId) ?? 0) + 1));
    return Array.from(map.entries());
  }, [servers]);

  const filteredServers = useMemo(() => {
    const q = query.trim().toLowerCase();
    return servers.filter((s) => {
      if (source !== "all" && s.sourceAgentId !== source) return false;
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        (s.command ?? "").toLowerCase().includes(q) ||
        (s.url ?? "").toLowerCase().includes(q)
      );
    });
  }, [servers, query, source]);

  return (
    <div className="space-y-4">
      <SectionCard
        title={tab === "managed" ? "MCP 资源库（单一事实源）" : "扫描到的 MCP 配置"}
        subtitle={
          tab === "managed"
            ? `共 ${resources.length} 个资源（启用 ${enabledCount}）—— 分发时按各 Agent 定义生成对应格式`
            : `从各 Agent 配置文件中解析到 ${servers.length} 个条目（只读）`
        }
        action={
          <div className="flex flex-wrap items-center gap-2">
            <SegmentedControl
              value={tab}
              onChange={setTab}
              options={[
                { value: "managed", label: "受管资源", count: resources.length },
                { value: "discovered", label: "扫描结果", count: servers.length },
              ]}
            />
            {tab === "managed" ? (
              <>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => void testAll()}
                  disabled={enabledCount === 0 || testing.size > 0}
                  title="逐个真实启动（stdio）或发 HTTP initialize，验证每个服务器能否完成握手"
                >
                  <Icon
                    name="play"
                    className={`h-3.5 w-3.5 ${testing.size > 0 ? "animate-pulse-soft" : ""}`}
                  />
                  {testing.size > 0 ? `握手中（${testing.size}）` : "握手测试"}
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => { setImportError(null); setImportSummary(null); setImportOpen(true); }}
                  disabled={servers.length === 0}
                >
                  <Icon name="plus" className="h-3.5 w-3.5" />
                  从扫描导入
                </button>
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={() => setTemplateOpen(true)}
                  title="从内置模板一键填充常用 MCP 服务器（filesystem / fetch / playwright / context7…）"
                >
                  <Icon name="sparkle" className="h-3.5 w-3.5" />
                  从模板添加
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
                  新增资源
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
              <div className="w-56">
                <SearchInput value={query} onChange={setQuery} placeholder="搜索名称 / 命令 / URL…" />
              </div>
            )}
          </div>
        }
        bodyClassName="space-y-3"
      >
        {tab === "managed" ? (
          resources.length === 0 ? (
            <Empty
              icon="mcp"
              title={loading ? "正在读取资源库…" : "资源库是空的"}
              description="可以「新增资源」手工定义，或「从扫描导入」把各 Agent 里已有的 MCP 条目收编为受管资源；之后就能一键分发到多个 Agent。"
              action={
                <div className="mt-2 flex gap-2">
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={() => { setImportError(null); setImportSummary(null); setImportOpen(true); }}
                    disabled={servers.length === 0}
                  >
                    从扫描导入
                  </button>
                  <button
                    type="button"
                    className="btn-primary"
                    onClick={() => {
                      setEditing(null);
                      setFormOpen(true);
                    }}
                  >
                    新增资源
                  </button>
                </div>
              }
            />
          ) : (
            <div className="space-y-2">
              {resources.map((res) => (
                <div
                  key={res.id}
                  className={`rounded-lg border px-3.5 py-3 transition-colors ${
                    res.enabled
                      ? "border-ink-800/70 bg-ink-900"
                      : "border-ink-800/50 bg-ink-950 opacity-70"
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-2.5">
                    <span className="rounded-md border border-sky-500/30 bg-sky-950 p-1.5 text-sky-300">
                      <Icon name="mcp" className="h-3.5 w-3.5" />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="truncate text-sm font-medium text-slate-100">
                          {res.name}
                        </span>
                        <Badge tone={TRANSPORT_TONE[res.transport] ?? "slate"}>
                          {TRANSPORT_LABEL[res.transport] ?? res.transport}
                        </Badge>
                        {!res.enabled && <Badge tone="slate">已停用</Badge>}
                        {res.env.length > 0 && (
                          <Badge tone="amber" icon="lock">
                            {res.env.length} 个环境变量
                          </Badge>
                        )}
                        {res.headers.length > 0 && (
                          <Badge tone="sky" icon="shield">
                            {res.headers.length} 个请求头
                          </Badge>
                        )}
                      </span>
                      <span
                        className="mono mt-0.5 block truncate"
                        title={res.transport === "stdio" ? `${res.command} ${res.args.join(" ")}` : res.url}
                      >
                        {res.transport === "stdio"
                          ? `${res.command} ${res.args.join(" ")}`.trim()
                          : res.url}
                      </span>
                      {res.health && res.health.status && (
                        <span
                          className={`mt-0.5 flex items-center gap-2 text-[11px] leading-relaxed ${
                            res.health.status === "ok"
                              ? "text-slate-400"
                              : res.health.status === "timeout"
                                ? "text-amber-300"
                                : "text-rose-300"
                          }`}
                          title={res.health.message}
                        >
                          <StatusDot
                            state={
                              res.health.status === "ok"
                                ? "ok"
                                : res.health.status === "timeout"
                                  ? "warn"
                                  : "error"
                            }
                          />
                          {res.health.status === "ok"
                            ? `握手 ${res.health.latencyMs} ms${
                                res.health.tools != null ? ` · ${res.health.tools} 个工具` : ""
                              }${res.health.testedAt ? ` · ${res.health.testedAt}` : ""}`
                            : res.health.message}
                        </span>
                      )}
                    </span>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => void testOne(res)}
                        disabled={testing.has(res.id)}
                        title="真实握手一次：stdio 启动进程 / http 发 initialize，进程结束即恢复"
                      >
                        <Icon
                          name="play"
                          className={`h-3.5 w-3.5 ${testing.has(res.id) ? "animate-pulse-soft" : ""}`}
                        />
                        {testing.has(res.id) ? "握手中" : "握手"}
                      </button>
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => void toggleEnabled(res)}
                      >
                        {res.enabled ? "停用" : "启用"}
                      </button>
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => {
                          setEditing(res);
                          setFormOpen(true);
                        }}
                      >
                        <Icon name="settings" className="h-3.5 w-3.5" />
                        编辑
                      </button>
                      <button
                        type="button"
                        className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                        onClick={() => void remove(res)}
                      >
                        <Icon name="close" className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                  {res.notes && (
                    <div className="mt-1 pl-9 text-[10.5px] text-slate-500">{res.notes}</div>
                  )}
                </div>
              ))}
            </div>
          )
        ) : (
          <>
            {sources.length > 1 && (
              <SegmentedControl
                value={source}
                onChange={setSource}
                options={[
                  { value: "all", label: "全部", count: servers.length },
                  ...sources.map(([id, count]) => ({
                    value: id,
                    label: snapshot?.agents.find((a) => a.id === id)?.name ?? id,
                    count,
                  })),
                ]}
              />
            )}
            {filteredServers.length === 0 ? (
              <Empty
                icon="mcp"
                title={servers.length === 0 ? "未发现 MCP 配置" : "没有匹配的条目"}
                description={
                  servers.length === 0
                    ? "AgentHub 会按各 Agent 定义里声明的 [[mcp]] 位置解析配置节点。当前机器上这些文件里还没有条目。"
                    : "换个关键词或切换来源筛选试试。"
                }
              />
            ) : (
              <div className="space-y-2">
                {filteredServers.map((server) => {
                  const isOpen = expanded === server.id;
                  return (
                    <div key={server.id} className="rounded-lg border border-ink-800/70 bg-ink-900">
                      <div className="flex items-center gap-3 px-3.5 py-3">
                        <span className="rounded-md border border-sky-500/30 bg-sky-950 p-1.5 text-sky-300">
                          <Icon name="mcp" className="h-3.5 w-3.5" />
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="truncate text-sm font-medium text-slate-100">
                              {server.name}
                            </span>
                            <Badge tone={TRANSPORT_TONE[server.transport] ?? "slate"}>
                              {TRANSPORT_LABEL[server.transport] ?? server.transport}
                            </Badge>
                            {server.envKeys.length > 0 && (
                              <Badge tone="amber" icon="lock">
                                {server.envKeys.length} 个环境变量
                              </Badge>
                            )}
                            {server.headerKeys.length > 0 && (
                              <Badge tone="sky" icon="shield">
                                {server.headerKeys.length} 个请求头
                              </Badge>
                            )}
                          </div>
                          <div className="mono mt-0.5 truncate" title={server.command ?? server.url ?? ""}>
                            {server.command
                              ? `${server.command} ${server.args.join(" ")}`.trim()
                              : (server.url ?? "（无启动信息）")}
                          </div>
                        </div>
                        <div className="hidden shrink-0 text-right lg:block">
                          <div className="text-[11px] text-slate-400">{server.sourceAgent}</div>
                          <button
                            type="button"
                            onClick={() => reveal(server.sourceFile)}
                            className="mono max-w-[240px] truncate text-[10.5px] text-slate-500 hover:text-brand-400"
                            title={server.sourceFile}
                          >
                            {shortenPath(server.sourceFile, 38)}
                          </button>
                        </div>
                        <button
                          type="button"
                          onClick={() => setExpanded(isOpen ? null : server.id)}
                          className="shrink-0 rounded-md border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                        >
                          <Icon
                            name="chevronDown"
                            className={`h-3.5 w-3.5 transition-transform ${isOpen ? "rotate-180" : ""}`}
                          />
                        </button>
                      </div>
                      {isOpen && (
                        <div className="space-y-2 border-t border-ink-800 px-3.5 py-3">
                          <div className="flex flex-wrap items-center gap-2">
                            <Badge tone="slate" icon="info">
                              握手测试：导入为受管资源后可测
                            </Badge>
                            <span className="mono text-[10.5px] text-slate-500">
                              {server.sourceFile}
                            </span>
                          </div>
                          {server.envKeys.length > 0 && (
                            <div className="flex flex-wrap gap-1.5">
                              {server.envKeys.map((key) => (
                                <span
                                  key={key}
                                  className="rounded border border-ink-700 bg-ink-950 px-1.5 py-0.5 font-mono text-[10.5px] text-slate-400"
                                >
                                  {key}
                                </span>
                              ))}
                            </div>
                          )}
                          <pre className="max-h-64 overflow-auto rounded-lg border border-ink-800 bg-ink-950 p-3 text-[11px] leading-relaxed text-slate-300">
                            {JSON.stringify(server.raw, null, 2)}
                          </pre>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}
      </SectionCard>

      {tab === "managed" && (
        <div className="grid gap-3 md:grid-cols-3">
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="link" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">写入策略由定义决定</span>
              <Badge tone="violet" className="ml-auto">
                T2
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              JSON 走「结构合并」（只增删受管的键，用户手写内容不动）；TOML 走「托管块」（标记之间就地替换）。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="shield" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">写前备份、随时回滚</span>
              <Badge tone="violet" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              每次写入前把目标文件备份到数据目录，可在「历史与审计 → 备份」里一键回滚。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="play" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">真实握手健康检查</span>
              <Badge tone="teal" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              「握手」按钮真实启动进程（stdio）或发 initialize（http），完成 JSON-RPC
              握手并清点工具数；进程结束即恢复原状，不写任何配置。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="sparkle" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">MCP 应用商店（内置模板）</span>
              <Badge tone="teal" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              「从模板添加」一键填充常用 MCP 的启动命令（filesystem、fetch、playwright、context7、
              memory、time…），保存后就是普通受管资源，可随意改参数。
            </p>
          </Card>
        </div>
      )}

      {/* 内置模板库（M1「MCP 应用商店」的最小形态） */}
      <Modal
        open={templateOpen}
        onClose={() => setTemplateOpen(false)}
        title="从模板添加 MCP 服务器"
        subtitle="常用 MCP 的启动命令已按官方文档填好；点「使用」后可在表单里微调参数（如 filesystem 的目录）"
        width="max-w-3xl"
      >
        <div className="grid gap-2.5 sm:grid-cols-2">
          {MCP_TEMPLATES.map((tpl) => {
            const exists = resources.some((r) => r.name.toLowerCase() === tpl.name);
            return (
              <div
                key={tpl.id}
                className="flex flex-col rounded-md border border-ink-800 bg-ink-900 px-3 py-2.5"
              >
                <div className="flex items-center gap-2">
                  <Icon name="mcp" className="h-4 w-4 shrink-0 text-slate-400" />
                  <span className="truncate font-mono text-sm text-slate-100">{tpl.name}</span>
                  <Badge tone={tpl.runtime === "uvx" ? "violet" : "teal"} className="ml-auto">
                    {tpl.runtime === "uvx" ? "uvx" : "npx"}
                  </Badge>
                  {exists && <Badge tone="slate">已存在同名</Badge>}
                </div>
                <p className="mt-1 text-xs leading-relaxed text-slate-500">{tpl.description}</p>
                <div className="mono mt-1.5 truncate text-[10.5px] text-slate-500" title={`${tpl.command} ${tpl.args.join(" ")}`}>
                  {tpl.command} {tpl.args.join(" ")}
                </div>
                <div className="mt-2 flex items-center gap-2">
                  <button
                    type="button"
                    className="btn-primary btn-sm"
                    onClick={() => {
                      setEditing(fromTemplate(tpl));
                      setTemplateOpen(false);
                      setFormOpen(true);
                    }}
                  >
                    <Icon name="plus" className="h-3.5 w-3.5" />
                    使用
                  </button>
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => reveal(tpl.docs)}
                    title={tpl.docs}
                  >
                    <Icon name="external" className="h-3.5 w-3.5" />
                    文档
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <p className="mt-3 text-[11px] leading-relaxed text-slate-500">
          模板只是「预先填好的表单」：保存后就是一条普通受管资源，改命令、加环境变量、
          停用、删除都和手工创建的完全一样。npx 型首次启动会现下载包，握手测试可验证。
        </p>
      </Modal>

      {/* 资源编辑器 */}
      <ResourceForm
        open={formOpen}
        onClose={() => setFormOpen(false)}
        initial={editing}
        onSaved={setResources}
      />

      {/* 从扫描结果导入 */}
      <Modal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title="从扫描结果导入 MCP"
        subtitle={`共发现 ${servers.length} 个条目；导入后成为受管资源，可再编辑与分发`}
        footer={
          importSummary ? (
            <button type="button" className="btn-primary" onClick={() => { setImportSummary(null); setImportOpen(false); }}>
              完成
            </button>
          ) : (
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
                导入选中的 {picked.length} 个
              </button>
            </>
          )
        }
      >
        <div className="space-y-1.5">
          {importError && (
            <div className="mb-2 flex items-start gap-2 rounded-lg border border-rose-500/40 bg-rose-950 px-3 py-2.5 text-xs text-rose-200">
              <Icon name="alert" className="mt-0.5 h-4 w-4 shrink-0" />
              <span className="leading-relaxed">导入失败：{importError}</span>
            </div>
          )}
          {importSummary && (
            <div className="mb-2 flex items-start gap-2 rounded-lg border border-brand-500/40 bg-brand-900 px-3 py-2.5 text-xs text-brand-300">
              <Icon name="check" className="mt-0.5 h-4 w-4 shrink-0" />
              <span className="leading-relaxed">{importSummary}</span>
            </div>
          )}
          <label className="flex cursor-pointer items-center gap-2.5 px-1 text-xs text-slate-400">
            <input
              type="checkbox"
              checked={picked.length === servers.length && servers.length > 0}
              onChange={(e) => setPicked(e.target.checked ? servers.map((s) => s.id) : [])}
              className="accent-teal-400"
            />
            全选
          </label>
          {servers.map((server) => (
            <label
              key={server.id}
              className="flex cursor-pointer items-start gap-2.5 rounded-lg border border-ink-800 bg-ink-900 px-3 py-2 hover:bg-ink-800"
            >
              <input
                type="checkbox"
                checked={picked.includes(server.id)}
                onChange={(e) =>
                  setPicked((prev) =>
                    e.target.checked ? [...prev, server.id] : prev.filter((id) => id !== server.id),
                  )
                }
                className="mt-1 accent-teal-400"
              />
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-2">
                  <span className="truncate text-[12.5px] text-slate-200">{server.name}</span>
                  <Badge tone={TRANSPORT_TONE[server.transport] ?? "slate"}>
                    {TRANSPORT_LABEL[server.transport] ?? server.transport}
                  </Badge>
                  <span className="text-[10.5px] text-slate-500">{server.sourceAgent}</span>
                </span>
                <span className="mono mt-0.5 block truncate text-[10.5px]">
                  {server.command
                    ? `${server.command} ${server.args.join(" ")}`.trim()
                    : (server.url ?? "")}
                </span>
              </span>
            </label>
          ))}
        </div>
      </Modal>

      {/* 分发向导 */}
      <SyncDialog
        open={syncOpen}
        onClose={() => setSyncOpen(false)}
        kind="mcp"
        resourceItems={resources.map((r) => ({
          name: r.name,
          tag: r.transport,
          enabled: r.enabled,
        }))}
        onDone={load}
        onReveal={reveal}
      />
    </div>
  );
}