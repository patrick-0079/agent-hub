/** 与 Rust `model.rs` 一一对应的类型定义（serde camelCase）。 */

export interface HostInfo {
  os: string;
  osVersion: string;
  arch: string;
  hostname: string;
  username: string;
  homeDir: string;
  appVersion: string;
  dataDir: string;
  dbPath: string;
}

export interface ExecutableInfo {
  name: string;
  displayName: string;
  category: string;
  found: boolean;
  path: string | null;
  version: string | null;
  source: string | null;
  usedBy: string[];
  hint: string | null;
}

export interface ConfigPath {
  label: string;
  path: string;
  kind: string;
  exists: boolean;
  size: number | null;
}

export interface AgentEvidence {
  /** cli | npm | install-dir | config | data | skill-dir */
  signal: string;
  /** strong | medium | weak */
  strength: "strong" | "medium" | "weak";
  label: string;
  value: string;
  found: boolean;
}

export interface AgentTarget {
  id: string;
  name: string;
  vendor: string;
  accent: string;
  /** cli | ide | extension | host */
  kind: "cli" | "ide" | "extension" | "host";
  /** installed | configured | leftover | absent */
  status: "installed" | "configured" | "leftover" | "absent";
  installed: boolean;
  summary: string;
  cli: string | null;
  cliVersion: string | null;
  npmPackage: string | null;
  root: string | null;
  configs: ConfigPath[];
  evidence: AgentEvidence[];
  mcpCount: number;
  skillCount: number;
  adapter: string;
  notes: string[];
}

export interface PythonEnv {
  id: string;
  name: string;
  manager: string;
  path: string;
  pythonVersion: string | null;
  packageCount: number | null;
  active: boolean;
  detail: string | null;
}

export interface NpmPackage {
  name: string;
  version: string;
  manager: string;
  scope: string;
  location: string | null;
  mcpCapable: boolean;
}

export interface McpServerFound {
  id: string;
  name: string;
  sourceAgent: string;
  sourceAgentId: string;
  sourceFile: string;
  transport: string;
  command: string | null;
  args: string[];
  url: string | null;
  envKeys: string[];
  /** http/sse 条目里声明的请求头键名（只读键名，值不进快照） */
  headerKeys: string[];
  raw: unknown;
}

export interface SkillFound {
  id: string;
  name: string;
  path: string;
  dirName: string;
  sourceAgent: string;
  sourceAgentId: string;
  description: string | null;
  whenToUse: string | null;
  hasManifest: boolean;
  fileCount: number;
  bytes: number;
  updatedAt: string | null;
  /** 分类目录名（技能库为 <category>/<skill> 结构时） */
  category: string | null;
  linkKind: string | null;
  linkTarget: string | null;
  broken: boolean;
}

/* ------------------------------------------- T2/T3 动作（导入/清理/删除） */

export interface PlanItem {
  action: string;
  target: string;
  source: string | null;
  detail: string;
  risk: "safe" | "destructive";
}

export interface ActionPlan {
  capability: string;
  tier: string;
  tierCode: string;
  title: string;
  summary: string;
  items: PlanItem[];
  warnings: string[];
  confirmHint: string;
}

export interface StepResult {
  target: string;
  ok: boolean;
  message: string;
}

export interface ActionResult {
  ok: boolean;
  title: string;
  summary: string;
  steps: StepResult[];
  manifest: string | null;
  restoreHint: string;
  warnings: string[];
}

export interface DiscoveredSkill {
  name: string;
  path: string;
  hasManifest: boolean;
  description: string | null;
  fileCount: number;
  bytes: number;
  conflict: boolean;
  isLink: boolean;
}

export interface BrokenRef {
  path: string;
  target: string;
  kind: string;
  owner: string;
  name: string;
}

export interface LibraryInfo {
  path: string;
  exists: boolean;
  skillCount: number;
}

export interface TrashEntry {
  name: string;
  path: string;
  /** 单对象条目为原路径；批量条目为 null */
  original: string | null;
  createdAt: string;
  size: number;
  /** 条目内的对象数量（批量删除会产生多对象条目） */
  itemCount: number;
  summary: string;
  /** link | dir | mixed | unknown */
  kind: string;
}

export interface ManifestInfo {
  path: string;
  op: string;
  createdAt: string;
  summary: string;
  entries: number;
}

/* ------------------------------------------------------------ 回收站管理 */

export interface TrashItemInfo {
  stored: string;
  name: string;
  /** link | dir */
  kind: string;
  original: string;
  linkTarget: string | null;
  size: number;
  /** 原位置是否仍空着（能否恢复） */
  restorable: boolean;
  removedAt: string;
}

export interface TrashDetail {
  entry: TrashEntry;
  items: TrashItemInfo[];
}

export interface TrashStats {
  entries: number;
  items: number;
  bytes: number;
  oldest: string | null;
  newest: string | null;
  dir: string;
}

/* ------------------------------------------- MCP 资源库与配置分发（T2） */

export interface EnvPair {
  key: string;
  value: string;
}

export interface McpResource {
  id: number;
  name: string;
  /** stdio | http | sse */
  transport: string;
  command: string;
  args: string[];
  env: EnvPair[];
  /** http/sse 请求头（值支持 %VAR% / $VAR 引用） */
  headers: EnvPair[];
  url: string;
  enabled: boolean;
  notes: string;
  /** 最近一次握手健康检查的结果（未测试时字段为空/零） */
  health: McpHealth;
}

/** ok | timeout | error；空 = 未测试 */
export type McpHealthStatus = "ok" | "timeout" | "error" | "";

export interface McpHealth {
  status: McpHealthStatus;
  latencyMs: number;
  protocolVersion: string | null;
  serverName: string | null;
  tools: number | null;
  message: string;
  testedAt: string;
}

/** 一次握手测试的返回（含服务器标识） */
export interface McpHandshakeResult extends McpHealth {
  serverId: number;
  name: string;
  transport: string;
}

export interface MergeChange {
  /** add | update | remove | unchanged */
  kind: string;
  key: string;
  detail: string;
}

export interface SyncTargetPlan {
  agentId: string;
  agentName: string;
  /** config（配置文件写入）| skill（Skill 部署） */
  kind: string;
  file: string;
  root: string;
  format: string;
  strategy: string;
  supported: boolean;
  reason: string | null;
  fileExists: boolean;
  backupPath: string | null;
  changes: MergeChange[];
  diff: string;
  added: number;
  updated: number;
  removed: number;
  /** 同名但非受管、默认跳过的条目数 */
  skipped: number;
  unchanged: number;
}

export interface SyncPlan {
  capability: string;
  tier: string;
  tierCode: string;
  title: string;
  summary: string;
  servers: string[];
  targets: SyncTargetPlan[];
  warnings: string[];
  confirmHint: string;
}

export interface BackupInfo {
  id: number;
  target: string;
  backupPath: string;
  createdAt: string;
  bytes: number;
  note: string;
}

/* ------------------------------------------------- Provider 资源与保险库 */

export interface ProviderResource {
  id: number;
  name: string;
  kind: string;
  baseUrl: string;
  models: string[];
  keyRef: string;
  hasKey: boolean;
  maskedKey: string | null;
  enabled: boolean;
  notes: string;
  /** 最近一次连通性测试结果（未测试时字段为空/零） */
  health: ProviderHealth;
  /** 最近一次余额查询结果（未查询时字段为空/零） */
  balance: ProviderBalanceState;
}

/** ok | no_key | error；空串 = 未测试 */
export type ProviderHealthStatus = "ok" | "no_key" | "error" | "";

export interface ProviderHealth {
  status: ProviderHealthStatus;
  httpStatus: number | null;
  latencyMs: number;
  models: number | null;
  message: string;
  testedAt: string;
  endpoint: string;
}

/** 一次连通性测试的返回（含供应商标识） */
export interface ProviderTestResult extends ProviderHealth {
  providerId: number;
  providerName: string;
}

/** ok | no_key | error | unsupported；空串 = 未查询 */
export type ProviderBalanceStatus = "ok" | "no_key" | "error" | "unsupported" | "";

export interface ProviderBalanceState {
  status: ProviderBalanceStatus;
  httpStatus: number | null;
  latencyMs: number;
  isAvailable: boolean | null;
  currency: string;
  totalBalance: string;
  grantedBalance: string;
  toppedUpBalance: string;
  message: string;
  testedAt: string;
  endpoint: string;
}

/** 一次余额查询的返回（含供应商标识） */
export interface ProviderBalanceResult extends ProviderBalanceState {
  providerId: number;
  providerName: string;
}

export interface VaultStatus {
  path: string;
  count: number;
  healthy: boolean;
  message: string;
}

/* ------------------------------------------------------------ Profile 档案 */

export interface ProfileCounts {
  mcp: number;
  provider: number;
  skill: number;
}

export interface ProfileResource {
  id: number;
  name: string;
  description: string;
  /** 绑定的 Agent id */
  agents: string[];
  counts: ProfileCounts;
  updatedAt: string;
}

export interface ProfileItem {
  id: number;
  /** mcp | provider | skill */
  resourceType: string;
  /** MCP/供应商用名称；Skill 用路径 */
  resourceRef: string;
  display: string;
}

export interface ProfileDetail {
  profile: ProfileResource;
  items: ProfileItem[];
}

export interface SkillEnv {
  libraries: LibraryInfo[];
  git: string | null;
  trash: TrashEntry[];
  trashDir: string;
  manifestDir: string;
  tmpDir: string;
  proxy: string;
}

export interface CloneOutcome {
  result: ActionResult;
  path: string | null;
}

export interface ProviderHint {
  id: string;
  label: string;
  kind: string;
  valueMasked: string;
  source: string;
  envVar: string | null;
}

export interface ScanSnapshot {
  scannedAt: string;
  durationMs: number;
  host: HostInfo;
  executables: ExecutableInfo[];
  agents: AgentTarget[];
  pythonEnvs: PythonEnv[];
  npmPackages: NpmPackage[];
  mcpServers: McpServerFound[];
  skills: SkillFound[];
  providerHints: ProviderHint[];
  warnings: string[];
}

export interface SnapshotMeta {
  id: number;
  scannedAt: string;
  durationMs: number;
  agents: number;
  skills: number;
  mcpServers: number;
  pythonEnvs: number;
  npmPackages: number;
}

/* ------------------------------------------------- 快照对比（M1） */

export interface SnapshotDiffEntry {
  key: string;
  label: string;
  detail: string;
}

export interface SnapshotDiffSection {
  /** agents | skills | mcp | providers | python | npm */
  resource: string;
  title: string;
  added: SnapshotDiffEntry[];
  removed: SnapshotDiffEntry[];
  changed: SnapshotDiffEntry[];
}

export interface SnapshotDiff {
  aId: number;
  aAt: string;
  bId: number;
  bAt: string;
  sections: SnapshotDiffSection[];
  summary: string;
}

/* ------------------------------------------------- 同步审计时间线（M2） */

export interface SyncHistoryEntry {
  id: number;
  /** 目标文件路径 */
  target: string;
  /** 变更摘要（如 "claude-code：+2 ~1 -0"） */
  summary: string;
  /** 写入前的备份路径（可回滚） */
  backupPath: string | null;
  /** ok | error */
  status: string;
  /** gui | cli */
  actor: string;
  createdAt: string;
}

/* ------------------------------------------------- Python 环境创建（M3） */

export interface EnvCreatePlan {
  tier: string;
  tierCode: string;
  command: string;
  args: string[];
  target: string;
  targetExists: boolean;
  pythonNote: string;
  uvFound: boolean;
  message: string;
}

/* ------------------------------------------------- 档案导出/导入（M1） */

export interface ProfileExportMeta {
  path: string;
  name: string;
  description: string;
  items: number;
  skillItems: number;
  mcpItems: number;
  providerItems: number;
  agents: number;
  exportedAt: string;
  bytes: number;
}

export interface ProfileExportOutcome {
  path: string;
  name: string;
  items: number;
  exportsDir: string;
}

export interface Progress {
  phase: string;
  message: string;
  current: number;
  total: number;
  level: "info" | "warn" | "error" | "done";
  ts: string;
}

export interface AppSettings {
  onboardingDone: boolean;
  theme: string;
  language: string;
  backupRetention: number;
  extraScanRoots: string[];
  extraSkillRoots: string[];
  executableOverrides: Record<string, string>;
  scanHomeDepth: number;
  autoScanOnStart: boolean;
  /** Agent 定义目录（留空 = 数据目录下的 agents/） */
  definitionsDir: string;
  /** 技能库目录（导入目标 / 删除范围白名单） */
  skillLibraries: string[];
  /** 网络代理（git 克隆等子进程使用） */
  networkProxy: string;
}

export interface TextPreview {
  path: string;
  exists: boolean;
  bytes: number;
  truncated: boolean;
  text: string;
  error: string | null;
}

/* ------------------------------------------- Agent 定义（配置驱动的 schema） */

export type CapabilityTier = "observe" | "parse" | "deploy" | "mutate";

export interface CapabilitySpec {
  id: string;
  tier: CapabilityTier;
  tierCode: string;
  label: string;
  description: string;
  /** M0 是否已实现（未实现的仅登记级别与参数） */
  implemented: boolean;
  params: string[];
}

export interface EvidenceRule {
  capability: string;
  strength: "strong" | "medium" | "weak";
  label: string;
  args: string[];
  paths: string[];
  format: string | null;
  root: string | null;
  key: string | null;
}

export interface PathRule {
  label: string;
  path: string;
  /** config | data | install | skills | mcp | provider */
  role: string;
  format: string;
  /** copy | link */
  deploy: string;
}

export interface McpSource {
  file: string;
  root: string;
  format: string;
  /** merge-keys | managed-block（留空按 format 推导） */
  strategy: string;
  marker: string;
  /** standard | opencode */
  entryStyle: string;
}

export interface ProviderSource {
  file: string;
  root: string;
  format: string;
  kind: string;
  baseUrlKey: string;
  modelsKey: string;
  label: string;
  keysOnly: boolean;
}

export interface CapabilityPolicy {
  maxTier: CapabilityTier;
  skillMethods: string[];
  configWrite: string[];
  notes: string;
}

export interface AgentMeta {
  id: string;
  name: string;
  vendor: string;
  kind: string;
  accent: string;
  note: string;
  adapter: string;
}

export interface AgentFile {
  agent: AgentMeta;
  evidence: EvidenceRule[];
  paths: PathRule[];
  mcp: McpSource[];
  provider: ProviderSource[];
  capabilities: CapabilityPolicy;
}

export interface UsedCapability {
  id: string;
  tier: string;
  tierCode: string;
  count: number;
}

export interface LoadedDef {
  file: AgentFile;
  /** "内置" 或用户定义文件路径 */
  source: string;
  fromUserDir: boolean;
  usedCapabilities: UsedCapability[];
}

export interface DefinitionsView {
  definitions: LoadedDef[];
  userDir: string;
  warnings: string[];
  builtinCount: number;
  userCount: number;
}

export interface DirEntry {
  name: string;
  isDir: boolean;
  size: number;
}

export type Route =
  | "dashboard"
  | "providers"
  | "skills"
  | "mcp"
  | "npm"
  | "python"
  | "profiles"
  | "agents"
  | "adapter"
  | "history"
  | "trash"
  | "vault"
  | "settings";