//! Agent 目标判定 —— 求值定义文件里的证据规则。
//!
//! 本模块**不含任何 Agent 专属知识**：全部信息来自 `config/agents/*.toml`。
//! 职责只有两件事：
//!   1. 把定义里的 `paths[].role` 翻译成默认证据（install→强、config→中、data/skills→弱）
//!   2. 对显式 `[[evidence]]` 规则调用对应级别的基础能力，按强度汇总成状态

use crate::agentdef::{AgentFile, LoadedDef, PathRule};
use crate::capability::{self, EvalContext, RuleParams};
use crate::model::{AgentEvidence, AgentTarget, ConfigPath};

/// 判断是否为「真正的 Agent」（kind = host 的宿主应用不算）
pub fn is_agent_kind(kind: &str) -> bool {
    kind != "host"
}

pub fn evaluate(defs: &[LoadedDef], ctx: &EvalContext) -> Vec<AgentTarget> {
    defs.iter().map(|def| build(def, ctx)).collect()
}

fn path_for(raw: &str) -> std::path::PathBuf {
    if raw.starts_with("./") || raw.starts_with(".\\") {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        cwd.join(raw.trim_start_matches("./").trim_start_matches(".\\"))
    } else {
        crate::util::expand_buf(raw)
    }
}

/// role → (信号类型, 强度, 默认标签)
fn implied(path: &PathRule) -> Option<(&'static str, &'static str, String)> {
    match path.role.as_str() {
        "install" => Some(("install-dir", "strong", "程序安装目录".to_string())),
        "config" => Some(("config", "medium", path.label.clone())),
        "data" => Some(("data", "weak", path.label.clone())),
        "skills" => Some(("skill-dir", "weak", "Skills 目录".to_string())),
        _ => None,
    }
}

fn build(def: &LoadedDef, ctx: &EvalContext) -> AgentTarget {
    let file: &AgentFile = &def.file;
    let mut evidence: Vec<AgentEvidence> = Vec::new();
    let mut cli_path: Option<String> = None;
    let mut cli_version: Option<String> = None;
    let mut npm_package: Option<String> = None;

    /* ------------------------------------------- 1) 显式证据（基础能力求值） */
    for rule in &file.evidence {
        let outcome = capability::invoke(&rule.capability, &rule.params, ctx);

        // `which` 命中时顺带探测版本（能力组合规则，由内核提供）
        if outcome.hit && rule.capability == "which" {
            // 提取程序名以探测版本（跳过明显不是程序的项）
            if let Some(program) = rule.params.args.first() {
                let version_params = RuleParams {
                    args: vec![program.clone()],
                    paths: rule.params.paths.clone(),
                    ..Default::default()
                };
                if let Ok(Some(v)) = version_of(&version_params, ctx) {
                    cli_version = Some(v);
                }
            }
            cli_path = Some(outcome.value.clone());
        }
        if outcome.hit && rule.capability == "npm.global.has" {
            npm_package = Some(outcome.value.clone());
        }

        evidence.push(AgentEvidence {
            signal: signal_of(&rule.capability),
            strength: rule.strength.clone(),
            label: if rule.label.is_empty() {
                format!("能力 {}", rule.capability)
            } else {
                rule.label.clone()
            },
            value: if outcome.hit {
                outcome.value.clone()
            } else {
                outcome
                    .detail
                    .clone()
                    .unwrap_or_else(|| outcome.error.clone().unwrap_or_default())
            },
            found: outcome.hit,
        });
    }

    /* ------------------------------------- 2) 角色派生的默认证据（同值去重） */
    for path in &file.paths {
        let Some((signal, strength, label)) = implied(path) else {
            continue;
        };
        let resolved = path_for(&path.path);
        if !resolved.exists() {
            continue;
        }
        let value = resolved.to_string_lossy().to_string();
        if evidence.iter().any(|e| e.value == value) {
            continue;
        }
        evidence.push(AgentEvidence {
            signal: signal.to_string(),
            strength: strength.to_string(),
            label,
            value,
            found: true,
        });
    }

    /* ------------------------------------------------------------ 3) 状态 */
    let has_strong = evidence.iter().any(|e| e.found && e.strength == "strong");
    let has_medium = evidence.iter().any(|e| e.found && e.strength == "medium");
    let has_weak = evidence.iter().any(|e| e.found && e.strength == "weak");
    let status = if has_strong {
        "installed"
    } else if has_medium {
        "configured"
    } else if has_weak {
        "leftover"
    } else {
        "absent"
    };

    // 已安装时只保留命中证据，避免「没找到 X」的噪音
    if status == "installed" {
        evidence.retain(|e| e.found);
    }

    let mut found_strong: Vec<&str> = Vec::new();
    for item in evidence.iter().filter(|e| e.found && e.strength == "strong") {
        if !found_strong.contains(&item.label.as_str()) {
            found_strong.push(item.label.as_str());
        }
    }
    let summary = match status {
        "installed" => format!("已安装：{}", found_strong.join(" + ")),
        "configured" => "仅发现配置文件，未找到程序本体（可能已卸载或为便携版）".to_string(),
        "leftover" => "仅有残留数据/技能目录，程序本体已不存在".to_string(),
        _ => "未检测到安装痕迹".to_string(),
    };

    /* --------------------------------------------------- 4) 配置路径与根 */
    let configs: Vec<ConfigPath> = file
        .paths
        .iter()
        .map(|p| {
            let resolved = path_for(&p.path);
            let exists = resolved.exists();
            ConfigPath {
                label: p.label.clone(),
                path: resolved.to_string_lossy().to_string(),
                kind: if p.format.is_empty() {
                    "dir".to_string()
                } else {
                    p.format.clone()
                },
                exists,
                size: if exists && resolved.is_file() {
                    crate::util::file_size(&resolved)
                } else {
                    None
                },
            }
        })
        .collect();

    let root = file
        .paths
        .iter()
        .filter(|p| p.role == "install")
        .map(|p| path_for(&p.path))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| {
            file.paths
                .iter()
                .map(|p| path_for(&p.path))
                .find(|p| p.exists())
                .map(|p| {
                    if p.is_dir() {
                        p.to_string_lossy().to_string()
                    } else {
                        p.parent()
                            .map(|x| x.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.to_string_lossy().to_string())
                    }
                })
        });

    /* ------------------------------------------------------------ 5) 说明 */
    let mut notes: Vec<String> = Vec::new();
    if !file.agent.note.is_empty() {
        notes.push(file.agent.note.clone());
    }
    if status == "leftover" {
        notes.push("如需纳管，请先安装该 Agent；残留目录不会被自动清理".to_string());
    }
    if status != "absent" && !file.capabilities.max_tier.is_empty() {
        notes.push(format!(
            "能力上限：{}（{}）",
            file.capabilities.max_tier,
            crate::capability::Tier::label_from_id(&file.capabilities.max_tier)
        ));
    }
    notes.push(if def.from_user_dir {
        format!("定义来源：用户文件 {}", def.source)
    } else {
        "定义来源：内置".to_string()
    });
    if !file.agent.adapter.is_empty() {
        notes.push(format!("内置同步适配器：{}（M2 启用）", file.agent.adapter));
    }

    AgentTarget {
        id: file.agent.id.clone(),
        name: if file.agent.name.is_empty() {
            file.agent.id.clone()
        } else {
            file.agent.name.clone()
        },
        vendor: file.agent.vendor.clone(),
        accent: file.agent.accent.clone(),
        kind: file.agent.kind.clone(),
        status: status.to_string(),
        installed: status == "installed",
        summary,
        cli: cli_path,
        cli_version,
        npm_package,
        root,
        configs,
        evidence,
        mcp_count: 0,
        skill_count: 0,
        adapter: file.agent.adapter.clone(),
        notes,
    }
}

fn version_of(params: &RuleParams, ctx: &EvalContext) -> Result<Option<String>, String> {
    // 直接复用内核的 run.version 能力
    let outcome = capability::invoke("run.version", params, ctx);
    Ok(if outcome.hit { Some(outcome.value) } else { None })
}

/// 能力 id → 证据信号类型（供 UI 图标区分）
fn signal_of(capability_id: &str) -> String {
    match capability_id {
        "which" | "run.version" => "cli",
        "npm.global.has" => "npm",
        "path.exists" | "path.stat" => "install-dir",
        "link.read" => "skill-dir",
        "dir.count" => "data",
        c if c.starts_with("file.") => "config",
        _ => "config",
    }
    .to_string()
}