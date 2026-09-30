/** Skills：只读清单 + SKILL.md 预览 + 快捷导入 / 一键清理失效 / 重建链接 / 删除。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import {
  Badge,
  Card,
  Drawer,
  Empty,
  SearchInput,
  SectionCard,
  SegmentedControl,
  StatusDot,
} from "../components/ui";
import {
  CleanupDialog,
  DeleteDialog,
  ImportDialog,
  toBrokenRefs,
} from "../components/SkillActions";
import { api, describeError } from "../lib/api";
import { formatBytes, shortenPath } from "../lib/format";
import { LINK_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { DirEntry, SkillEnv, SkillFound, TextPreview } from "../lib/types";

export function SkillsPage() {
  const snapshot = useApp((s) => s.snapshot);
  const scan = useApp((s) => s.scan);
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();

  const [query, setQuery] = useState("");
  const [source, setSource] = useState("all");
  const [stateFilter, setStateFilter] = useState("all");
  const [categoryFilter, setCategoryFilter] = useState("all");

  const [selected, setSelected] = useState<SkillFound | null>(null);
  const [preview, setPreview] = useState<TextPreview | null>(null);
  const [files, setFiles] = useState<DirEntry[]>([]);

  const [env, setEnv] = useState<SkillEnv | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [cleanupOpen, setCleanupOpen] = useState(false);
  const [deleteSkill, setDeleteSkill] = useState<SkillFound | null>(null);

  const skills = snapshot?.skills ?? [];
  const broken = useMemo(() => toBrokenRefs(skills), [skills]);
  const activeCount = skills.length - broken.length;

  const loadEnv = () => {
    api.skillEnvironment().then(setEnv).catch(() => undefined);
  };

  useEffect(() => {
    loadEnv();
  }, []);

  /** 动作完成后：刷新环境信息并重新扫描，让页面反映真实状态 */
  const afterAction = () => {
    loadEnv();
    void scan();
  };

  const sources = useMemo(() => {
    const map = new Map<string, number>();
    skills.forEach((s) => map.set(s.sourceAgentId, (map.get(s.sourceAgentId) ?? 0) + 1));
    return Array.from(map.entries());
  }, [skills]);

  const categories = useMemo(() => {
    const map = new Map<string, number>();
    skills.forEach((s) => {
      if (s.category) map.set(s.category, (map.get(s.category) ?? 0) + 1);
    });
    return Array.from(map.entries()).sort((a, b) => b[1] - a[1]);
  }, [skills]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return skills.filter((s) => {
      if (stateFilter === "broken" && !s.broken) return false;
      if (stateFilter === "active" && s.broken) return false;
      if (source !== "all" && s.sourceAgentId !== source) return false;
      if (categoryFilter !== "all" && s.category !== categoryFilter) return false;
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        (s.description ?? "").toLowerCase().includes(q) ||
        s.path.toLowerCase().includes(q)
      );
    });
  }, [skills, query, source, stateFilter, categoryFilter]);

  const open = async (skill: SkillFound) => {
    setSelected(skill);
    setPreview(null);
    setFiles([]);
    if (skill.broken) {
      setPreview({
        path: skill.path,
        exists: false,
        bytes: 0,
        truncated: false,
        text: "",
        error: "链接目标不存在，无法读取内容。",
      });
      return;
    }
    try {
      if (skill.hasManifest) {
        const manifestPath = `${skill.path}\\SKILL.md`;
        const [text, entries] = await Promise.all([
          api.readTextPreview(manifestPath, 160 * 1024),
          api.listDir(skill.path, 200),
        ]);
        setPreview(text);
        setFiles(entries);
      } else {
        setPreview(await api.readTextPreview(skill.path, 160 * 1024));
      }
    } catch (error) {
      setBanner(describeError(error));
    }
  };

  /** 有多少链接指向这个 Skill（删除会让它们失效） */
  const linkImpact = (skill: SkillFound) =>
    skills.filter(
      (s) =>
        s.linkTarget != null &&
        s.linkTarget.toLowerCase().startsWith(skill.path.toLowerCase()) &&
        !s.path.toLowerCase().startsWith(skill.path.toLowerCase()),
    ).length;

  return (
    <div className="space-y-4">
      {/* 失效链接汇总条 */}
      {broken.length > 0 && (
        <Card className="border-rose-500/30">
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-rose-300">
              <Icon name="alert" className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium text-rose-100">
                {broken.length} 个 Skill 链接失效
                {activeCount > 0 && (
                  <span className="ml-2 font-normal text-slate-400">（另有 {activeCount} 个可用）</span>
                )}
              </div>
              <div className="mt-0.5 text-[11px] leading-relaxed text-slate-400">
                目标目录被删除或移动后链接没有清理。可「重建链接」把它们指向现有技能库，或「清理失效」删除这些空链接。
              </div>
            </div>
            <button
              type="button"
              className="btn-ghost btn-sm shrink-0"
              onClick={() => setCleanupOpen(true)}
            >
              <Icon name="link" className="h-3.5 w-3.5" />
              重建 / 清理
            </button>
          </div>
        </Card>
      )}

      <SectionCard
        title="Skill 库"
        subtitle={`共 ${skills.length} 个（可用 ${activeCount} · 失效 ${broken.length}），来自 ${sources.length} 个来源目录`}
        action={
          <div className="flex flex-wrap items-center gap-2">
            <button type="button" className="btn-primary btn-sm" onClick={() => setImportOpen(true)}>
              <Icon name="plus" className="h-3.5 w-3.5" />
              快捷导入
            </button>
            {broken.length > 0 && (
              <button
                type="button"
                className="btn bg-rose-600/90 btn-sm text-white hover:bg-rose-500"
                onClick={() => setCleanupOpen(true)}
              >
                <Icon name="close" className="h-3.5 w-3.5" />
                清理失效 {broken.length}
              </button>
            )}
            <div className="w-52">
              <SearchInput value={query} onChange={setQuery} placeholder="搜索名称 / 描述 / 路径…" />
            </div>
          </div>
        }
        bodyClassName="space-y-3"
      >
        <div className="flex flex-wrap items-center gap-3">
          <SegmentedControl
            value={stateFilter}
            onChange={setStateFilter}
            options={[
              { value: "all", label: "全部", count: skills.length },
              { value: "active", label: "可用", count: activeCount },
              { value: "broken", label: "失效", count: broken.length },
            ]}
          />
          {sources.length > 1 && (
            <SegmentedControl
              value={source}
              onChange={setSource}
              options={[
                { value: "all", label: "全部来源", count: skills.length },
                ...sources.map(([id, count]) => ({
                  value: id,
                  label: snapshot?.agents.find((a) => a.id === id)?.name ?? id,
                  count,
                })),
              ]}
            />
          )}
          {categories.length > 1 && (
            <select
              value={categoryFilter}
              onChange={(e) => setCategoryFilter(e.target.value)}
              className="rounded-lg border border-ink-700 bg-ink-900 px-2.5 py-1 text-xs text-slate-300"
            >
              <option value="all">全部分类（{categories.length}）</option>
              {categories.map(([name, count]) => (
                <option key={name} value={name}>
                  {name}（{count}）
                </option>
              ))}
            </select>
          )}
        </div>

        {filtered.length === 0 ? (
          <Empty
            icon="skills"
            title={skills.length === 0 ? "尚未发现 Skill" : "没有匹配的 Skill"}
            description={
              skills.length === 0
                ? "AgentHub 会扫描定义文件里 role = skills 的目录，以及共享技能库。可以用「快捷导入」从本地目录或 Git 仓库导入。"
                : "换个关键词或筛选条件试试。"
            }
            action={
              <button type="button" className="btn-primary mt-2" onClick={() => setImportOpen(true)}>
                <Icon name="plus" className="h-4 w-4" />
                快捷导入
              </button>
            }
          />
        ) : (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
            {filtered.map((skill) => (
              <button
                key={skill.id}
                type="button"
                onClick={() => void open(skill)}
                className={`card group flex flex-col gap-2 p-4 text-left transition-colors ${
                  skill.broken
                    ? "border-rose-500/40 hover:border-rose-500/60"
                    : "hover:border-brand-500/40 hover:bg-ink-800"
                }`}
              >
                <div className="flex items-start gap-2">
                  <span
                    className={`mt-0.5 rounded-md border p-1.5 ${
                      skill.broken
                        ? "border-rose-500/40 text-rose-300"
                        : "border-brand-500/30 text-brand-400"
                    }`}
                  >
                    <Icon name={skill.broken ? "alert" : "skills"} className="h-3.5 w-3.5" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-slate-100">{skill.name}</div>
                    <div className="mono truncate" title={skill.path}>
                      {shortenPath(skill.broken ? (skill.linkTarget ?? skill.path) : skill.path, 44)}
                    </div>
                  </div>
                </div>

                {skill.broken ? (
                  <p className="text-xs leading-relaxed text-rose-300">
                    {skill.linkKind ? (LINK_LABEL[skill.linkKind] ?? skill.linkKind) : "链接"}
                    指向的目标不存在，Agent 侧会静默失效。
                  </p>
                ) : (
                  skill.description && (
                    <p className="line-clamp-3 text-xs leading-relaxed text-slate-400">
                      {skill.description}
                    </p>
                  )
                )}

                <div className="mt-auto flex flex-wrap items-center gap-1.5 pt-1">
                  {skill.broken ? (
                    <Badge tone="rose" icon="alert">
                      链接失效
                    </Badge>
                  ) : (
                    <>
                      <Badge tone={skill.hasManifest ? "teal" : "slate"}>
                        {skill.hasManifest ? "SKILL.md" : "单文件"}
                      </Badge>
                      <Badge tone="slate" icon="folder">
                        {skill.fileCount} 文件 · {formatBytes(skill.bytes)}
                      </Badge>
                      {skill.linkKind && (
                        <Badge tone="violet" icon="link">
                          {LINK_LABEL[skill.linkKind] ?? skill.linkKind}
                        </Badge>
                      )}
                    </>
                  )}
                  {skill.category && <Badge tone="sky">{skill.category}</Badge>}
                  <span className="ml-auto text-[10px] text-slate-500">{skill.sourceAgent}</span>
                </div>
              </button>
            ))}
          </div>
        )}
      </SectionCard>

      {/* 详情抽屉 */}
      <Drawer
        open={selected != null}
        onClose={() => setSelected(null)}
        title={selected?.name ?? ""}
        subtitle={selected?.path}
      >
        {selected && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone="slate" icon="agents">
                来源：{selected.sourceAgent}
              </Badge>
              {selected.broken ? (
                <Badge tone="rose" icon="alert">
                  链接失效
                </Badge>
              ) : (
                <Badge tone="slate" icon="folder">
                  {selected.fileCount} 个文件 · {formatBytes(selected.bytes)}
                </Badge>
              )}
              {selected.category && <Badge tone="sky">分类：{selected.category}</Badge>}
              {selected.linkKind && (
                <Badge tone="violet" icon="link">
                  {LINK_LABEL[selected.linkKind] ?? selected.linkKind} 部署
                </Badge>
              )}
              {selected.updatedAt && <Badge tone="teal">更新于 {selected.updatedAt}</Badge>}
              <div className="ml-auto flex items-center gap-2">
                <button type="button" onClick={() => reveal(selected.path)} className="btn-ghost btn-sm">
                  <Icon name="folder" className="h-3.5 w-3.5" />
                  打开所在目录
                </button>
                <button
                  type="button"
                  onClick={() => setDeleteSkill(selected)}
                  className="btn border border-rose-500/40 btn-sm text-rose-300 hover:bg-rose-950"
                >
                  <Icon name="close" className="h-3.5 w-3.5" />
                  删除
                </button>
              </div>
            </div>

            {selected.linkTarget && (
              <Card
                className={selected.broken ? "border-rose-500/40" : "border-ink-700"}
              >
                <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                  链接目标
                </div>
                <p
                  className={`mono mt-1 break-all ${
                    selected.broken ? "text-rose-300" : "text-slate-300"
                  }`}
                >
                  {selected.linkTarget}
                </p>
                {selected.broken && (
                  <div className="mt-2 space-y-1.5">
                    <p className="text-xs leading-relaxed text-rose-200">
                      该目标不存在，因此 Agent 无法加载这个 Skill。处理方式：
                    </p>
                    <ul className="list-inside list-disc text-[11px] leading-relaxed text-slate-400">
                      <li>把技能库导入 / 恢复到该路径 → 链接自动重新生效</li>
                      <li>用「重建链接」把它指向现有技能库</li>
                      <li>用「清理失效」删除这条空链接</li>
                    </ul>
                    <button
                      type="button"
                      className="btn-ghost btn-sm mt-1"
                      onClick={() => setCleanupOpen(true)}
                    >
                      <Icon name="link" className="h-3.5 w-3.5" />
                      处理失效链接
                    </button>
                  </div>
                )}
              </Card>
            )}

            {selected.broken && (
              <div className="flex items-start gap-2 rounded-lg border border-rose-500/30 px-3 py-2.5 text-xs text-rose-200">
                <Icon name="alert" className="mt-0.5 h-4 w-4 shrink-0" />
                <span className="leading-relaxed">
                  这是链接指向失效，不是 Skill 本身的问题 —— 内容无处可读，故下方不显示预览。
                </span>
              </div>
            )}

            {selected.whenToUse && (
              <Card className="border-ink-700">
                <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                  何时使用
                </div>
                <p className="mt-1 text-xs leading-relaxed text-slate-300">{selected.whenToUse}</p>
              </Card>
            )}

            {files.length > 0 && (
              <div>
                <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                  文件清单
                </div>
                <div className="grid gap-1 sm:grid-cols-2">
                  {files.slice(0, 24).map((file) => (
                    <div
                      key={file.name}
                      className="flex items-center gap-2 rounded-md border border-ink-800 bg-ink-900 px-2.5 py-1.5"
                    >
                      <Icon
                        name={file.isDir ? "folder" : "terminal"}
                        className="h-3 w-3 shrink-0 text-slate-500"
                      />
                      <span className="mono min-w-0 flex-1 truncate">{file.name}</span>
                      {!file.isDir && (
                        <span className="shrink-0 font-mono text-[10px] text-slate-600">
                          {formatBytes(file.size)}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div>
              <div className="mb-1.5 flex items-center gap-2">
                <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-500">
                  {selected.hasManifest ? "SKILL.md 内容" : "文件内容"}
                </span>
                {preview?.truncated && <Badge tone="amber">已截断预览</Badge>}
                <StatusDot state={preview?.exists ? "ok" : "warn"} className="ml-auto" />
              </div>
              {preview?.error ? (
                <p className="text-xs text-rose-300">{preview.error}</p>
              ) : (
                <pre className="max-h-[420px] overflow-auto rounded-lg border border-ink-800 bg-ink-950 p-3 text-[11.5px] leading-relaxed text-slate-300">
                  {preview?.text ?? "加载中…"}
                </pre>
              )}
            </div>
          </div>
        )}
      </Drawer>

      {/* 动作弹窗 */}
      <ImportDialog
        open={importOpen}
        onClose={() => {
          setImportOpen(false);
          afterAction();
        }}
        env={env}
        onDone={afterAction}
        onReveal={reveal}
      />
      <CleanupDialog
        open={cleanupOpen}
        onClose={() => {
          setCleanupOpen(false);
          afterAction();
        }}
        broken={broken}
        env={env}
        onDone={afterAction}
        onReveal={reveal}
      />
      <DeleteDialog
        open={deleteSkill != null}
        onClose={() => {
          setDeleteSkill(null);
          afterAction();
        }}
        skill={deleteSkill}
        impact={deleteSkill ? linkImpact(deleteSkill) : 0}
        onDone={afterAction}
        onReveal={reveal}
      />
    </div>
  );
}