//! Agent 定义加载器。
//!
//! **内核不含任何 Agent 专属知识**：Agent 的名字、去哪找、用什么证据判定、每条路径
//! 扮演什么角色、允许操作到哪一级，全部来自 TOML 定义文件。
//!
//! 加载顺序（后者覆盖同 id 的前者）：
//!   1. 内置定义（编译进二进制，见 `BUILTIN_FILES`）
//!   2. 用户目录 `%APPDATA%/dev.agenthub.desktop/agents/*.toml`
//!
//! 应用启动时会把内置定义**导出**到用户目录，方便直接编辑；用户文件一旦存在即优先生效。
//! 有语法错误的用户文件会被跳过并记入告警（同时回退到内置定义）。

use crate::capability::{tier_of, RuleParams};
use crate::model::AppSettings;
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/* ------------------------------------------------------------- 结构定义 */
//
// 注意：schema 使用 camelCase（`#[serde(rename_all = "camelCase")]`），因此
// **定义文件里的多词键必须写成 camelCase**（maxTier / skillMethods / baseUrlKey /
// modelsKey / keysOnly …）。除 EvidenceRule（用 flatten 合并能力参数）外，所有结构
// 都设了 deny_unknown_fields：写错的键会直接报错而不是被静默忽略 —— 静默忽略曾在
// 凭据文件上造成「值被读入内存」的真实事故。

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMeta {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    /// cli | ide | extension | host
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub adapter: String,
}

fn default_kind() -> String {
    "cli".to_string()
}

fn default_accent() -> String {
    "#64748b".to_string()
}

/// 一条判定证据：引用一个基础能力 + 参数 + 证据强度
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRule {
    /// 能力 id，见 capability::catalog()
    pub capability: String,
    /// strong | medium | weak
    #[serde(default = "default_strength")]
    pub strength: String,
    #[serde(default)]
    pub label: String,
    #[serde(flatten)]
    pub params: RuleParams,
}

fn default_strength() -> String {
    "medium".to_string()
}

/// 一条路径及其角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathRule {
    pub label: String,
    pub path: String,
    /// config | data | install | skills | mcp | provider
    #[serde(default = "default_role")]
    pub role: String,
    /// json | toml | yaml | conf | md | dir | db
    #[serde(default)]
    pub format: String,
    /// 期望的部署方式：copy | link（用于 role = skills）
    #[serde(default)]
    pub deploy: String,
}

fn default_role() -> String {
    "config".to_string()
}

/// MCP 配置来源：明确告诉内核「在这个文件的这个节点下」
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpSource {
    pub file: String,
    /// 点号分隔的节点路径，例如 "mcp" / "mcpServers" / "mcp.servers"
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub format: String,
    /// 写入策略：merge-keys（结构化节点合并）| managed-block（文本托管块）
    /// 留空时按 format 推导：json → merge-keys，其余 → managed-block
    #[serde(default)]
    pub strategy: String,
    /// managed-block 时使用的标记名（默认取 root）
    #[serde(default)]
    pub marker: String,
    /// 条目形状：standard（command/args/env | url）| opencode（type=local/remote，
    /// command 为数组，环境变量键名为 environment）
    #[serde(default)]
    pub entry_style: String,
}

impl McpSource {
    /// 实际写入策略：定义里显式声明优先，否则按 format 推导
    pub fn effective_strategy(&self) -> &str {
        match self.strategy.as_str() {
            "merge-keys" => "merge-keys",
            "managed-block" => "managed-block",
            _ => {
                if self.format == "json" {
                    "merge-keys"
                } else {
                    "managed-block"
                }
            }
        }
    }

    pub fn effective_style(&self) -> &str {
        if self.entry_style.is_empty() {
            "standard"
        } else {
            self.entry_style.as_str()
        }
    }
}

/// 供应商线索来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderSource {
    pub file: String,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub format: String,
    /// 每个条目输出的线索类型：base-url | model | credential
    #[serde(default = "default_provider_kind")]
    pub kind: String,
    /// 条目内 baseURL 的点号路径
    #[serde(default)]
    pub base_url_key: String,
    /// 条目内模型表的键名（用于统计模型数量）
    #[serde(default)]
    pub models_key: String,
    /// 线索标签前缀，例如 "opencode"
    #[serde(default)]
    pub label: String,
    /// 仅列键名、值永不读取（用于凭据文件）
    #[serde(default)]
    pub keys_only: bool,
}

fn default_provider_kind() -> String {
    "base-url".to_string()
}

/// 该 Agent 被允许操作到哪一级（能力分级授权）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPolicy {
    /// observe | parse | deploy | mutate
    #[serde(default = "default_max_tier")]
    pub max_tier: String,
    /// 允许用于 Skill 的部署方式
    #[serde(default)]
    pub skill_methods: Vec<String>,
    /// 允许的配置写入方式
    #[serde(default)]
    pub config_write: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

fn default_max_tier() -> String {
    "parse".to_string()
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self {
            max_tier: default_max_tier(),
            skill_methods: Vec::new(),
            config_write: Vec::new(),
            notes: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFile {
    pub agent: AgentMeta,
    #[serde(default)]
    pub evidence: Vec<EvidenceRule>,
    #[serde(default)]
    pub paths: Vec<PathRule>,
    #[serde(default)]
    pub mcp: Vec<McpSource>,
    #[serde(default)]
    pub provider: Vec<ProviderSource>,
    /// Provider 写入声明（把供应商资源分发到该 Agent 时用）
    #[serde(default)]
    pub provider_write: Vec<crate::model::ProviderWrite>,
    #[serde(default)]
    pub capabilities: CapabilityPolicy,
}

/* --------------------------------------------------------- 已加载的定义 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedDef {
    pub file: AgentFile,
    /// 内置 / 用户文件路径
    pub source: String,
    pub from_user_dir: bool,
    /// 定义里用到的能力及其级别（供 UI 展示）
    pub used_capabilities: Vec<UsedCapability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedCapability {
    pub id: String,
    pub tier: String,
    pub tier_code: String,
    pub count: usize,
}

pub struct Loaded {
    pub defs: Vec<LoadedDef>,
    pub warnings: Vec<String>,
    pub user_dir: PathBuf,
}

/// 供 GUI 使用的定义总览
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionsView {
    pub definitions: Vec<LoadedDef>,
    pub user_dir: String,
    pub warnings: Vec<String>,
    pub builtin_count: usize,
    pub user_count: usize,
}

pub fn definitions_view(settings: &AppSettings) -> DefinitionsView {
    let loaded = load(settings);
    DefinitionsView {
        builtin_count: loaded.defs.iter().filter(|d| !d.from_user_dir).count(),
        user_count: loaded.defs.iter().filter(|d| d.from_user_dir).count(),
        user_dir: loaded.user_dir.to_string_lossy().to_string(),
        warnings: loaded.warnings,
        definitions: loaded.defs,
    }
}

/* ----------------------------------------------------------- 内置定义源 */

/// 内置定义（编译进二进制）。用户目录中的同名定义会覆盖它。
pub const BUILTIN_FILES: &[(&str, &str)] = &[
    ("opencode", include_str!("../config/agents/opencode.toml")),
    ("ohmypi", include_str!("../config/agents/ohmypi.toml")),
    ("dsh", include_str!("../config/agents/dsh.toml")),
    ("zcode", include_str!("../config/agents/zcode.toml")),
    ("claude-code", include_str!("../config/agents/claude-code.toml")),
    ("codex", include_str!("../config/agents/codex.toml")),
    ("qoder", include_str!("../config/agents/qoder.toml")),
    ("copilot", include_str!("../config/agents/copilot.toml")),
    ("openviking", include_str!("../config/agents/openviking.toml")),
    ("cua", include_str!("../config/agents/cua.toml")),
    ("qmind", include_str!("../config/agents/qmind.toml")),
    ("gemini", include_str!("../config/agents/gemini.toml")),
    ("aider", include_str!("../config/agents/aider.toml")),
    ("crush", include_str!("../config/agents/crush.toml")),
    ("goose", include_str!("../config/agents/goose.toml")),
    ("cline", include_str!("../config/agents/cline.toml")),
    ("roo-code", include_str!("../config/agents/roo-code.toml")),
    ("continue", include_str!("../config/agents/continue.toml")),
    ("cursor", include_str!("../config/agents/cursor.toml")),
    ("windsurf", include_str!("../config/agents/windsurf.toml")),
    ("vscode", include_str!("../config/agents/vscode.toml")),
];

/// 解析定义文件里的路径：支持 `~`、`%VAR%`、`$VAR`，以及相对当前工作目录的 `./`
pub fn resolve_path(raw: &str) -> PathBuf {
    if raw.starts_with("./") || raw.starts_with(".\\") {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(raw.trim_start_matches("./").trim_start_matches(".\\"))
    } else {
        util::expand_buf(raw)
    }
}

/* --------------------------------------------------------------- 加载 */

pub fn user_dir(settings: &AppSettings) -> PathBuf {
    if !settings.definitions_dir.trim().is_empty() {
        util::expand_buf(&settings.definitions_dir)
    } else {
        crate::default_data_dir().join("agents")
    }
}

/// 首次运行时把内置定义导出到用户目录，便于直接编辑。
pub fn seed_user_dir(dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建定义目录失败：{}", e))?;
    let mut written = 0usize;
    for (id, text) in BUILTIN_FILES {
        let target = dir.join(format!("{}.toml", id));
        if !target.exists() {
            std::fs::write(&target, text).map_err(|e| format!("写入 {} 失败：{}", target.display(), e))?;
            written += 1;
        }
    }
    Ok(written)
}

/// 解析单个定义文件（供加载器与将来的编辑器复用）
pub fn parse(text: &str) -> Result<AgentFile, String> {
    let file: AgentFile =
        toml::from_str(text).map_err(|e| format!("TOML 解析失败：{}", e))?;
    validate(&file)?;
    Ok(file)
}

fn validate(file: &AgentFile) -> Result<(), String> {
    if file.agent.id.trim().is_empty() {
        return Err("缺少 agent.id".to_string());
    }
    if !matches!(file.agent.kind.as_str(), "cli" | "ide" | "extension" | "host") {
        return Err(format!(
            "agent.kind 取值非法：{}（应为 cli | ide | extension | host）",
            file.agent.kind
        ));
    }
    for rule in &file.evidence {
        if tier_of(&rule.capability).is_none() {
            return Err(format!(
                "引用了未登记的能力：{}（见能力目录）",
                rule.capability
            ));
        }
        if !matches!(rule.strength.as_str(), "strong" | "medium" | "weak") {
            return Err(format!("evidence.strength 取值非法：{}", rule.strength));
        }
    }
    for path in &file.paths {
        if !matches!(
            path.role.as_str(),
            "config" | "data" | "install" | "skills" | "mcp" | "provider"
        ) {
            return Err(format!("paths.role 取值非法：{}", path.role));
        }
    }
    let policy = &file.capabilities.max_tier;
    if !matches!(policy.as_str(), "observe" | "parse" | "deploy" | "mutate") {
        return Err(format!("capabilities.max_tier 取值非法：{}", policy));
    }
    Ok(())
}

/// 加载全部定义：内置 + 用户目录覆盖
pub fn load(settings: &AppSettings) -> Loaded {
    let mut warnings: Vec<String> = Vec::new();
    let mut map: BTreeMap<String, LoadedDef> = BTreeMap::new();

    for (id, text) in BUILTIN_FILES {
        match parse(text) {
            Ok(file) => {
                map.insert(
                    file.agent.id.clone(),
                    LoadedDef {
                        used_capabilities: used_capabilities(&file),
                        file,
                        source: "内置".to_string(),
                        from_user_dir: false,
                    },
                );
            }
            Err(e) => warnings.push(format!("内置定义 {} 解析失败：{}", id, e)),
        }
    }

    let dir = user_dir(settings);
    if dir.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|it| {
                it.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .map(|x| x.eq_ignore_ascii_case("toml"))
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort();

        for path in entries {
            match std::fs::read_to_string(&path) {
                Ok(text) => match parse(&text) {
                    Ok(file) => {
                        let id = file.agent.id.clone();
                        // 与内置文本逐字对比：导出的副本不算「自定义」，
                        // 只有内容确实被改过才提示覆盖，避免刷屏淹没真正的手改
                        let pristine = BUILTIN_FILES
                            .iter()
                            .find(|(bid, _)| *bid == id)
                            .map(|(_, builtin)| normalize(builtin) == normalize(&text))
                            .unwrap_or(false);
                        let overrides_builtin = map.contains_key(&id);
                        if overrides_builtin && !pristine {
                            warnings.push(format!(
                                "用户定义 {} 已覆盖同 id 的内置定义（内容有改动）",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        let source = if pristine {
                            format!("内置（已导出至 {}）", path.to_string_lossy())
                        } else {
                            path.to_string_lossy().to_string()
                        };
                        map.insert(
                            id,
                            LoadedDef {
                                used_capabilities: used_capabilities(&file),
                                file,
                                source,
                                from_user_dir: !pristine,
                            },
                        );
                    }
                    Err(e) => warnings.push(format!(
                        "用户定义 {} 已跳过：{}（继续使用内置定义）",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        e
                    )),
                },
                Err(e) => warnings.push(format!("无法读取 {}：{}", path.display(), e)),
            }
        }
    }

    Loaded {
        defs: map.into_values().collect(),
        warnings,
        user_dir: dir,
    }
}

/// 归一化文本，用于判断用户文件是否只是内置定义的未改动副本
pub fn normalize(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .trim()
        .to_string()
}

/// 该定义文本是否为「自定义」（相对内置有改动，或内置里根本没有这个 id）
/// 换机迁移导出时用它只带走真正手写/改过的定义，不复制未改动的导出副本
pub fn is_custom_definition(text: &str) -> bool {
    match parse(text) {
        Ok(file) => {
            let id = file.agent.id.clone();
            !BUILTIN_FILES
                .iter()
                .find(|(bid, _)| *bid == id)
                .map(|(_, builtin)| normalize(builtin) == normalize(text))
                .unwrap_or(false)
        }
        Err(_) => false, // 解析失败的文件不迁移（目标机上也会被跳过）
    }
}

fn used_capabilities(file: &AgentFile) -> Vec<UsedCapability> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in &file.evidence {
        *counts.entry(rule.capability.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(id, count)| {
            let tier = tier_of(&id);
            UsedCapability {
                tier_code: tier.map(|t| t.code().to_string()).unwrap_or_default(),
                tier: tier.map(|t| t.id().to_string()).unwrap_or_default(),
                id,
                count,
            }
        })
        .collect()
}

/* ------------------------------------------------------------ 便捷访问 */

impl AgentFile {
    pub fn skill_paths(&self) -> Vec<&PathRule> {
        self.paths.iter().filter(|p| p.role == "skills").collect()
    }

    pub fn install_paths(&self) -> Vec<&PathRule> {
        self.paths
            .iter()
            .filter(|p| p.role == "install" || p.role == "data")
            .collect()
    }
}