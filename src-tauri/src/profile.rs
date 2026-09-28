//! Profile（环境档案）一键应用。
//!
//! 把「哪套 MCP + 哪个供应商 + 哪些 Skill」打包成一个档案，一次确认后：
//! * **配置写入**复用 `sync` 的 MCP 与供应商引擎（结构合并 / 托管块 + 写前备份）
//! * **Skill 部署**按目标 Agent 定义里 `role = "skills"` 的路径与 `deploy` 方式
//!   （link / copy）落位，已存在的内容默认不动

use crate::agentdef::LoadedDef;
use crate::model::{
    AgentTarget, McpResource, MergeChange, ProviderResource, SyncPlan, SyncTargetPlan,
};
use crate::store::Store;

/// 档案里的一个 Skill
#[derive(Debug, Clone)]
pub struct ProfileSkill {
    pub path: String,
    pub name: String,
}

pub struct ProfileApplyRequest<'a> {
    pub defs: &'a [LoadedDef],
    pub agents: &'a [AgentTarget],
    /// 已按档案筛选出的资源
    pub mcp_resources: &'a [McpResource],
    pub providers: &'a [ProviderResource],
    pub skills: &'a [ProfileSkill],
    pub resolve_key: &'a dyn Fn(&str) -> Option<String>,
    pub agent_ids: &'a [String],
    pub overwrite_unmanaged: bool,
    pub store: &'a Store,
}

/// 目标 Agent 的 Skill 部署目录（取定义里第一个 role = skills 的路径）
fn skill_deploy_dir(def: &LoadedDef) -> Option<(std::path::PathBuf, String)> {
    let rule = def.file.skill_paths().into_iter().next()?;
    let dir = crate::agentdef::resolve_path(&rule.path);
    let method = if rule.deploy.is_empty() {
        "link".to_string()
    } else {
        rule.deploy.clone()
    };
    Some((dir, method))
}

/// Skill 部署目标：每个 Agent 一个目标，条目逐个说明动作
fn plan_skill_targets(req: &ProfileApplyRequest) -> Vec<SyncTargetPlan> {
    let mut out: Vec<SyncTargetPlan> = Vec::new();
    for def in req.defs {
        let agent_id = &def.file.agent.id;
        if !req.agent_ids.is_empty() && !req.agent_ids.iter().any(|id| id == agent_id) {
            continue;
        }
        let Some(agent) = req.agents.iter().find(|a| &a.id == agent_id) else {
            continue;
        };
        if agent.status == "absent" {
            continue;
        }
        let Some((dir, method)) = skill_deploy_dir(def) else {
            continue;
        };

        let mut plan = SyncTargetPlan {
            agent_id: agent_id.clone(),
            agent_name: agent.name.clone(),
            kind: "skill".to_string(),
            file: dir.to_string_lossy().to_string(),
            format: if method == "copy" { "copy" } else { "link" }.to_string(),
            strategy: format!("skill-{}", method),
            supported: true,
            file_exists: dir.is_dir(),
            reason: if dir.is_dir() {
                None
            } else {
                Some("Skill 目录尚不存在，执行时会自动创建".to_string())
            },
            ..Default::default()
        };

        for skill in req.skills {
            let source = std::path::PathBuf::from(&skill.path);
            let dest = dir.join(&skill.name);
            if crate::actions::is_link(&dest) {
                let same = std::fs::read_link(&dest)
                    .map(|t| t.to_string_lossy().eq_ignore_ascii_case(&skill.path))
                    .unwrap_or(false);
                if same {
                    plan.unchanged += 1;
                    plan.changes.push(MergeChange {
                        kind: "unchanged".into(),
                        key: skill.name.clone(),
                        detail: "链接已指向同一位置".into(),
                    });
                } else {
                    plan.skipped += 1;
                    plan.changes.push(MergeChange {
                        kind: "skipped".into(),
                        key: skill.name.clone(),
                        detail: "已存在指向别处的链接，默认不动它".into(),
                    });
                }
            } else if dest.exists() {
                plan.skipped += 1;
                plan.changes.push(MergeChange {
                    kind: "skipped".into(),
                    key: skill.name.clone(),
                    detail: "目标位置已有真实目录，默认不覆盖".into(),
                });
            } else if !source.is_dir() {
                plan.skipped += 1;
                plan.changes.push(MergeChange {
                    kind: "skipped".into(),
                    key: skill.name.clone(),
                    detail: format!("来源不存在：{}", skill.path),
                });
            } else {
                plan.added += 1;
                plan.changes.push(MergeChange {
                    kind: "add".into(),
                    key: skill.name.clone(),
                    detail: format!(
                        "{} {}",
                        if method == "copy" { "拷贝到" } else { "链接到" },
                        dest.to_string_lossy()
                    ),
                });
            }
        }
        out.push(plan);
    }
    out
}

pub fn plan_profile_apply(req: &ProfileApplyRequest) -> Result<SyncPlan, String> {
    if req.mcp_resources.is_empty() && req.providers.is_empty() && req.skills.is_empty() {
        return Err("该档案里没有任何资源".to_string());
    }

    let mut targets: Vec<SyncTargetPlan> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if !req.mcp_resources.is_empty() {
        match crate::sync::plan_sync(&crate::sync::SyncRequest {
            defs: req.defs,
            agents: req.agents,
            resources: req.mcp_resources,
            agent_ids: req.agent_ids,
            overwrite_unmanaged: req.overwrite_unmanaged,
            store: req.store,
        }) {
            Ok(plan) => {
                warnings.extend(plan.warnings);
                targets.extend(plan.targets);
            }
            Err(e) => warnings.push(format!("MCP 部分未生成计划：{}", e)),
        }
    }

    if !req.providers.is_empty() {
        match crate::sync::plan_provider_sync(&crate::sync::ProviderSyncRequest {
            defs: req.defs,
            agents: req.agents,
            providers: req.providers,
            resolve_key: req.resolve_key,
            agent_ids: req.agent_ids,
            overwrite_unmanaged: req.overwrite_unmanaged,
            store: req.store,
        }) {
            Ok(plan) => {
                warnings.extend(plan.warnings);
                targets.extend(plan.targets);
            }
            Err(e) => warnings.push(format!("供应商部分未生成计划：{}", e)),
        }
    }

    if !req.skills.is_empty() {
        targets.extend(plan_skill_targets(req));
    }

    if targets.is_empty() {
        return Err(
            "没有匹配到任何可应用的 Agent（请确认目标已安装，且定义里有 MCP / providerWrite / skills 声明）"
                .to_string(),
        );
    }

    let config_targets = targets.iter().filter(|t| t.kind == "config").count();
    let skill_targets = targets.iter().filter(|t| t.kind == "skill").count();
    let changed = targets
        .iter()
        .filter(|t| t.added + t.updated + t.removed > 0)
        .count();

    Ok(SyncPlan {
        capability: "profile.apply".into(),
        tier: "deploy".into(),
        tier_code: "T2".into(),
        title: "应用环境档案".into(),
        summary: format!(
            "{} 个配置目标 + {} 个 Skill 目标（其中 {} 个有实际变更）",
            config_targets, skill_targets, changed
        ),
        servers: req
            .mcp_resources
            .iter()
            .map(|r| r.name.clone())
            .chain(req.providers.iter().map(|p| p.name.clone()))
            .chain(req.skills.iter().map(|s| s.name.clone()))
            .collect(),
        targets,
        warnings,
        confirm_hint:
            "T2 级操作：配置文件写入前自动备份；Skill 以链接/拷贝部署，已存在的内容默认不动".into(),
    })
}

pub fn apply_profile_apply(req: &ProfileApplyRequest) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "应用环境档案".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };

    let plan = match plan_profile_apply(req) {
        Ok(p) => p,
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: "(plan)".into(),
                ok: false,
                message: e,
            });
            return result;
        }
    };
    result.warnings = plan.warnings.clone();

    // 1) 配置写入：MCP
    if !req.mcp_resources.is_empty() {
        let sub = crate::sync::apply_sync(&crate::sync::SyncRequest {
            defs: req.defs,
            agents: req.agents,
            resources: req.mcp_resources,
            agent_ids: req.agent_ids,
            overwrite_unmanaged: req.overwrite_unmanaged,
            store: req.store,
        });
        if !sub.ok {
            result.ok = false;
            result.warnings.push(format!("MCP 分发未完全成功：{}", sub.summary));
        }
        result.steps.extend(sub.steps);
    }

    // 2) 配置写入：供应商
    if !req.providers.is_empty() {
        let sub = crate::sync::apply_provider_sync(&crate::sync::ProviderSyncRequest {
            defs: req.defs,
            agents: req.agents,
            providers: req.providers,
            resolve_key: req.resolve_key,
            agent_ids: req.agent_ids,
            overwrite_unmanaged: req.overwrite_unmanaged,
            store: req.store,
        });
        if !sub.ok {
            result.ok = false;
            result.warnings.push(format!("供应商分发未完全成功：{}", sub.summary));
        }
        result.steps.extend(sub.steps);
    }

    // 3) Skill 部署
    if !req.skills.is_empty() {
        let mut manifest_entries: Vec<crate::actions::ManifestEntry> = Vec::new();
        for target in plan.targets.iter().filter(|t| t.kind == "skill") {
            let dir = std::path::PathBuf::from(&target.file);
            if !dir.is_dir() {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    result.ok = false;
                    result.steps.push(crate::actions::StepResult {
                        target: target.file.clone(),
                        ok: false,
                        message: format!("创建 Skill 目录失败：{}", e),
                    });
                    continue;
                }
            }
            let copy_mode = target.strategy == "skill-copy";
            let mut done = 0usize;

            for change in target.changes.iter().filter(|c| c.kind == "add") {
                let Some(skill) = req.skills.iter().find(|s| s.name == change.key) else {
                    continue;
                };
                let source = std::path::PathBuf::from(&skill.path);
                let dest = dir.join(&skill.name);
                let outcome = if copy_mode {
                    crate::actions::copy_skill_dir(&source, &dest).map(|(files, bytes)| {
                        format!(
                            "已拷贝 {} 个文件 / {}",
                            files,
                            crate::util::format_bytes(bytes)
                        )
                    })
                } else {
                    crate::actions::create_dir_link(&dest, &source)
                        .map(|_| "已创建链接".to_string())
                };
                match outcome {
                    Ok(message) => {
                        done += 1;
                        manifest_entries.push(crate::actions::ManifestEntry {
                            action: if copy_mode {
                                "copy-dir".into()
                            } else {
                                "create-link".into()
                            },
                            target: dest.to_string_lossy().to_string(),
                            source: Some(skill.path.clone()),
                            ..Default::default()
                        });
                        result.steps.push(crate::actions::StepResult {
                            target: dest.to_string_lossy().to_string(),
                            ok: true,
                            message,
                        });
                    }
                    Err(e) => {
                        result.ok = false;
                        result.steps.push(crate::actions::StepResult {
                            target: dest.to_string_lossy().to_string(),
                            ok: false,
                            message: e,
                        });
                    }
                }
            }

            for change in target.changes.iter().filter(|c| c.kind != "add") {
                result.steps.push(crate::actions::StepResult {
                    target: dir.join(&change.key).to_string_lossy().to_string(),
                    ok: true,
                    message: change.detail.clone(),
                });
            }
            let _ = req.store.sync_history_add(
                &target.file,
                &format!("{}：部署 {} 个 Skill", target.agent_name, done),
                None,
                "applied",
            );
        }
        if !manifest_entries.is_empty() {
            result.manifest = crate::actions::write_manifest_public(
                "profile-skill-deploy",
                "应用档案时的 Skill 部署",
                manifest_entries,
            );
        }
    }

    let ok_count = result.steps.iter().filter(|s| s.ok).count();
    result.summary = format!(
        "档案应用完成：{} / {} 项成功{}",
        ok_count,
        result.steps.len(),
        if result.ok { "" } else { "（存在失败项，见明细）" }
    );
    result.restore_hint =
        "配置文件可在「历史与审计 → 配置备份」回滚；Skill 部署可依清单撤销".into();
    result
}