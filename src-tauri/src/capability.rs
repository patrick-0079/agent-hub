//! 内核基础能力（Capability）—— AgentHub 只提供原语，不含任何 Agent 专属逻辑。
//!
//! 能力分四级，级别决定「谁来做决定」：
//!
//! | 级别 | 名称 | 副作用 | 授权方式 |
//! |---|---|---|---|
//! | T0 | 观察 Observe | 无（只读元数据） | 扫描时自动执行 |
//! | T1 | 解析 Parse | 无（只读内容） | 扫描时自动执行 |
//! | T2 | 部署 Deploy | 写磁盘（可备份回滚） | 用户显式确认 |
//! | T3 | 变更 Mutate | 调外部程序改系统状态 | 用户显式确认 + 二次警告 |
//!
//! Agent 定义文件（config/agents/*.toml）通过 `capability = "..."` 引用这些原语，
//! 内核按声明求值。M0 实现 T0/T1（探测所需）；T2/T3 已登记级别与参数，留待 M2/M3。

use crate::model::{AppSettings, NpmPackage};
use crate::util;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// T0 观察
    Observe,
    /// T1 解析
    Parse,
    /// T2 部署
    Deploy,
    /// T3 变更
    Mutate,
}

impl Tier {
    /// 由级别 id 还原（observe / parse / deploy / mutate）
    pub fn from_id(id: &str) -> Option<Tier> {
        match id {
            "observe" => Some(Tier::Observe),
            "parse" => Some(Tier::Parse),
            "deploy" => Some(Tier::Deploy),
            "mutate" => Some(Tier::Mutate),
            _ => None,
        }
    }

    pub fn label_from_id(id: &str) -> &'static str {
        Tier::from_id(id).map(|t| t.label()).unwrap_or("未知")
    }

    pub fn id(&self) -> &'static str {
        match self {
            Tier::Observe => "observe",
            Tier::Parse => "parse",
            Tier::Deploy => "deploy",
            Tier::Mutate => "mutate",
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Tier::Observe => "T0",
            Tier::Parse => "T1",
            Tier::Deploy => "T2",
            Tier::Mutate => "T3",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tier::Observe => "观察",
            Tier::Parse => "解析",
            Tier::Deploy => "部署",
            Tier::Mutate => "变更",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Tier::Observe => "只读元数据，无任何副作用，扫描时自动执行",
            Tier::Parse => "只读文件内容并解析，无任何副作用，扫描时自动执行",
            Tier::Deploy => "写入磁盘（部署 Skill、写托管块、合并配置键），写前备份、可回滚",
            Tier::Mutate => "调用外部程序改变系统状态（装包、建环境、删路径），需显式确认",
        }
    }
}

/// 能力目录条目（GUI 直接渲染这张表）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySpec {
    pub id: String,
    pub tier: Tier,
    pub tier_code: String,
    pub label: String,
    pub description: String,
    /// M0 是否已实现（未实现的仅登记级别，供 UI 展示与授权预演）
    pub implemented: bool,
    /// 参数说明，例如 ["paths", "args"]
    pub params: Vec<String>,
}

/// Agent 定义里一条证据规则的参数
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleParams {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    pub format: Option<String>,
    /// 文件内的节点路径，点号分隔（如 "options.baseURL"）
    pub root: Option<String>,
    /// 具体的键名
    pub key: Option<String>,
}

/// 能力求值结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityOutcome {
    pub hit: bool,
    /// 展示值（路径、版本、包名…）
    pub value: String,
    pub detail: Option<String>,
    pub error: Option<String>,
}

/// 求值上下文：一次性扫描内共享的只读数据
pub struct EvalContext<'a> {
    pub npm_packages: &'a [NpmPackage],
    pub settings: &'a AppSettings,
    pub home: PathBuf,
}

impl<'a> EvalContext<'a> {
    pub fn new(npm_packages: &'a [NpmPackage], settings: &'a AppSettings) -> Self {
        Self {
            npm_packages,
            settings,
            home: util::home_dir(),
        }
    }
}

/* ----------------------------------------------------------- 能力目录表 */

const CATALOG: &[(&str, Tier, &str, &str, bool, &[&str])] = &[
    /* ---------------------------------------------------------- T0 观察 */
    ("path.exists", Tier::Observe, "路径是否存在", "检查一个或多个路径是否存在，返回首个命中的路径", true, &["paths"]),
    ("path.stat", Tier::Observe, "路径元信息", "返回是否存在、大小、最后修改时间", true, &["paths"]),
    ("path.glob", Tier::Observe, "目录匹配", "在目录下按模式匹配条目（`*` 段内通配、`**` 跨层、`?` 单字符；深度与条目上限保护）", true, &["paths", "args"]),
    ("link.read", Tier::Observe, "读取链接", "判断路径是否为符号链接/junction，返回链接类型与目标", true, &["paths"]),
    ("dir.count", Tier::Observe, "统计目录条目", "统计目录内条目数量", true, &["paths"]),
    ("which", Tier::Observe, "解析可执行文件", "按 PATH + 额外目录 + 用户手动指定的位置解析命令行程序", true, &["args", "paths"]),
    ("run.version", Tier::Observe, "读取程序版本", "执行 `<程序> --version` 并提取版本号（带超时保护）", true, &["args"]),
    ("npm.global.has", Tier::Observe, "检查全局 npm 包", "在已扫描的全局包清单中查找指定包", true, &["args"]),
    /* ---------------------------------------------------------- T1 解析 */
    ("file.json.get", Tier::Parse, "读取 JSON 节点", "按点号路径读取 JSON 文件中的节点", true, &["paths", "root"]),
    ("file.json.keys", Tier::Parse, "列出 JSON 键", "列出 JSON 文件某个节点的直接子键", true, &["paths", "root"]),
    ("file.toml.get", Tier::Parse, "读取 TOML 节点", "按点号路径读取 TOML 文件中的节点", true, &["paths", "root"]),
    ("file.toml.keys", Tier::Parse, "列出 TOML 键", "列出 TOML 文件某个节点的直接子键", true, &["paths", "root"]),
    ("file.text.head", Tier::Parse, "读取文本开头", "读取文本文件的前若干行（用于配置预览）", true, &["paths", "args"]),
    ("skill.frontmatter", Tier::Parse, "解析 Skill frontmatter", "解析 Markdown 前置元数据（name / description / when_to_use）", true, &["paths"]),
    /* ---------------------------------------------------------- T2 部署 */
    ("skill.copy", Tier::Deploy, "拷贝 Skill", "把 Skill 目录复制到技能库或 Agent 目录（写前记录清单）", true, &["paths"]),
    ("skill.link", Tier::Deploy, "链接 Skill", "以目录链接 / junction 部署 Skill（Windows 下无需管理员）", true, &["paths"]),
    ("skill.relink", Tier::Deploy, "重建失效链接", "把失效链接重新指向新的技能库路径", true, &["paths", "args"]),
    ("file.merge_keys", Tier::Deploy, "合并配置键", "结构化文件（JSON）的节点级合并：只增删本软件管理的键，用户手写的其它内容不动；同名非受管条目默认跳过", true, &["paths", "root"]),
    ("file.write_block", Tier::Deploy, "写托管块", "文本文件（TOML 等）的标记托管块：只替换标记之间的内容，标记之外一字不改", true, &["paths", "root"]),
    ("file.render", Tier::Deploy, "整文件模版渲染", "用模版渲染整个文件（Tera 语法）", false, &["paths"]),
    ("backup.create", Tier::Deploy, "记录可恢复清单", "写入前记录 manifest（操作类型、来源、目标），据此可撤销或恢复", true, &["paths"]),
    ("backup.restore", Tier::Deploy, "依清单恢复", "按 manifest 撤销导入 / 重建被删链接 / 从回收站移回", true, &["paths"]),
    /* ---------------------------------------------------------- T3 变更 */
    ("pkg.npm.install", Tier::Mutate, "安装 npm 全局包", "执行全局包安装（流式输出到任务控制台）", false, &["args"]),
    ("pkg.npm.remove", Tier::Mutate, "卸载 npm 全局包", "移除全局包", false, &["args"]),
    ("py.env.create", Tier::Mutate, "创建 Python 环境", "通过 uv venv 创建虚拟环境（读取 pyvenv.cfg 版本并标记为受管）；conda 创建暂未启用", true, &["args", "paths"]),
    ("py.env.remove", Tier::Mutate, "删除 Python 环境", "受管环境整体移入回收站（可恢复）并清除受管记录", true, &["paths"]),
    ("proc.spawn.probe", Tier::Mutate, "MCP 握手探测", "实际启动 MCP 进程（stdio）或发起 HTTP 请求，完成 initialize 握手并清点工具数；进程结束即恢复原状", true, &["args"]),
    ("net.provider.probe", Tier::Mutate, "供应商连通性测试", "对供应商端点发起一次最小只读请求（GET models）：返回延迟、模型数与错误原因；Key 只在内存中使用，不落日志不落库", true, &["args"]),
    ("net.provider.balance", Tier::Mutate, "供应商余额查询", "查询供应商账户余额（当前支持 DeepSeek 的 /user/balance）：返回币种、总余额、赠送与充值拆分；Key 只在内存中使用", true, &["args"]),
    ("path.delete", Tier::Mutate, "删除路径", "删除链接或目录 —— 全部先移入回收站（保留指向关系与内容），可一键恢复", true, &["paths"]),
    ("git.clone", Tier::Mutate, "克隆 Git 仓库", "把远程仓库浅克隆到临时目录（遵循设置里的网络代理）", true, &["args", "paths"]),
];

pub fn catalog() -> Vec<CapabilitySpec> {
    CATALOG
        .iter()
        .map(|(id, tier, label, description, implemented, params)| CapabilitySpec {
            id: (*id).to_string(),
            tier: *tier,
            tier_code: tier.code().to_string(),
            label: (*label).to_string(),
            description: (*description).to_string(),
            implemented: *implemented,
            params: params.iter().map(|p| (*p).to_string()).collect(),
        })
        .collect()
}

pub fn tier_of(capability_id: &str) -> Option<Tier> {
    CATALOG
        .iter()
        .find(|(id, ..)| *id == capability_id)
        .map(|(_, tier, ..)| *tier)
}

/* --------------------------------------------------------------- 求值 */

pub fn invoke(capability_id: &str, params: &RuleParams, ctx: &EvalContext) -> CapabilityOutcome {
    match capability_id {
        /* ------------------------------------------------------ T0 观察 */
        "path.exists" => {
            for raw in &params.paths {
                let p = expand(raw);
                if p.exists() {
                    return hit(p.to_string_lossy().to_string());
                }
            }
            miss("路径不存在")
        }
        "path.stat" => {
            for raw in &params.paths {
                let p = expand(raw);
                if let Ok(meta) = std::fs::metadata(&p) {
                    let kind = if meta.is_dir() { "目录" } else { "文件" };
                    let size = util::file_size(&p).unwrap_or(0);
                    return CapabilityOutcome {
                        hit: true,
                        value: p.to_string_lossy().to_string(),
                        detail: Some(format!("{} · {} 字节", kind, size)),
                        error: None,
                    };
                }
            }
            miss("路径不存在")
        }
        "path.glob" => {
            let Some(pattern) = params.args.first().filter(|p| !p.trim().is_empty()) else {
                return miss("缺少模式参数（args[0]）");
            };
            for raw in &params.paths {
                let root = expand(raw);
                if !root.is_dir() {
                    continue;
                }
                let mut matches: Vec<String> = Vec::new();
                util::glob_collect(&root, pattern.trim(), &mut matches, 2000, 12);
                if !matches.is_empty() {
                    let first = root.join(&matches[0]);
                    return CapabilityOutcome {
                        hit: true,
                        value: first.to_string_lossy().to_string(),
                        detail: Some(format!(
                            "匹配 {} 个条目（上限 2000，深度 12）",
                            matches.len()
                        )),
                        error: None,
                    };
                }
            }
            miss("无匹配条目")
        }
        "link.read" => {
            for raw in &params.paths {
                let p = expand(raw);
                match std::fs::symlink_metadata(&p) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        let target = std::fs::read_link(&p)
                            .ok()
                            .map(|t| t.to_string_lossy().to_string());
                        let broken = !p.exists();
                        return CapabilityOutcome {
                            hit: !broken,
                            value: target.clone().unwrap_or_default(),
                            detail: Some(if broken {
                                "链接已失效：目标不存在".to_string()
                            } else {
                                "链接有效".to_string()
                            }),
                            error: None,
                        };
                    }
                    Ok(_) => continue,
                    Err(_) => continue,
                }
            }
            miss("未发现链接")
        }
        "dir.count" => {
            for raw in &params.paths {
                let p = expand(raw);
                if p.is_dir() {
                    let count = std::fs::read_dir(&p)
                        .map(|it| it.flatten().count())
                        .unwrap_or(0);
                    return CapabilityOutcome {
                        hit: count > 0,
                        value: p.to_string_lossy().to_string(),
                        detail: Some(format!("{} 个条目", count)),
                        error: None,
                    };
                }
            }
            miss("目录不存在")
        }
        "which" => {
            let extra: Vec<PathBuf> = params.paths.iter().map(|p| expand(p)).collect();
            for name in &params.args {
                // 用户手动指定的路径优先
                let override_path = ctx
                    .settings
                    .executable_overrides
                    .get(name)
                    .map(|p| expand(p))
                    .filter(|p| p.is_file());
                if let Some(path) = override_path.or_else(|| util::resolve_program(name, &extra)) {
                    return hit(path.to_string_lossy().to_string());
                }
            }
            miss("未在 PATH 与已知位置找到")
        }
        "run.version" => {
            let Some(name) = params.args.first() else {
                return miss("缺少程序名参数");
            };
            let extra: Vec<PathBuf> = params.paths.iter().map(|p| expand(p)).collect();
            let Some(path) = util::resolve_program(name, &extra) else {
                return miss("程序未找到");
            };
            match util::run_capture(&path, &["--version"], Duration::from_secs(6)) {
                Ok(out) => match util::extract_version(&out) {
                    Some(v) => CapabilityOutcome {
                        hit: true,
                        value: v,
                        detail: Some(path.to_string_lossy().to_string()),
                        error: None,
                    },
                    None => miss("无法解析版本号"),
                },
                Err(e) => CapabilityOutcome {
                    hit: false,
                    value: String::new(),
                    detail: None,
                    error: Some(e),
                },
            }
        }
        "npm.global.has" => {
            for name in &params.args {
                if let Some(pkg) = ctx
                    .npm_packages
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                {
                    return hit(format!("{}@{}", pkg.name, pkg.version));
                }
            }
            miss("全局包未安装")
        }
        /* ------------------------------------------------------ T1 解析 */
        "file.json.get" => read_json_node(params, |node| {
            let text = serde_json::to_string(node).unwrap_or_default();
            truncate(text, 200)
        }),
        "file.json.keys" => read_json_node(params, |node| {
            node.as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join("、"))
                .unwrap_or_default()
        }),
        "file.toml.get" => read_toml_node(params, |node| {
            truncate(serde_json::to_string(node).unwrap_or_default(), 200)
        }),
        "file.toml.keys" => read_toml_node(params, |node| {
            node.as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join("、"))
                .unwrap_or_default()
        }),
        "file.text.head" => {
            let lines = params
                .args
                .first()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(12);
            for raw in &params.paths {
                let p = expand(raw);
                if let Ok(text) = std::fs::read_to_string(&p) {
                    let head: Vec<&str> = text.lines().take(lines).collect();
                    return CapabilityOutcome {
                        hit: true,
                        value: head.join("\n"),
                        detail: Some(p.to_string_lossy().to_string()),
                        error: None,
                    };
                }
            }
            miss("文件不存在或不可读")
        }
        "skill.frontmatter" => {
            let mut names = Vec::new();
            for raw in &params.paths {
                let p = expand(raw);
                if p.is_file() {
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    let (meta, _) = crate::scan::skills::parse_frontmatter(&text);
                    if let Some(name) = meta.get("name") {
                        names.push(name.clone());
                    }
                }
            }
            if names.is_empty() {
                miss("未解析到 frontmatter")
            } else {
                hit(names.join("、"))
            }
        }
        /* ------------------------------------------ T2 / T3：已登记，未实现 */
        other => CapabilityOutcome {
            hit: false,
            value: String::new(),
            detail: Some(format!(
                "能力 {} 尚未实现（M0 只实现 T0 观察 / T1 解析）",
                other
            )),
            error: None,
        },
    }
}

/* --------------------------------------------------------------- 助手 */

fn expand(raw: &str) -> PathBuf {
    if raw.starts_with("./") || raw.starts_with(".\\") {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(raw.trim_start_matches("./").trim_start_matches(".\\"))
    } else {
        util::expand_buf(raw)
    }
}

fn hit(value: impl Into<String>) -> CapabilityOutcome {
    CapabilityOutcome {
        hit: true,
        value: value.into(),
        detail: None,
        error: None,
    }
}

fn miss(reason: &str) -> CapabilityOutcome {
    CapabilityOutcome {
        hit: false,
        value: String::new(),
        detail: Some(reason.to_string()),
        error: None,
    }
}

fn truncate(text: String, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text
    }
}

/// 按点号路径在 JSON 值中下钻
pub fn walk_json<'v>(root: &'v Value, path: &str) -> Option<&'v Value> {
    if path.trim().is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in path.split('.').filter(|s| !s.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

fn read_json_node(
    params: &RuleParams,
    render: impl Fn(&Value) -> String,
) -> CapabilityOutcome {
    for raw in &params.paths {
        let p = expand(raw);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let node = walk_json(&json, params.root.as_deref().unwrap_or(""));
        if let Some(node) = node {
            let value = render(node);
            if !value.is_empty() {
                return CapabilityOutcome {
                    hit: true,
                    value,
                    detail: Some(p.to_string_lossy().to_string()),
                    error: None,
                };
            }
        }
    }
    miss("未读取到节点")
}

fn read_toml_node(
    params: &RuleParams,
    render: impl Fn(&Value) -> String,
) -> CapabilityOutcome {
    for raw in &params.paths {
        let p = expand(raw);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        let Ok(json) = serde_json::to_value(&parsed) else {
            continue;
        };
        let node = walk_json(&json, params.root.as_deref().unwrap_or(""));
        if let Some(node) = node {
            let value = render(node);
            if !value.is_empty() {
                return CapabilityOutcome {
                    hit: true,
                    value,
                    detail: Some(p.to_string_lossy().to_string()),
                    error: None,
                };
            }
        }
    }
    miss("未读取到节点")
}

/// 供未来模版/规则引擎复用的占位
pub type RuleExtras = HashMap<String, String>;