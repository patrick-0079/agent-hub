//! 数据模型：AgentHub 的统一资源表示。
//!
//! 所有与前端交互的结构体使用 camelCase 序列化，前端 TS 类型与之一一对应。
//! M0 阶段真实产出的是 `ScanSnapshot`（只读侦察结果）；资源表结构（Provider/Profile/
//! SyncHistory 等）已在 store 中预置，M1/M2 直接落库。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/* ------------------------------------------------------------------ 主机信息 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub hostname: String,
    pub username: String,
    pub home_dir: String,
    pub app_version: String,
    pub data_dir: String,
    pub db_path: String,
}

/* ------------------------------------------------------------ 可执行文件探测 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableInfo {
    /// 命令名（node / uv / conda …）
    pub name: String,
    pub display_name: String,
    /// runtime | node-pm | python | vcs | container | editor
    pub category: String,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// PATH | known-location | user-override
    pub source: Option<String>,
    /// 哪些能力依赖它（用于 UI 展示影响面）
    pub used_by: Vec<String>,
    /// 缺失时的安装建议
    pub hint: Option<String>,
}

/* ---------------------------------------------------------------- Agent 目标 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPath {
    pub label: String,
    pub path: String,
    pub kind: String,
    pub exists: bool,
    pub size: Option<u64>,
}

/// 单条安装证据：为什么判定它装了 / 没装。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvidence {
    /// cli | npm | install-dir | config | data | skill-dir
    pub signal: String,
    /// strong | medium | weak
    pub strength: String,
    pub label: String,
    /// 路径、包名（含版本）等具体值
    pub value: String,
    pub found: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentTarget {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub accent: String,
    /// cli | ide | extension | host
    pub kind: String,
    /// installed | configured | leftover | absent
    pub status: String,
    /// status == "installed"（便于计数）
    pub installed: bool,
    /// 一句话结论
    pub summary: String,
    /// 解析到的命令行程序路径
    pub cli: Option<String>,
    pub cli_version: Option<String>,
    /// 命中的 npm 全局包（含版本）
    pub npm_package: Option<String>,
    /// 配置文件所在根目录（首个存在的目录）
    pub root: Option<String>,
    pub configs: Vec<ConfigPath>,
    pub evidence: Vec<AgentEvidence>,
    pub mcp_count: usize,
    pub skill_count: usize,
    /// 是否已有内置同步适配器（M2 起可用）
    pub adapter: String,
    pub notes: Vec<String>,
}

/* --------------------------------------------------------------- Python 环境 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PythonEnv {
    pub id: String,
    pub name: String,
    /// conda | uv | venv
    pub manager: String,
    pub path: String,
    pub python_version: Option<String>,
    pub package_count: Option<usize>,
    pub active: bool,
    pub detail: Option<String>,
}

/* --------------------------------------------------------------- npm 全局包 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NpmPackage {
    pub name: String,
    pub version: String,
    /// npm | pnpm
    pub manager: String,
    pub scope: String,
    pub location: Option<String>,
    /// 是否可作为 MCP 服务器启动（npx/uvx 型）
    pub mcp_capable: bool,
}

/* ------------------------------------------------------------------- MCP */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServerFound {
    pub id: String,
    pub name: String,
    /// 来自哪个 Agent 目标
    pub source_agent: String,
    pub source_agent_id: String,
    pub source_file: String,
    /// stdio | http | sse | unknown
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_keys: Vec<String>,
    pub raw: serde_json::Value,
}

/* ------------------------------------------------------------------ Skill */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillFound {
    pub id: String,
    pub name: String,
    pub path: String,
    /// 目录名（用于展示来源布局）
    pub dir_name: String,
    pub source_agent: String,
    pub source_agent_id: String,
    pub description: Option<String>,
    pub when_to_use: Option<String>,
    pub has_manifest: bool,
    pub file_count: usize,
    pub bytes: u64,
    pub updated_at: Option<String>,
    /// 分类目录名（技能库采用 `<category>/<skill>` 结构时）
    pub category: Option<String>,
    /// 该 Skill 是否为符号链接 / junction 部署
    pub link_kind: Option<String>,
    pub link_target: Option<String>,
    /// 链接指向的目标不存在（环境已损坏，需要修复）
    pub broken: bool,
}

/* ------------------------------------------------------------- 供应商线索 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHint {
    pub id: String,
    pub label: String,
    /// base-url | api-key | model | credential
    pub kind: String,
    pub value_masked: String,
    pub source: String,
    pub env_var: Option<String>,
}

/* --------------------------------------------------------------- 扫描快照 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanSnapshot {
    pub scanned_at: String,
    pub duration_ms: u64,
    pub host: HostInfo,
    pub executables: Vec<ExecutableInfo>,
    pub agents: Vec<AgentTarget>,
    pub python_envs: Vec<PythonEnv>,
    pub npm_packages: Vec<NpmPackage>,
    pub mcp_servers: Vec<McpServerFound>,
    pub skills: Vec<SkillFound>,
    pub provider_hints: Vec<ProviderHint>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: i64,
    pub scanned_at: String,
    pub duration_ms: u64,
    pub agents: usize,
    pub skills: usize,
    pub mcp_servers: usize,
    pub python_envs: usize,
    pub npm_packages: usize,
}

/* ---------------------------------------------------------------- 扫描进度 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
    /// info | warn | error | done
    pub level: String,
    pub ts: String,
}

/* ------------------------------------------------------------------ 设置 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub onboarding_done: bool,
    pub theme: String,
    pub language: String,
    /// 同步备份保留份数（M2 生效）
    pub backup_retention: u32,
    /// 额外的虚拟环境搜索根目录
    pub extra_scan_roots: Vec<String>,
    /// 额外的 Skill 存放目录（自定义 Agent 用）
    pub extra_skill_roots: Vec<String>,
    /// 手动指定的可执行文件路径覆盖（name -> path）
    pub executable_overrides: HashMap<String, String>,
    /// 扫描用户目录的最大深度
    pub scan_home_depth: u32,
    /// 启动时自动扫描
    pub auto_scan_on_start: bool,
    /// Agent 定义目录（留空 = 数据目录下的 agents/）
    pub definitions_dir: String,
    /// 技能库目录（导入目标 / 删除范围白名单；留空 = ~/.agent/skills 与 ~/.agents/skills）
    pub skill_libraries: Vec<String>,
    /// 网络代理（用于 git clone 等；留空表示直连）
    pub network_proxy: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            onboarding_done: false,
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            backup_retention: 10,
            extra_scan_roots: Vec::new(),
            extra_skill_roots: Vec::new(),
            executable_overrides: HashMap::new(),
            scan_home_depth: 3,
            auto_scan_on_start: true,
            definitions_dir: String::new(),
            skill_libraries: Vec::new(),
            network_proxy: "http://127.0.0.1:7897".to_string(),
        }
    }
}

/* ------------------------------------------------------------ MCP 资源 */

/// MCP 环境变量的键值对（值可以是 `%VAR%` / `$VAR` 形式的环境变量引用，不落明文密钥）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvPair {
    pub key: String,
    pub value: String,
}

/// 受管的 MCP 服务器资源（单一事实源）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub id: i64,
    pub name: String,
    /// stdio | http | sse
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<EnvPair>,
    pub url: String,
    pub enabled: bool,
    pub notes: String,
}

/* --------------------------------------------------------- Provider 资源 */

/// 受管的模型供应商资源（单一事实源）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResource {
    pub id: i64,
    pub name: String,
    /// openai-compatible | anthropic | ollama | azure | openrouter …
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
    /// 保险库里的密钥标识（值本身永不进数据库）
    pub key_ref: String,
    /// 是否已在保险库中存有密钥
    pub has_key: bool,
    /// 掩码展示（永不返回明文）
    pub masked_key: Option<String>,
    pub enabled: bool,
    pub notes: String,
    /// 最近一次连通性测试的结果（未测试时为默认值）
    #[serde(default)]
    pub health: ProviderHealth,
}

/// 供应商连通性测试结果（存 provider.health 列）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    /// ok | no_key | error |（空 = 未测试）
    pub status: String,
    pub http_status: Option<u16>,
    pub latency_ms: u64,
    pub models: Option<usize>,
    pub message: String,
    pub tested_at: String,
    pub endpoint: String,
}

/// Agent 定义里的 Provider 写入声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderWrite {
    pub file: String,
    /// 写入节点（点号路径）
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub format: String,
    /// merge-keys | managed-block
    #[serde(default)]
    pub strategy: String,
    /// true：每个 provider 写成 root 下的一个对象（键名取资源名）
    /// false：把 entries 平铺写入 root
    #[serde(default)]
    pub object_per_provider: bool,
    /// 字段映射：key 为目标键（可含点号，如 options.baseURL），from 为来源
    /// 来源取值：baseUrl | apiKey | name | models
    #[serde(default)]
    pub entries: Vec<ProviderEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderEntry {
    pub key: String,
    /// baseUrl | apiKey | name | models
    pub from: String,
}

/// 一次键级变更
/* ---------------------------------------------------------- Profile 档案 */

/// 档案里的资源构成
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCounts {
    pub mcp: usize,
    pub provider: usize,
    pub skill: usize,
}

/// 环境档案：把「哪套 MCP + 哪个供应商 + 哪些 Skill」打包
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResource {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// 绑定的 Agent id
    pub agents: Vec<String>,
    pub counts: ProfileCounts,
    pub updated_at: String,
}

/// 档案中的一项资源
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItem {
    pub id: i64,
    /// mcp | provider | skill
    pub resource_type: String,
    /// MCP/供应商 用名称；Skill 用其路径
    pub resource_ref: String,
    /// 展示名（Skill 用目录名，便于界面显示）
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetail {
    pub profile: ProfileResource,
    pub items: Vec<ProfileItem>,
}

/* ------------------------------------------------------ 配置合并与同步 */

fn default_target_kind() -> String {
    "config".to_string()
}

/// 一次键级变更
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MergeChange {
    /// add | update | remove | skipped | unchanged
    pub kind: String,
    pub key: String,
    pub detail: String,
}

/// 单个目标文件的同步计划
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncTargetPlan {
    pub agent_id: String,
    pub agent_name: String,
    /// config（配置文件写入）| skill（Skill 部署）
    #[serde(default = "default_target_kind")]
    pub kind: String,
    pub file: String,
    pub root: String,
    pub format: String,
    /// merge-keys | managed-block
    pub strategy: String,
    /// 该目标是否支持写入（格式不支持时为 false）
    pub supported: bool,
    pub reason: Option<String>,
    pub file_exists: bool,
    pub backup_path: Option<String>,
    pub changes: Vec<MergeChange>,
    pub diff: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// 同名但非受管、默认跳过的条目数
    pub skipped: usize,
    pub unchanged: usize,
}

/// 一次分发的整体计划（界面上先看这个，再确认）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub capability: String,
    pub tier: String,
    pub tier_code: String,
    pub title: String,
    pub summary: String,
    pub servers: Vec<String>,
    pub targets: Vec<SyncTargetPlan>,
    pub warnings: Vec<String>,
    pub confirm_hint: String,
}

/// 备份索引条目
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: i64,
    pub target: String,
    pub backup_path: String,
    pub created_at: String,
    pub bytes: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TextPreview {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub truncated: bool,
    pub text: String,
    pub error: Option<String>,
}