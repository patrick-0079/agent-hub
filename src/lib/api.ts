/** Tauri 后端调用封装。浏览器直开时给出明确的降级提示，而不是静默失败。 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ActionPlan,
  ActionResult,
  AppSettings,
  BackupInfo,
  BrokenRef,
  CapabilitySpec,
  CloneOutcome,
  ConfigNode,
  DefinitionsView,
  DirEntry,
  DiscoveredSkill,
  DraftOutcome,
  DraftRequest,
  EnvCreatePlan,
  ExecutableInfo,
  HostInfo,
  ManifestInfo,
  McpHandshakeResult,
  McpResource,
  MigrationImportSummary,
  MigrationMeta,
  MigrationOutcome,
  NpmInstallPlan,
  NpmOutdated,
  ProfileDetail,
  ProfileExportMeta,
  ProfileExportOutcome,
  ProfileItem,
  ProfileResource,
  Progress,
  ProviderBalanceResult,
  ProviderResource,
  ProviderTestResult,
  PythonEnv,
  ScanSnapshot,
  SkillEnv,
  SnapshotDiff,
  SnapshotMeta,
  SyncHistoryEntry,
  SyncPlan,
  TemplateRenderResult,
  TextPreview,
  TrashDetail,
  TrashEntry,
  TrashStats,
  VaultKeyInfo,
  VaultStatus,
} from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in (window as unknown as object);

export class BackendUnavailable extends Error {}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    throw new BackendUnavailable("浏览器预览模式：未连接 Tauri 后端，请通过 `pnpm app:dev` 启动。");
  }
  return invoke<T>(cmd, args);
}

export const api = {
  appInfo: () => call<HostInfo>("app_info"),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  resetOnboarding: () => call<AppSettings>("reset_onboarding"),
  detectExecutables: () => call<ExecutableInfo[]>("detect_executables"),
  runScan: () => call<ScanSnapshot>("run_scan"),
  lastSnapshot: () => call<ScanSnapshot | null>("last_snapshot"),
  snapshotHistory: (limit = 30) => call<SnapshotMeta[]>("snapshot_history", { limit }),
  readTextPreview: (path: string, maxBytes?: number) =>
    call<TextPreview>("read_text_preview", { path, maxBytes }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  listDir: (path: string, limit?: number) => call<DirEntry[]>("list_dir", { path, limit }),
  frontendReady: () => call<void>("frontend_ready"),

  // 内核能力目录与 Agent 定义（配置驱动）
  capabilityCatalog: () => call<CapabilitySpec[]>("capability_catalog"),
  agentDefinitions: () => call<DefinitionsView>("agent_definitions"),
  saveAgentDefinition: (id: string, content: string) =>
    call<DefinitionsView>("save_agent_definition", { id, content }),
  resetAgentDefinition: (id: string) =>
    call<DefinitionsView>("reset_agent_definition", { id }),
  seedAgentDefinitions: () => call<number>("seed_agent_definitions"),

  // T2/T3 动作：Skill 导入 / 清理 / 重建 / 删除
  skillEnvironment: () => call<SkillEnv>("skill_environment"),
  skillDiscover: (source: string, library?: string) =>
    call<DiscoveredSkill[]>("skill_discover", { source, library: library ?? null }),
  skillImportPlan: (source: string, library: string | null, mode: string, names: string[]) =>
    call<ActionPlan>("skill_import_plan", { source, library, mode, names }),
  skillImportApply: (source: string, library: string | null, mode: string, names: string[]) =>
    call<ActionResult>("skill_import_apply", { source, library, mode, names }),
  gitCloneRepo: (url: string, proxy?: string) =>
    call<CloneOutcome>("git_clone_repo", { url, proxy: proxy ?? null }),
  tmpCleanup: (path: string) => call<ActionResult>("tmp_cleanup", { path }),
  skillCleanupPlan: (broken: BrokenRef[]) => call<ActionPlan>("skill_cleanup_plan", { broken }),
  skillCleanupApply: (broken: BrokenRef[]) => call<ActionResult>("skill_cleanup_apply", { broken }),
  skillRelinkPlan: (broken: BrokenRef[], newRoot: string) =>
    call<ActionPlan>("skill_relink_plan", { broken, newRoot }),
  skillRelinkApply: (broken: BrokenRef[], newRoot: string) =>
    call<ActionResult>("skill_relink_apply", { broken, newRoot }),
  skillDeletePlan: (path: string, linkImpact: number) =>
    call<ActionPlan>("skill_delete_plan", { path, linkImpact }),
  skillDeleteApply: (path: string) => call<ActionResult>("skill_delete_apply", { path }),
  trashList: () => call<TrashEntry[]>("trash_list"),
  trashRestore: (name: string) => call<ActionResult>("trash_restore", { name }),
  trashStats: () => call<TrashStats>("trash_stats"),
  trashDetail: (name: string) => call<TrashDetail>("trash_detail", { name }),
  trashRestoreItems: (name: string, stored: string[]) =>
    call<ActionResult>("trash_restore_items", { name, stored }),
  trashPurgePlan: (names: string[]) => call<ActionPlan>("trash_purge_plan", { names }),
  trashPurgeApply: (names: string[]) => call<ActionResult>("trash_purge_apply", { names }),
  trashPurgeOlderPlan: (days: number) => call<ActionPlan>("trash_purge_older_plan", { days }),
  trashPurgeOlderApply: (days: number) => call<ActionResult>("trash_purge_older_apply", { days }),

  // MCP 资源库与配置分发（T2）
  mcpResources: () => call<McpResource[]>("mcp_resources"),
  mcpSave: (resource: McpResource) => call<McpResource[]>("mcp_save", { resource }),
  mcpRemove: (id: number) => call<McpResource[]>("mcp_remove", { id }),
  mcpImport: (items: McpResource[]) => call<McpResource[]>("mcp_import", { items }),
  mcpTest: (id: number) => call<McpHandshakeResult>("mcp_test", { id }),
  mcpSyncPlan: (agentIds: string[], overwriteUnmanaged = false) =>
    call<SyncPlan>("mcp_sync_plan", { agentIds, overwriteUnmanaged }),
  mcpSyncApply: (agentIds: string[], overwriteUnmanaged = false) =>
    call<ActionResult>("mcp_sync_apply", { agentIds, overwriteUnmanaged }),
  backupsList: (limit = 50) => call<BackupInfo[]>("backups_list", { limit }),

  // Profile 环境档案
  profileList: () => call<ProfileResource[]>("profile_list"),
  profileDetail: (id: number) => call<ProfileDetail | null>("profile_detail", { id }),
  profileSave: (profile: ProfileResource, items: ProfileItem[]) =>
    call<ProfileResource[]>("profile_save", { profile, items }),
  profileDelete: (id: number) => call<ProfileResource[]>("profile_delete", { id }),
  profileApplyPlan: (profileId: number, agentIds: string[], overwriteUnmanaged = false) =>
    call<SyncPlan>("profile_apply_plan", { profileId, agentIds, overwriteUnmanaged }),
  profileApplyRun: (profileId: number, agentIds: string[], overwriteUnmanaged = false) =>
    call<ActionResult>("profile_apply_run", { profileId, agentIds, overwriteUnmanaged }),
  backupRestore: (id: number) => call<ActionResult>("backup_restore", { id }),

  // 供应商资源库与保险库（T2 + DPAPI）
  providerResources: () => call<ProviderResource[]>("provider_resources"),
  providerSave: (resource: ProviderResource, apiKey?: string | null) =>
    call<ProviderResource[]>("provider_save", { resource, apiKey: apiKey ?? null }),
  providerRemove: (id: number, removeKey = true) =>
    call<ProviderResource[]>("provider_remove", { id, removeKey }),
  providerImport: (items: ProviderResource[]) =>
    call<ProviderResource[]>("provider_import", { items }),
  providerRevealKey: (id: number) => call<string>("provider_reveal_key", { id }),
  vaultStatus: () => call<VaultStatus>("vault_status"),
  vaultKeys: () => call<VaultKeyInfo[]>("vault_keys"),
  vaultKeySet: (id: string, secret: string) =>
    call<VaultKeyInfo[]>("vault_key_set", { id, secret }),
  vaultKeyRemove: (id: string) => call<VaultKeyInfo[]>("vault_key_remove", { id }),
  vaultReveal: (id: string) => call<string>("vault_reveal", { id }),
  providerTest: (id: number) => call<ProviderTestResult>("provider_test", { id }),
  providerBalanceQuery: (id: number) =>
    call<ProviderBalanceResult>("provider_balance_query", { id }),
  snapshotDiff: (idA: number, idB: number) =>
    call<SnapshotDiff>("snapshot_diff", { idA, idB }),
  syncHistory: (limit = 50) => call<SyncHistoryEntry[]>("sync_history", { limit }),
  pythonEnvCreatePlan: (path: string, python: string | null, manager: string | null) =>
    call<EnvCreatePlan>("python_env_create_plan", { path, python, manager }),
  pythonEnvCreateRun: (path: string, python: string | null, manager: string | null) =>
    call<ActionResult>("python_env_create_run", { path, python, manager }),
  pythonEnvManaged: () => call<PythonEnv[]>("python_env_managed"),
  pythonEnvRemove: (path: string) => call<ActionResult>("python_env_remove", { path }),
  npmInstallPlan: (manager: string, packages: string[]) =>
    call<NpmInstallPlan>("npm_install_plan", { manager, packages }),
  npmInstallRun: (manager: string, packages: string[]) =>
    call<ActionResult>("npm_install_run", { manager, packages }),
  npmRemovePlan: (manager: string, pkg: string) =>
    call<NpmInstallPlan>("npm_remove_plan", { manager, package: pkg }),
  npmRemoveRun: (manager: string, pkg: string) =>
    call<ActionResult>("npm_remove_run", { manager, package: pkg }),
  npmOutdated: (manager: string) => call<NpmOutdated[]>("npm_outdated", { manager }),
  templateRender: (template: string, contextJson: string) =>
    call<TemplateRenderResult>("template_render", { template, contextJson }),
  migrationExport: (includeSettings = true) =>
    call<MigrationOutcome>("migration_export", { includeSettings }),
  migrationList: () => call<MigrationMeta[]>("migration_list"),
  migrationImport: (
    path: string,
    includeSettings: boolean,
    includeProfiles: boolean,
    includeDefinitions: boolean,
  ) =>
    call<MigrationImportSummary>("migration_import", {
      path,
      includeSettings,
      includeProfiles,
      includeDefinitions,
    }),
  configTree: (path: string) => call<ConfigNode>("config_tree", { path }),
  definitionDraft: (req: DraftRequest) => call<DraftOutcome>("definition_draft", { req }),
  profileExport: (id: number) => call<ProfileExportOutcome>("profile_export", { id }),
  profileExportList: () => call<ProfileExportMeta[]>("profile_export_list"),
  profileImport: (path: string) => call<ProfileDetail>("profile_import", { path }),
  providerSyncPlan: (agentIds: string[], overwriteUnmanaged = false) =>
    call<SyncPlan>("provider_sync_plan", { agentIds, overwriteUnmanaged }),
  providerSyncApply: (agentIds: string[], overwriteUnmanaged = false) =>
    call<ActionResult>("provider_sync_apply", { agentIds, overwriteUnmanaged }),
  manifestsList: (limit = 30) => call<ManifestInfo[]>("manifests_list", { limit }),
  manifestRestore: (path: string) => call<ActionResult>("manifest_restore", { path }),
};

/** 订阅扫描进度事件，返回取消订阅函数。 */
export function onScanProgress(handler: (p: Progress) => void): () => void {
  if (!isTauri) return () => {};
  const pending = listen<Progress>("scan:progress", (event) => handler(event.payload));
  return () => {
    pending.then((unlisten) => unlisten()).catch(() => {});
  };
}

export function describeError(error: unknown): string {
  if (error instanceof BackendUnavailable) return error.message;
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}