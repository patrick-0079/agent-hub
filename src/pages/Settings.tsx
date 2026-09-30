/** 设置：工具链探测面板（含手动指定路径）、扫描范围、数据与外观。 */

import React, { useEffect, useMemo, useState } from "react";
import { Icon, type IconName } from "../components/Icon";
import { Badge, Card, Empty, PathRow, SectionCard, StatusDot, Toggle, useCopy } from "../components/ui";
import { api, describeError } from "../lib/api";
import { formatBytes } from "../lib/format";
import { CATEGORY_LABEL, useReveal } from "../lib/hooks";
import { useApp } from "../lib/store";
import type { ExecutableInfo, MigrationMeta, MigrationOutcome } from "../lib/types";

function PathListEditor({
  title,
  hint,
  values,
  onChange,
  placeholder,
}: {
  title: string;
  hint: string;
  values: string[];
  onChange: (next: string[]) => void;
  placeholder: string;
}) {
  const [draft, setDraft] = useState("");
  return (
    <div className="py-3">
      <div className="text-sm text-slate-200">{title}</div>
      <div className="mt-0.5 text-xs text-slate-500">{hint}</div>
      <div className="mt-2 space-y-1.5">
        {values.map((value, index) => (
          <div
            key={`${value}-${index}`}
            className="flex items-center gap-2 rounded-lg border border-ink-700 bg-ink-900 px-3 py-1.5"
          >
            <Icon name="folder" className="h-3.5 w-3.5 shrink-0 text-slate-500" />
            <span className="mono min-w-0 flex-1 truncate" title={value}>
              {value}
            </span>
            <button
              type="button"
              onClick={() => onChange(values.filter((_, i) => i !== index))}
              className="rounded-md p-1 text-slate-500 hover:bg-ink-800 hover:text-rose-300"
              title="移除"
            >
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
        {values.length === 0 && <p className="text-xs text-slate-600">尚未添加自定义目录</p>}
      </div>
      <div className="mt-2 flex gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={placeholder}
          className="input"
          onKeyDown={(e) => {
            if (e.key === "Enter" && draft.trim()) {
              onChange([...values, draft.trim()]);
              setDraft("");
            }
          }}
        />
        <button
          type="button"
          disabled={!draft.trim()}
          onClick={() => {
            onChange([...values, draft.trim()]);
            setDraft("");
          }}
          className="btn-ghost shrink-0"
        >
          <Icon name="plus" className="h-4 w-4" />
          添加
        </button>
      </div>
    </div>
  );
}

export function Settings() {
  const settings = useApp((s) => s.settings);
  const snapshot = useApp((s) => s.snapshot);
  const host = useApp((s) => s.host);
  const patchSettings = useApp((s) => s.patchSettings);
  const setBanner = useApp((s) => s.setBanner);
  const navigate = useApp((s) => s.navigate);
  const reveal = useReveal();
  const [, copy] = useCopy();

  const [execs, setExecs] = useState<ExecutableInfo[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [draftPath, setDraftPath] = useState("");

  const list = useMemo(() => execs ?? snapshot?.executables ?? [], [execs, snapshot]);

  const redetect = async () => {
    setBusy(true);
    try {
      setExecs(await api.detectExecutables());
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!snapshot && !execs && settings) void redetect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, settings]);

  const overrides = settings?.executableOverrides ?? {};

  const applyOverride = async (name: string, path: string) => {
    const next = { ...overrides };
    if (path.trim()) next[name] = path.trim();
    else delete next[name];
    await patchSettings({ executableOverrides: next });
    setEditing(null);
    setDraftPath("");
    await redetect();
  };

  const grouped = list.reduce<Record<string, ExecutableInfo[]>>((acc, item) => {
    (acc[item.category] ??= []).push(item);
    return acc;
  }, {});

  if (!settings) {
    return (
      <Card>
        <p className="text-sm text-slate-400">正在加载设置…</p>
      </Card>
    );
  }

  const readyCount = list.filter((e) => e.found).length;

  return (
    <div className="space-y-4">
      {/* 工具链探测 */}
      <SectionCard
        title="工具链探测面板"
        subtitle={`已定位 ${readyCount}/${list.length} 个组件。未找到的组件可以手动指定可执行文件路径。`}
        action={
          <button type="button" onClick={() => void redetect()} disabled={busy} className="btn-ghost btn-sm">
            <Icon name="refresh" className={`h-3.5 w-3.5 ${busy ? "animate-spin" : ""}`} strokeWidth={2} />
            {busy ? "探测中" : "重新探测"}
          </button>
        }
        bodyClassName="space-y-4"
      >
        {Object.entries(grouped).map(([category, items]) => (
          <div key={category}>
            <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-slate-600">
              {CATEGORY_LABEL[category] ?? category}
            </div>
            <div className="space-y-2">
              {items.map((item) => {
                const overridden = overrides[item.name] != null;
                return (
                  <div
                    key={item.name}
                    className="rounded-lg border border-ink-800 bg-ink-900 px-3 py-2.5"
                  >
                    <div className="flex items-center gap-3">
                      <StatusDot state={item.found ? "ok" : "idle"} />
                      <span className="w-24 shrink-0 text-sm text-slate-200">{item.displayName}</span>
                      <span className="mono min-w-0 flex-1 truncate" title={item.path ?? ""}>
                        {item.path ?? <span className="text-slate-600">未找到</span>}
                      </span>
                      {item.version && (
                        <span className="shrink-0 font-mono text-[11px] text-brand-400">
                          v{item.version}
                        </span>
                      )}
                      {overridden && (
                        <Badge tone="violet" icon="link">
                          手动指定
                        </Badge>
                      )}
                      {!overridden && item.source === "known-location" && (
                        <Badge tone="amber">非 PATH</Badge>
                      )}
                      <div className="flex shrink-0 items-center gap-1">
                        {item.path && (
                          <button
                            type="button"
                            onClick={() => reveal(item.path as string)}
                            className="rounded-md border border-ink-700 p-1.5 text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                            title="打开所在目录"
                          >
                            <Icon name="folder" className="h-3.5 w-3.5" />
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => {
                            setEditing(editing === item.name ? null : item.name);
                            setDraftPath(overrides[item.name] ?? item.path ?? "");
                          }}
                          className="rounded-md border border-ink-700 px-2 py-1 text-[11px] text-slate-400 hover:bg-ink-800 hover:text-slate-200"
                        >
                          {editing === item.name ? "取消" : "指定路径"}
                        </button>
                      </div>
                    </div>

                    {/* 手动指定路径 */}
                    {editing === item.name && (
                      <div className="mt-2.5 flex gap-2 border-t border-ink-800 pt-2.5">
                        <input
                          value={draftPath}
                          onChange={(e) => setDraftPath(e.target.value)}
                          placeholder="例如 %USERPROFILE%\.local\bin\uv.exe"
                          className="input font-mono text-xs"
                        />
                        <button
                          type="button"
                          onClick={() => void applyOverride(item.name, draftPath)}
                          className="btn-primary btn-sm shrink-0"
                        >
                          保存
                        </button>
                        {overridden && (
                          <button
                            type="button"
                            onClick={() => void applyOverride(item.name, "")}
                            className="btn-ghost btn-sm shrink-0"
                          >
                            清除覆盖
                          </button>
                        )}
                      </div>
                    )}

                    {!item.found && (
                      <div className="mt-1.5 flex items-center gap-2 pl-5 text-[11px] text-slate-500">
                        <Icon name="info" className="h-3 w-3" />
                        安装建议：<span className="font-mono">{item.hint}</span>
                        <span className="text-slate-600">·</span>
                        <span>被以下能力依赖：{item.usedBy.join("、")}</span>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </SectionCard>

      <div className="grid gap-4 xl:grid-cols-2">
        {/* 扫描范围 */}
        <SectionCard title="扫描范围" subtitle="控制 AgentHub 去哪里发现虚拟环境与 Skill">
          <PathListEditor
            title="额外虚拟环境搜索目录"
            hint="除用户主目录与当前目录外，额外搜索 pyvenv.cfg 的根目录（便于发现项目里的 .venv）。"
            values={settings.extraScanRoots}
            onChange={(next) => void patchSettings({ extraScanRoots: next })}
            placeholder="例如 A:\Projects"
          />
          <div className="my-2 border-t border-ink-800" />
          <PathListEditor
            title="额外 Skill 存放目录"
            hint="自定义 Agent 的 Skill 目录，或你自己维护的 Skill 库。"
            values={settings.extraSkillRoots}
            onChange={(next) => void patchSettings({ extraSkillRoots: next })}
            placeholder="例如 A:\Skills"
          />
          <div className="my-2 border-t border-ink-800" />
          <div className="flex items-center justify-between py-2.5">
            <div>
              <div className="text-sm text-slate-200">主目录扫描深度</div>
              <div className="mt-0.5 text-xs text-slate-500">
                深度越大越容易发现嵌套环境，但扫描更慢（1–6）
              </div>
            </div>
            <input
              type="number"
              min={1}
              max={6}
              value={settings.scanHomeDepth}
              onChange={(e) =>
                void patchSettings({ scanHomeDepth: Math.min(6, Math.max(1, Number(e.target.value) || 3)) })
              }
              className="input w-20 text-center"
            />
          </div>
          <div className="border-t border-ink-800">
            <Toggle
              checked={settings.autoScanOnStart}
              onChange={(next) => void patchSettings({ autoScanOnStart: next })}
              label="启动时自动扫描"
              hint="首次启动且尚无快照时自动执行一次环境侦察"
            />
          </div>
        </SectionCard>

        <div className="space-y-4">
          {/* Agent 定义 */}
          <SectionCard
            title="Agent 定义"
            subtitle="Agent 去哪找、怎么判定、允许写到哪一级，全部由定义文件描述"
            action={<Badge tone="violet">配置驱动</Badge>}
          >
            <div className="py-1">
              <div className="text-sm text-slate-200">定义目录</div>
              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">
                留空则使用数据目录下的 <span className="font-mono">agents/</span>。把自定义 Agent
                的 <span className="font-mono">.toml</span> 放进该目录即可接入；同名文件会覆盖内置定义。
              </div>
              <input
                value={settings.definitionsDir}
                onChange={(e) => void patchSettings({ definitionsDir: e.target.value })}
                placeholder={host ? `${host.dataDir}\\agents` : "%APPDATA%\\dev.agenthub.desktop\\agents"}
                className="input mt-2 font-mono text-xs"
              />
            </div>
            <div className="flex flex-wrap items-center gap-2 border-t border-ink-800 pt-3">
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={async () => {
                  try {
                    const count = await api.seedAgentDefinitions();
                    setBanner(
                      count > 0
                        ? `已导出 ${count} 个内置定义到用户目录，可直接编辑`
                        : "所有内置定义都已存在于用户目录",
                    );
                  } catch (error) {
                    setBanner(describeError(error));
                  }
                }}
              >
                <Icon name="plus" className="h-3.5 w-3.5" />
                导出内置定义
              </button>
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={() => navigate("adapter")}
              >
                <Icon name="adapter" className="h-3.5 w-3.5" />
                查看定义与能力分级
              </button>
            </div>
          </SectionCard>

          {/* 技能库与网络 */}
          <SectionCard
            title="技能库与网络"
            subtitle="导入目标与删除白名单；Git 导入遵循这里的代理设置"
            action={<Badge tone="violet">T2/T3 作用范围</Badge>}
          >
            <PathListEditor
              title="技能库目录"
              hint="「快捷导入」默认写入这里；「删除 Skill」也只允许操作这些目录内的内容（留空则使用内置默认值）。"
              values={settings.skillLibraries}
              onChange={(next) => void patchSettings({ skillLibraries: next })}
              placeholder="例如 ~/.agent/skills"
            />
            <div className="my-2 border-t border-ink-800" />
            <div className="py-1">
              <div className="text-sm text-slate-200">网络代理</div>
              <div className="mt-0.5 text-xs leading-relaxed text-slate-500">
                仅作用于 AgentHub 发起的 git 克隆等子进程，不会修改你的全局 git 配置。留空表示直连。
              </div>
              <input
                value={settings.networkProxy}
                onChange={(e) => void patchSettings({ networkProxy: e.target.value })}
                placeholder="http://127.0.0.1:7897"
                className="input mt-2 font-mono text-xs"
              />
            </div>
            <div className="flex flex-wrap items-center gap-2 border-t border-ink-800 pt-3">
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={async () => {
                  try {
                    const env = await api.skillEnvironment();
                    setBanner(
                      env.git
                        ? `git 可用：${env.git}；代理 ${env.proxy || "未设置"}；技能库 ${env.libraries.length} 个`
                        : "未检测到 git，Git 仓库导入不可用（可在设置 → 工具链探测面板手动指定路径）",
                    );
                  } catch (error) {
                    setBanner(describeError(error));
                  }
                }}
              >
                <Icon name="link" className="h-3.5 w-3.5" />
                检测 git 与技能库
              </button>
              <button type="button" className="btn-ghost btn-sm" onClick={() => navigate("history")}>
                <Icon name="history" className="h-3.5 w-3.5" />
                回收站与操作清单
              </button>
            </div>
          </SectionCard>

          {/* 同步与备份 */}
          <SectionCard
            title="同步与备份"
            subtitle="M2 同步引擎启用后生效"
            action={<Badge tone="violet">M2</Badge>}
          >
            <div className="flex items-center justify-between py-2.5">
              <div>
                <div className="text-sm text-slate-200">备份保留份数</div>
                <div className="mt-0.5 text-xs text-slate-500">
                  每次写入 Agent 配置前自动备份，超出份数的旧备份会被清理
                </div>
              </div>
              <input
                type="number"
                min={3}
                max={50}
                value={settings.backupRetention}
                onChange={(e) =>
                  void patchSettings({
                    backupRetention: Math.min(50, Math.max(3, Number(e.target.value) || 10)),
                  })
                }
                className="input w-20 text-center"
              />
            </div>
            <div className="border-t border-ink-800">
              <Toggle
                checked
                disabled
                onChange={() => {}}
                label="写入前展示 diff 并要求确认"
                hint="安全不变式，M2 起不可关闭"
              />
            </div>
          </SectionCard>

          {/* 数据与存储 */}
          <SectionCard
            title="数据与存储"
            subtitle="全部数据保存在本机 SQLite 数据库中"
            action={
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={() => host && reveal(host.dataDir)}
                disabled={!host}
              >
                <Icon name="folder" className="h-3.5 w-3.5" />
                打开数据目录
              </button>
            }
          >
            {host ? (
              <div className="divide-y divide-ink-800">
                <PathRow label="数据目录" path={host.dataDir} exists onReveal={reveal} />
                <PathRow label="数据库" path={host.dbPath} exists onReveal={reveal} />
                <PathRow label="用户主目录" path={host.homeDir} exists onReveal={reveal} />
              </div>
            ) : (
              <p className="text-xs text-slate-500">未获取到主机信息</p>
            )}
            <div className="mt-3 flex items-center gap-2 border-t border-ink-800 pt-3">
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={() => void patchSettings({ onboardingDone: false })}
              >
                <Icon name="play" className="h-3.5 w-3.5" />
                重新运行引导
              </button>
              <span className="text-[11px] text-slate-500">
                下次启动（或立即）回到首次设置向导
              </span>
            </div>
          </SectionCard>

          {/* 外观 */}
          <SectionCard title="外观与语言" subtitle="主题与界面语言">
            <Toggle
              checked={settings.theme !== "light"}
              onChange={(next) => void patchSettings({ theme: next ? "dark" : "light" })}
              label="深色主题"
              hint="关闭切换为浅色主题（扁平双色板，即时生效）"
            />
            <div className="border-t border-ink-800">
              <Toggle
                checked={settings.language === "zh-CN"}
                disabled
                onChange={() => {}}
                label="界面语言：简体中文"
                hint="英文界面计划在 M4 提供"
              />
            </div>
          </SectionCard>
        </div>
      </div>

      {/* 换机迁移 */}
      <MigrationCard />

      {/* 关于 */}
      <SectionCard title="关于" subtitle="AgentHub · 统一 Agent 环境管理器">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {[
            { label: "版本", value: host ? `v${host.appVersion}` : "—", icon: "sparkle" as IconName },
            { label: "运行平台", value: host ? `${host.os} · ${host.arch}` : "—", icon: "cpu" as IconName },
            { label: "主机名", value: host?.hostname ?? "—", icon: "link" as IconName },
            { label: "能力目录", value: "31 项全部实现", icon: "shield" as IconName },
          ].map((item) => (
            <div key={item.label} className="rounded-lg border border-ink-800 bg-ink-900 px-3 py-2.5">
              <div className="flex items-center gap-2 text-[11px] uppercase tracking-wide text-slate-500">
                <Icon name={item.icon} className="h-3 w-3" />
                {item.label}
              </div>
              <div className="mt-1 truncate text-sm text-slate-200">{item.value}</div>
            </div>
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

/* -------------------------------------------------------------- 换机迁移 */

function MigrationCard() {
  const setBanner = useApp((s) => s.setBanner);
  const reveal = useReveal();
  const [includeSettings, setIncludeSettings] = useState(true);
  const [busy, setBusy] = useState(false);
  const [lastExport, setLastExport] = useState<MigrationOutcome | null>(null);
  const [bundles, setBundles] = useState<MigrationMeta[]>([]);
  const [importOpts, setImportOpts] = useState<Record<string, { settings: boolean; profiles: boolean; definitions: boolean }>>({});
  const [importing, setImporting] = useState<string | null>(null);

  const refresh = () => {
    api
      .migrationList()
      .then(setBundles)
      .catch(() => setBundles([]));
  };

  useEffect(() => {
    refresh();
  }, []);

  const doExport = async () => {
    setBusy(true);
    try {
      const outcome = await api.migrationExport(includeSettings);
      setLastExport(outcome);
      refresh();
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setBusy(false);
    }
  };

  const doImport = async (bundle: MigrationMeta) => {
    const opts = importOpts[bundle.path] ?? { settings: true, profiles: true, definitions: true };
    setImporting(bundle.path);
    try {
      const summary = await api.migrationImport(
        bundle.path,
        opts.settings && bundle.settingsIncluded,
        opts.profiles,
        opts.definitions,
      );
      setBanner(
        `迁移完成：档案 ${summary.profilesImported} · 定义 ${summary.definitionsImported}` +
          `${summary.settingsApplied ? " · 设置已应用" : ""}` +
          (summary.skipped.length > 0 ? `；跳过 ${summary.skipped.length} 项（${summary.skipped[0]}…）` : ""),
      );
    } catch (error) {
      setBanner(describeError(error));
    } finally {
      setImporting(null);
    }
  };

  return (
    <SectionCard
      title="换机迁移"
      subtitle="一个目录带走全部环境定义：设置 + 环境档案 + 自定义 Agent 定义 —— 密钥与当前用户绑定（DPAPI），永不进包，换机后重新录入"
      action={
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="btn-ghost btn-sm"
            onClick={refresh}
            title="刷新迁移包列表"
          >
            <Icon name="refresh" className="h-3.5 w-3.5" />
          </button>
          <button type="button" className="btn-primary btn-sm" onClick={() => void doExport()} disabled={busy}>
            <Icon name="external" className="h-3.5 w-3.5" />
            {busy ? "导出中…" : "导出迁移包"}
          </button>
        </div>
      }
      bodyClassName="space-y-3"
    >
      <div className="rounded-md border border-ink-800 bg-ink-900 px-3 py-2">
        <Toggle
          checked={includeSettings}
          onChange={setIncludeSettings}
          label="包含设置（扫描范围 / 代理 / 工具链覆盖等）"
          hint="引导标记不带走；目标机的定义目录不会被源机路径覆盖"
        />
      </div>

      {lastExport && (
        <div className="rounded-md border border-brand-800 px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Icon name="check" className="h-4 w-4 text-brand-400" />
            <span className="text-brand-400">
              已导出：{lastExport.profileCount} 个档案 · {lastExport.definitionCount} 个自定义定义
              {lastExport.settingsIncluded ? " · 含设置" : ""}
            </span>
            <button
              type="button"
              className="btn-ghost btn-sm ml-auto"
              onClick={() => reveal(lastExport.path)}
            >
              <Icon name="folder" className="h-3.5 w-3.5" />
              打开目录
            </button>
            <button type="button" className="btn-ghost btn-sm" onClick={() => setLastExport(null)}>
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
          <div className="mono mt-1 truncate text-[10.5px] text-slate-400" title={lastExport.path}>
            {lastExport.path}
          </div>
        </div>
      )}

      {bundles.length === 0 ? (
        <Empty
          icon="external"
          title="还没有迁移包"
          description="「导出迁移包」会把设置、全部环境档案与自定义 Agent 定义打进 exports/migrations/ 下的一个目录；整个目录拷到新机器后在这里导入。"
        />
      ) : (
        <div className="space-y-2">
          {bundles.map((bundle) => {
            const opts = importOpts[bundle.path] ?? { settings: true, profiles: true, definitions: true };
            return (
              <div key={bundle.path} className="rounded-md border border-ink-800 bg-ink-900 px-3 py-2.5">
                <div className="flex flex-wrap items-center gap-2">
                  <Icon name="folder" className="h-4 w-4 shrink-0 text-slate-400" />
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-center gap-2">
                      <span className="text-sm text-slate-200">{bundle.exportedAt} 导出</span>
                      <Badge tone="violet" icon="profiles">
                        档案 {bundle.profileCount}
                      </Badge>
                      {bundle.definitionCount > 0 && (
                        <Badge tone="sky" icon="adapter">
                          定义 {bundle.definitionCount}
                        </Badge>
                      )}
                      {bundle.settingsIncluded && <Badge tone="teal">含设置</Badge>}
                      <Badge tone="slate">v{bundle.appVersion || "?"}</Badge>
                    </span>
                    <span className="mono mt-0.5 block truncate text-[10.5px]" title={bundle.path}>
                      {bundle.path} · {formatBytes(bundle.bytes)}
                    </span>
                  </span>
                  <button
                    type="button"
                    className="btn-primary btn-sm shrink-0"
                    onClick={() => void doImport(bundle)}
                    disabled={importing === bundle.path}
                  >
                    <Icon name="plus" className="h-3.5 w-3.5" />
                    {importing === bundle.path ? "导入中…" : "导入到本机"}
                  </button>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-ink-800 pt-2 text-[11px] text-slate-500">
                  <label className="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={opts.profiles}
                      onChange={(e) =>
                        setImportOpts((prev) => ({
                          ...prev,
                          [bundle.path]: { ...opts, profiles: e.target.checked },
                        }))
                      }
                    />
                    档案（同名自动加后缀）
                  </label>
                  <label className="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={opts.definitions}
                      onChange={(e) =>
                        setImportOpts((prev) => ({
                          ...prev,
                          [bundle.path]: { ...opts, definitions: e.target.checked },
                        }))
                      }
                    />
                    定义（同名跳过不覆盖手改）
                  </label>
                  <label className={`flex items-center gap-1.5 ${bundle.settingsIncluded ? "" : "opacity-40"}`}>
                    <input
                      type="checkbox"
                      checked={opts.settings && bundle.settingsIncluded}
                      disabled={!bundle.settingsIncluded}
                      onChange={(e) =>
                        setImportOpts((prev) => ({
                          ...prev,
                          [bundle.path]: { ...opts, settings: e.target.checked },
                        }))
                      }
                    />
                    设置（保留本机引导与定义目录）
                  </label>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </SectionCard>
  );
}