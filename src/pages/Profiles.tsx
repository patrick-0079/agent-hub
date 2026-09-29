/** Profiles：把「哪套 MCP + 哪个供应商 + 哪些 Skill」打包成档案，一键应用到多个 Agent。 */

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
} from "../components/ui";
import { api, describeError } from "../lib/api";
import { formatBytes, shortenPath } from "../lib/format";
import { useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type {
  McpResource,
  ProfileDetail,
  ProfileExportMeta,
  ProfileExportOutcome,
  ProfileItem,
  ProfileResource,
  ProviderResource,
} from "../lib/types";

function emptyProfile(): ProfileResource {
  return {
    id: 0,
    name: "",
    description: "",
    agents: [],
    counts: { mcp: 0, provider: 0, skill: 0 },
    updatedAt: "",
  };
}

/* ------------------------------------------------------------ 档案编辑器 */

function ProfileEditor({
  open,
  onClose,
  initial,
  mcpList,
  providerList,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  initial: ProfileDetail | null;
  mcpList: McpResource[];
  providerList: ProviderResource[];
  onSaved: (list: ProfileResource[]) => void;
}) {
  const snapshot = useApp((s) => s.snapshot);
  const setBanner = useApp((s) => s.setBanner);

  const [draft, setDraft] = useState<ProfileResource>(emptyProfile());
  const [agents, setAgents] = useState<string[]>([]);
  const [mcp, setMcp] = useState<string[]>([]);
  const [providers, setProviders] = useState<string[]>([]);
  const [skills, setSkills] = useState<string[]>([]);
  const [skillQuery, setSkillQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState("mcp");

  useEffect(() => {
    if (!open) return;
    const profile = initial?.profile ?? emptyProfile();
    setDraft(profile);
    setAgents(profile.agents);
    const items = initial?.items ?? [];
    setMcp(items.filter((i) => i.resourceType === "mcp").map((i) => i.resourceRef));
    setProviders(items.filter((i) => i.resourceType === "provider").map((i) => i.resourceRef));
    setSkills(items.filter((i) => i.resourceType === "skill").map((i) => i.resourceRef));
    setSkillQuery("");
    setTab("mcp");
  }, [open, initial]);

  /** 可选的 Agent（已安装 / 有痕迹） */
  const agentOptions = useMemo(() => {
    const list = snapshot?.agents ?? [];
    return list
      .filter((a) => a.kind !== "host" && a.status !== "absent")
      .map((a) => ({ id: a.id, name: a.name, accent: a.accent, status: a.status }));
  }, [snapshot]);

  /** 可选的 Skill（排除失效链接） */
  const skillOptions = useMemo(() => {
    const list = (snapshot?.skills ?? []).filter((s) => !s.broken);
    const q = skillQuery.trim().toLowerCase();
    const filtered = q
      ? list.filter(
          (s) =>
            s.name.toLowerCase().includes(q) ||
            (s.category ?? "").toLowerCase().includes(q) ||
            s.path.toLowerCase().includes(q),
        )
      : list;
    return { total: list.length, matched: filtered, shown: filtered.slice(0, 150) };
  }, [snapshot, skillQuery]);

  const toggle = (
    value: string,
    current: string[],
    setter: (next: string[]) => void,
  ) => setter(current.includes(value) ? current.filter((v) => v !== value) : [...current, value]);

  const save = async () => {
    setBusy(true);
    try {
      const items: ProfileItem[] = [
        ...mcp.map((ref) => ({ id: 0, resourceType: "mcp", resourceRef: ref, display: ref })),
        ...providers.map((ref) => ({
          id: 0,
          resourceType: "provider",
          resourceRef: ref,
          display: ref,
        })),
        ...skills.map((ref) => {
          const found = (snapshot?.skills ?? []).find((s) => s.path === ref);
          return {
            id: 0,
            resourceType: "skill",
            resourceRef: ref,
            display: found?.dirName ?? ref.split(/[\\/]/).pop() ?? ref,
          };
        }),
      ];
      onSaved(await api.profileSave({ ...draft, agents }, items));
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
      title={draft.id > 0 ? `编辑档案：${draft.name}` : "新建环境档案"}
      subtitle="档案 = 一套环境；应用时一次确认，配置写入与 Skill 部署一起完成"
      width="max-w-4xl"
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
            {busy ? "保存中…" : "保存档案"}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="text-xs text-slate-400">档案名称</label>
            <input
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              placeholder="例如 日常开发"
              className="input mt-1.5 text-sm"
            />
          </div>
          <div>
            <label className="text-xs text-slate-400">说明</label>
            <input
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              placeholder="这套环境用来做什么"
              className="input mt-1.5 text-sm"
            />
          </div>
        </div>

        {/* Agent 绑定 */}
        <Card className="border-ink-700/60">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-medium text-slate-200">绑定 Agent</span>
            <span className="text-[11px] text-slate-500">
              记录「这套档案给谁用」；应用时仍可临时改选
            </span>
            <span className="ml-auto text-[11px] text-slate-500">已选 {agents.length}</span>
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {agentOptions.map((agent) => (
              <button
                key={agent.id}
                type="button"
                onClick={() => toggle(agent.id, agents, setAgents)}
                className={`flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-[11.5px] transition-colors ${
                  agents.includes(agent.id)
                    ? "border-brand-500/50 bg-brand-900 text-brand-300"
                    : "border-ink-700 bg-ink-900 text-slate-400 hover:bg-ink-800"
                }`}
              >
                <span className="h-3.5 w-1 rounded" style={{ background: agent.accent }} />
                {agent.name}
              </button>
            ))}
            {agentOptions.length === 0 && (
              <span className="text-xs text-slate-500">未发现已安装的 Agent</span>
            )}
          </div>
        </Card>

        {/* 资源选择 */}
        <div>
          <SegmentedControl
            value={tab}
            onChange={setTab}
            options={[
              { value: "mcp", label: "MCP", count: mcp.length },
              { value: "provider", label: "供应商", count: providers.length },
              { value: "skill", label: "Skill", count: skills.length },
            ]}
          />

          <div className="mt-3 max-h-72 overflow-auto rounded-lg border border-ink-800 bg-ink-900 p-2">
            {tab === "mcp" &&
              (mcpList.length === 0 ? (
                <p className="px-2 py-4 text-xs text-slate-500">
                  资源库里还没有 MCP —— 先去「MCP 服务器」页导入或新增
                </p>
              ) : (
                mcpList.map((item) => (
                  <label
                    key={item.id}
                    className="flex cursor-pointer items-center gap-2.5 rounded px-2 py-1.5 hover:bg-ink-800"
                  >
                    <input
                      type="checkbox"
                      checked={mcp.includes(item.name)}
                      onChange={() => toggle(item.name, mcp, setMcp)}
                      className="accent-teal-400"
                    />
                    <span className="min-w-0 flex-1 truncate text-[12.5px] text-slate-200">
                      {item.name}
                    </span>
                    <Badge tone="sky">{item.transport}</Badge>
                    {!item.enabled && <Badge tone="slate">已停用</Badge>}
                  </label>
                ))
              ))}

            {tab === "provider" &&
              (providerList.length === 0 ? (
                <p className="px-2 py-4 text-xs text-slate-500">
                  资源库里还没有供应商 —— 先去「模型供应商」页新增
                </p>
              ) : (
                providerList.map((item) => (
                  <label
                    key={item.id}
                    className="flex cursor-pointer items-center gap-2.5 rounded px-2 py-1.5 hover:bg-ink-800"
                  >
                    <input
                      type="checkbox"
                      checked={providers.includes(item.name)}
                      onChange={() => toggle(item.name, providers, setProviders)}
                      className="accent-teal-400"
                    />
                    <span className="min-w-0 flex-1 truncate text-[12.5px] text-slate-200">
                      {item.name}
                    </span>
                    <span className="mono truncate text-[10.5px] text-slate-500">
                      {shortenPath(item.baseUrl || "（未设置 URL）", 36)}
                    </span>
                    {item.hasKey ? (
                      <Badge tone="teal" icon="lock">
                        有密钥
                      </Badge>
                    ) : (
                      <Badge tone="amber">无密钥</Badge>
                    )}
                  </label>
                ))
              ))}

            {tab === "skill" && (
              <div className="space-y-2">
                <div className="flex flex-wrap items-center gap-2 px-1">
                  <div className="w-52">
                    <SearchInput
                      value={skillQuery}
                      onChange={setSkillQuery}
                      placeholder="搜索 Skill / 分类…"
                    />
                  </div>
                  <span className="text-[11px] text-slate-500">
                    可用 Skill {skillOptions.total} 个（已排除失效链接）
                    {skillQuery && `，匹配 ${skillOptions.matched.length} 个`}
                  </span>
                  <button
                    type="button"
                    className="ml-auto text-[11px] text-slate-500 hover:text-brand-400"
                    onClick={() =>
                      setSkills(
                        Array.from(
                          new Set([
                            ...skills,
                            ...skillOptions.shown.map((s) => s.path),
                          ]),
                        ),
                      )
                    }
                  >
                    选中当前列出的全部
                  </button>
                  {skills.length > 0 && (
                    <button
                      type="button"
                      className="text-[11px] text-slate-500 hover:text-rose-300"
                      onClick={() => setSkills([])}
                    >
                      清空
                    </button>
                  )}
                </div>
                {skillOptions.shown.map((skill) => (
                  <label
                    key={skill.id}
                    className="flex cursor-pointer items-center gap-2.5 rounded px-2 py-1.5 hover:bg-ink-800"
                  >
                    <input
                      type="checkbox"
                      checked={skills.includes(skill.path)}
                      onChange={() => toggle(skill.path, skills, setSkills)}
                      className="accent-teal-400"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[12.5px] text-slate-200">
                        {skill.name}
                      </span>
                      <span className="mono block truncate text-[10px] text-slate-500">
                        {shortenPath(skill.path, 60)}
                      </span>
                    </span>
                    {skill.category && <Badge tone="sky">{skill.category}</Badge>}
                    {skill.linkKind && <Badge tone="violet">链接</Badge>}
                  </label>
                ))}
                {skillOptions.matched.length > skillOptions.shown.length && (
                  <p className="px-2 py-1 text-[11px] text-slate-600">
                    仅显示前 {skillOptions.shown.length} 个，用搜索缩小范围
                  </p>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- 页面 */

export function ProfilesPage() {
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [profiles, setProfiles] = useState<ProfileResource[]>([]);
  const [mcpList, setMcpList] = useState<McpResource[]>([]);
  const [providerList, setProviderList] = useState<ProviderResource[]>([]);
  const [loading, setLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editing, setEditing] = useState<ProfileDetail | null>(null);
  const [applyProfile, setApplyProfile] = useState<ProfileDetail | null>(null);

  /* 导出 / 导入 */
  const [importOpen, setImportOpen] = useState(false);
  const [exportMetas, setExportMetas] = useState<ProfileExportMeta[]>([]);
  const [manualPath, setManualPath] = useState("");
  const [importBusy, setImportBusy] = useState(false);
  const [lastExport, setLastExport] = useState<ProfileExportOutcome | null>(null);

  const load = () => {
    setLoading(true);
    Promise.all([api.profileList(), api.mcpResources(), api.providerResources()])
      .then(([list, mcpRes, provRes]) => {
        setProfiles(list);
        setMcpList(mcpRes);
        setProviderList(provRes);
      })
      .catch((error) => setBanner(describeError(error)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshExports = () => {
    api
      .profileExportList()
      .then(setExportMetas)
      .catch(() => setExportMetas([]));
  };

  /** 导出一个档案：文件落在数据目录 exports/，界面给出路径与打开入口 */
  const exportOne = async (profile: ProfileResource) => {
    try {
      const outcome = await api.profileExport(profile.id);
      setLastExport(outcome);
      refreshExports();
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const importFrom = async (path: string) => {
    if (!path.trim()) return;
    setImportBusy(true);
    try {
      const detail = await api.profileImport(path.trim());
      setImportOpen(false);
      setManualPath("");
      load();
      setBanner(`已导入档案「${detail.profile.name}」（${detail.items.length} 项资源）`);
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setImportBusy(false);
    }
  };

  const openEditor = async (profile?: ProfileResource) => {
    if (!profile) {
      setEditing(null);
      setEditorOpen(true);
      return;
    }
    try {
      setEditing(await api.profileDetail(profile.id));
      setEditorOpen(true);
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const openApply = async (profile: ProfileResource) => {
    try {
      setApplyProfile(await api.profileDetail(profile.id));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  const remove = async (profile: ProfileResource) => {
    try {
      setProfiles(await api.profileDelete(profile.id));
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  return (
    <div className="space-y-4">
      <SectionCard
        title="环境档案"
        subtitle={`共 ${profiles.length} 个档案 —— 每个档案是一套「MCP + 供应商 + Skill」的组合`}
        action={
          <div className="flex items-center gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={load} disabled={loading}>
              <Icon name="refresh" className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} strokeWidth={2} />
              刷新
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm"
              onClick={() => {
                refreshExports();
                setImportOpen(true);
              }}
            >
              <Icon name="external" className="h-3.5 w-3.5" />
              导入档案
            </button>
            <button type="button" className="btn-primary btn-sm" onClick={() => void openEditor()}>
              <Icon name="plus" className="h-3.5 w-3.5" />
              新建档案
            </button>
          </div>
        }
        bodyClassName="space-y-3"
      >
        {profiles.length === 0 ? (
          <Empty
            icon="profiles"
            title={loading ? "正在读取档案…" : "还没有环境档案"}
            description="档案把「哪套 MCP + 哪个供应商 + 哪些 Skill」打包成一套环境，之后一键应用到任意多个 Agent —— 配置写入与 Skill 部署一次完成，写前自动备份。"
            action={
              <button type="button" className="btn-primary mt-2" onClick={() => void openEditor()}>
                <Icon name="plus" className="h-4 w-4" />
                新建档案
              </button>
            }
          />
        ) : (
          <div className="grid gap-3 xl:grid-cols-2">
            {profiles.map((profile) => (
              <Card key={profile.id} className="border-ink-700/60">
                <div className="flex items-start gap-3">
                  <span className="rounded-lg border border-accent-500/40 bg-accent-900 p-2 text-accent-400">
                    <Icon name="profiles" className="h-4 w-4" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-semibold text-slate-100">
                        {profile.name}
                      </span>
                      <Badge tone="sky" icon="mcp">
                        MCP {profile.counts.mcp}
                      </Badge>
                      <Badge tone="rose" icon="providers">
                        供应商 {profile.counts.provider}
                      </Badge>
                      <Badge tone="violet" icon="skills">
                        Skill {profile.counts.skill}
                      </Badge>
                    </div>
                    {profile.description && (
                      <p className="mt-1 text-xs leading-relaxed text-slate-400">
                        {profile.description}
                      </p>
                    )}
                    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                      <span className="text-[10.5px] text-slate-500">绑定 Agent：</span>
                      {profile.agents.length === 0 ? (
                        <span className="text-[10.5px] text-slate-600">未绑定</span>
                      ) : (
                        profile.agents.map((id) => (
                          <Badge key={id} tone="slate">
                            {id}
                          </Badge>
                        ))
                      )}
                    </div>
                  </div>
                </div>

                <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-ink-800/70 pt-3">
                  <button
                    type="button"
                    className="btn-primary btn-sm"
                    onClick={() => void openApply(profile)}
                  >
                    <Icon name="play" className="h-3.5 w-3.5" />
                    应用到 Agent
                  </button>
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => void openEditor(profile)}
                  >
                    <Icon name="settings" className="h-3.5 w-3.5" />
                    编辑
                  </button>
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    onClick={() => void exportOne(profile)}
                    title="导出为自包含 JSON（不含任何密钥），落在数据目录 exports/ 下"
                  >
                    <Icon name="external" className="h-3.5 w-3.5" />
                    导出
                  </button>
                  <button
                    type="button"
                    className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                    onClick={() => void remove(profile)}
                  >
                    <Icon name="close" className="h-3.5 w-3.5" />
                    删除
                  </button>
                  <span className="ml-auto font-mono text-[10.5px] text-slate-600">
                    更新于 {profile.updatedAt || "—"}
                  </span>
                </div>
              </Card>
            ))}
          </div>
        )}
      </SectionCard>

      {profiles.length > 0 && (
        <div className="grid gap-3 md:grid-cols-3">
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="search" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">应用前先看 diff</span>
              <Badge tone="violet" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              配置文件逐个展示键级变更与行级 diff；Skill 部署列出每个 Skill 的动作（部署 / 未变 / 跳过）。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="shield" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">写前备份、可回滚</span>
              <Badge tone="violet" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              配置文件备份到「历史与审计 → 配置备份」；Skill 部署记录可撤销清单。
            </p>
          </Card>
          <Card className="border-dashed">
            <div className="flex items-center gap-2">
              <Icon name="link" className="h-4 w-4 text-accent-400" />
              <span className="text-sm text-slate-200">档案导出 / 分享</span>
              <Badge tone="teal" className="ml-auto">
                已实现
              </Badge>
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
              一个 JSON 带走完整环境定义（密钥默认剔除），落在数据目录 exports/ 下，拷给别人即可导入；同名导入自动加后缀，互不覆盖。
            </p>
          </Card>
        </div>
      )}

      {/* 导出结果反馈 */}
      {lastExport && (
        <Card className="border-brand-500/30">
          <div className="flex flex-wrap items-center gap-3">
            <span className="rounded-md border border-brand-800 bg-brand-900 p-2 text-brand-400">
              <Icon name="external" className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-sm text-slate-200">
                已导出「{lastExport.name}」（{lastExport.items} 项资源，不含任何密钥）
              </div>
              <div className="mono mt-0.5 truncate" title={lastExport.path}>
                {lastExport.path}
              </div>
            </div>
            <button
              type="button"
              className="btn-ghost btn-sm shrink-0"
              onClick={() => reveal(lastExport.path)}
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开文件位置
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm shrink-0"
              onClick={() => setLastExport(null)}
            >
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
        </Card>
      )}

      {/* 导入档案 */}
      <Modal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        title="导入环境档案"
        subtitle="从导出目录选择，或粘贴任意的 .agenthub-profile.json 文件路径（密钥永不包含在文件里）"
        width="max-w-2xl"
        footer={
          <>
            <button type="button" className="btn-ghost" onClick={() => setImportOpen(false)}>
              取消
            </button>
            <button
              type="button"
              className="btn-primary"
              disabled={!manualPath.trim() || importBusy}
              onClick={() => void importFrom(manualPath)}
            >
              {importBusy ? "导入中…" : "从路径导入"}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <div>
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-medium text-slate-300">
                导出目录（{exportMetas.length} 个文件）
              </span>
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={() =>
                  exportMetas[0]
                    ? reveal(exportMetas[0].path)
                    : setBanner("导出目录还是空的：先在上方卡片点「导出」")
                }
              >
                <Icon name="folder" className="h-3.5 w-3.5" />
                打开导出目录
              </button>
            </div>
            {exportMetas.length === 0 ? (
              <Empty
                icon="profiles"
                title="导出目录还没有文件"
                description="在本页档案卡片上点「导出」，或把别人分享的 .agenthub-profile.json 放进导出目录后点「刷新」。"
              />
            ) : (
              <div className="space-y-2">
                {exportMetas.map((meta) => (
                  <div
                    key={meta.path}
                    className="flex flex-wrap items-center gap-2 rounded-md border border-ink-800 bg-ink-900 px-3 py-2.5"
                  >
                    <Icon name="profiles" className="h-4 w-4 shrink-0 text-slate-400" />
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="truncate text-sm font-medium text-slate-100">
                          {meta.name}
                        </span>
                        {meta.skillItems > 0 && (
                          <Badge tone="violet" icon="skills">
                            Skill {meta.skillItems}
                          </Badge>
                        )}
                        {meta.mcpItems > 0 && (
                          <Badge tone="sky" icon="mcp">
                            MCP {meta.mcpItems}
                          </Badge>
                        )}
                        {meta.providerItems > 0 && (
                          <Badge tone="rose" icon="providers">
                            供应商 {meta.providerItems}
                          </Badge>
                        )}
                      </span>
                      <span className="mono mt-0.5 block truncate" title={meta.path}>
                        {shortenPath(meta.path, 64)} · {formatBytes(meta.bytes)} ·{" "}
                        {meta.exportedAt || "时间未知"}
                      </span>
                    </span>
                    <button
                      type="button"
                      className="btn-primary btn-sm shrink-0"
                      disabled={importBusy}
                      onClick={() => void importFrom(meta.path)}
                    >
                      <Icon name="plus" className="h-3.5 w-3.5" />
                      导入
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
          <div>
            <label className="mb-1.5 block text-xs font-medium text-slate-300">
              从任意路径导入
            </label>
            <div className="flex items-center gap-2">
              <input
                className="input"
                placeholder="C:\ ...\my-profile.agenthub-profile.json"
                value={manualPath}
                onChange={(e) => setManualPath(e.target.value)}
              />
              <button
                type="button"
                className="btn-ghost btn-sm shrink-0"
                onClick={refreshExports}
              >
                <Icon name="refresh" className="h-3.5 w-3.5" />
                刷新
              </button>
            </div>
            <p className="mt-1.5 text-[11px] leading-relaxed text-slate-500">
              同名档案不会互相覆盖：导入时自动加「（导入）」后缀；导出文件不包含 API Key，
              导入后需要在「模型供应商」里重新录入密钥。
            </p>
          </div>
        </div>
      </Modal>

      <ProfileEditor
        open={editorOpen}
        onClose={() => setEditorOpen(false)}
        initial={editing}
        mcpList={mcpList}
        providerList={providerList}
        onSaved={setProfiles}
      />

      <SyncDialog
        open={applyProfile != null}
        onClose={() => {
          setApplyProfile(null);
          load();
        }}
        kind="profile"
        profileId={applyProfile?.profile.id}
        resourceItems={[
          ...(applyProfile?.items ?? []).map((item) => ({
            name: item.display,
            tag:
              item.resourceType === "mcp"
                ? "MCP"
                : item.resourceType === "provider"
                  ? "供应商"
                  : "Skill",
            enabled: true,
          })),
        ]}
        onDone={load}
        onReveal={reveal}
      />
    </div>
  );
}