//! MCP 资源分发（Sync Engine 的 MCP 部分）。
//!
//! 内核不硬编码任何 Agent 的配置格式 —— 目标文件、节点路径、格式与写入策略
//! 全部来自 Agent 定义里的 `[[mcp]]`。本模块只负责：
//!
//! 1. 把资源库里的 MCP 服务器按目标 Agent 的条目形状渲染成结构化条目
//! 2. 求值合并（见 `merge`）并产出 new_text 与 diff
//! 3. 执行时**先备份**、再原子写入、最后记录同步状态与历史
//!
//! 与 T2 的其它能力一致：先在界面上展示计划（含 diff），确认后才落盘。

use crate::agentdef::LoadedDef;
use crate::merge;
use crate::model::{
    AgentTarget, BackupInfo, McpResource, SyncPlan, SyncTargetPlan,
};
use crate::store::Store;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/* ------------------------------------------------------------ 条目渲染 */

/// 按目标 Agent 的条目形状渲染一个 MCP 资源
fn render_entry(res: &McpResource, style: &str) -> Value {
    let mut env = Map::new();
    for pair in &res.env {
        if !pair.key.trim().is_empty() {
            env.insert(pair.key.clone(), json!(pair.value));
        }
    }

    match style {
        // opencode 系：type = local / remote，command 为数组，环境变量键名为 environment
        "opencode" => {
            let mut obj = Map::new();
            if res.transport == "stdio" {
                obj.insert("type".into(), json!("local"));
                let mut command = vec![json!(res.command.clone())];
                command.extend(res.args.iter().map(|a| json!(a)));
                obj.insert("command".into(), Value::Array(command));
                if !env.is_empty() {
                    obj.insert("environment".into(), Value::Object(env));
                }
            } else {
                obj.insert("type".into(), json!("remote"));
                obj.insert("url".into(), json!(res.url.clone()));
            }
            Value::Object(obj)
        }
        // 标准形状（Claude Code / VS Code / Cursor / Cline / Roo / Gemini）
        _ => {
            let mut obj = Map::new();
            if res.transport == "stdio" {
                obj.insert("command".into(), json!(res.command.clone()));
                if !res.args.is_empty() {
                    obj.insert(
                        "args".into(),
                        Value::Array(res.args.iter().map(|a| json!(a)).collect()),
                    );
                }
                if !env.is_empty() {
                    obj.insert("env".into(), Value::Object(env));
                }
            } else {
                obj.insert("url".into(), json!(res.url.clone()));
            }
            Value::Object(obj)
        }
    }
}

/// 托管块正文（TOML 表格）
fn render_toml_block(root: &str, resources: &[&McpResource]) -> String {
    let table_root = if root.trim().is_empty() {
        "mcp_servers"
    } else {
        root
    };
    let mut out = String::new();
    for res in resources {
        out.push_str(&format!("[{}.{}]\n", table_root, res.name));
        if res.transport == "stdio" {
            out.push_str(&format!(
                "command = {}\n",
                toml_string(&res.command)
            ));
            if !res.args.is_empty() {
                let args: Vec<String> = res.args.iter().map(|a| toml_string(a)).collect();
                out.push_str(&format!("args = [{}]\n", args.join(", ")));
            }
            if !res.env.is_empty() {
                out.push_str(&format!("[{}.{}.env]\n", table_root, res.name));
                for pair in &res.env {
                    if !pair.key.trim().is_empty() {
                        out.push_str(&format!(
                            "{} = {}\n",
                            pair.key,
                            toml_string(&pair.value)
                        ));
                    }
                }
            }
        } else {
            out.push_str(&format!("url = {}\n", toml_string(&res.url)));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn toml_string(raw: &str) -> String {
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

/* -------------------------------------------------------------- 路径 */

fn resolve_file(raw: &str) -> PathBuf {
    crate::agentdef::resolve_path(raw)
}

/* -------------------------------------------------------------- 计划 */

pub struct SyncRequest<'a> {
    pub defs: &'a [LoadedDef],
    pub agents: &'a [AgentTarget],
    pub resources: &'a [McpResource],
    /// 目标 Agent id；空表示全部有定义且非 absent 的 Agent
    pub agent_ids: &'a [String],
    /// 是否允许覆盖同名但非 AgentHub 管理的条目（默认 false，只新增不覆盖）
    pub overwrite_unmanaged: bool,
    pub store: &'a Store,
}

/// 生成分发计划（只读，不改任何文件）
pub fn plan_sync(req: &SyncRequest) -> Result<SyncPlan, String> {
    let enabled: Vec<&McpResource> = req.resources.iter().filter(|r| r.enabled).collect();
    if enabled.is_empty() {
        return Err("没有启用中的 MCP 资源可分发".to_string());
    }

    let mut targets: Vec<SyncTargetPlan> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

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
        if def.file.mcp.is_empty() {
            continue;
        }
        // 能力分级：observe 级不允许写入
        if def.file.capabilities.max_tier == "observe" {
            warnings.push(format!(
                "{} 的能力上限是 observe，已跳过（可在其定义里提升 maxTier）",
                agent.name
            ));
            continue;
        }

        for source in &def.file.mcp {
            let file = resolve_file(&source.file);
            let strategy = source.effective_strategy().to_string();
            let format = if source.format.is_empty() {
                file.extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default()
            } else {
                source.format.clone()
            };

            let existing = if file.is_file() {
                std::fs::read_to_string(&file).unwrap_or_default()
            } else {
                String::new()
            };
            let managed_before = req
                .store
                .sync_state_get(agent_id, &file.to_string_lossy(), &source.root);

            // 格式支持判断
            let supported = match (strategy.as_str(), format.as_str()) {
                ("merge-keys", "json") => true,
                ("managed-block", "toml") => true,
                _ => false,
            };

            let mut plan = SyncTargetPlan {
                agent_id: agent_id.clone(),
                agent_name: agent.name.clone(),
                kind: "config".to_string(),
                file: file.to_string_lossy().to_string(),
                root: source.root.clone(),
                format: format.clone(),
                strategy: strategy.clone(),
                supported,
                file_exists: file.is_file(),
                ..Default::default()
            };

            if !supported {
                plan.reason = Some(format!(
                    "暂不支持写入 {} + {} 的组合（当前支持 JSON 结构合并与 TOML 托管块）",
                    strategy, format
                ));
                targets.push(plan);
                continue;
            }

            let outcome = match strategy.as_str() {
                "merge-keys" => {
                    let desired: BTreeMap<String, Value> = enabled
                        .iter()
                        .map(|r| (r.name.clone(), render_entry(r, source.effective_style())))
                        .collect();
                    merge::merge_json_keys(
                        &existing,
                        &source.root,
                        &desired,
                        &managed_before,
                        req.overwrite_unmanaged,
                    )?
                }
                _ => {
                    let body = render_toml_block(&source.root, &enabled);
                    let marker = if source.marker.is_empty() {
                        source.root.clone()
                    } else {
                        source.marker.clone()
                    };
                    merge::managed_block(&existing, &marker, &body)?
                }
            };

            let (added, updated, removed, skipped, unchanged) = outcome.counts();
            // 合并层的提示（例如「非受管同名条目已跳过」）必须传导到计划里，
            // 否则用户在界面上看不到被跳过了什么
            for warning in &outcome.warnings {
                warnings.push(format!("{}：{}", agent.name, warning));
            }
            plan.changes = outcome.changes;
            plan.added = added;
            plan.updated = updated;
            plan.removed = removed;
            plan.skipped = skipped;
            plan.unchanged = unchanged;
            plan.diff = merge::unified_diff(&existing, &outcome.new_text, 3);
            plan.backup_path = Some(planned_backup_path(&file).to_string_lossy().to_string());
            targets.push(plan);
        }
    }

    if targets.is_empty() {
        return Err("没有匹配到任何可写入的 Agent 配置（请确认目标 Agent 已安装且定义了 MCP 来源）".to_string());
    }
    let write_targets = targets.iter().filter(|t| t.supported).count();
    let changed = targets
        .iter()
        .filter(|t| t.supported && (t.added + t.updated + t.removed) > 0)
        .count();
    if changed == 0 {
        warnings.push("所有目标文件已与资源库一致，无需写入".to_string());
    }

    Ok(SyncPlan {
        capability: "file.merge_keys".into(),
        tier: "deploy".into(),
        tier_code: "T2".into(),
        title: "分发 MCP 到 Agent".into(),
        summary: format!(
            "{} 个 MCP 资源 → {} 个配置文件（其中 {} 个需要写入）",
            enabled.len(),
            write_targets,
            changed
        ),
        servers: enabled.iter().map(|r| r.name.clone()).collect(),
        targets,
        warnings,
        confirm_hint: "T2 级操作：写入前会自动备份目标文件，可随时回滚；用户手写的其它内容不会被动".into(),
    })
}

/// 备份路径：毫秒级时间戳 + 冲突递增。
///
/// 曾用秒级时间戳，导致同一秒内的两次写入指向同一个备份文件，
/// 后一次会顶掉前一次的记录（backup_path 唯一），回滚点因此丢失。
fn planned_backup_path(file: &Path) -> PathBuf {
    let now = chrono::Local::now();
    let stamp = format!(
        "{}{}",
        now.format("%Y%m%d-%H%M%S"),
        now.format("%.3f").to_string().replace('.', "-")
    );
    let flat = file
        .to_string_lossy()
        .replace(':', "")
        .replace(['\\', '/'], "_");
    let dir = crate::actions::data_dir().join("backups").join(&stamp);
    let mut candidate = dir.join(format!("{}.bak", flat));
    let mut counter = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}-{}.bak", flat, counter));
        counter += 1;
    }
    candidate
}

/* -------------------------------------------------------------- 执行 */

/// 执行分发：先备份、再原子写入、最后记录状态与历史
pub fn apply_sync(req: &SyncRequest) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "分发 MCP 到 Agent".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };

    let plan = match plan_sync(req) {
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

    let backup_root = crate::actions::data_dir().join("backups");
    let mut written = 0usize;
    let mut skipped = 0usize;

    for target in plan.targets.iter().filter(|t| t.supported) {
        if target.added + target.updated + target.removed == 0 {
            skipped += 1;
            result.steps.push(crate::actions::StepResult {
                target: target.file.clone(),
                ok: true,
                message: "已是最新，跳过写入".into(),
            });
            continue;
        }

        let file = PathBuf::from(&target.file);
        // 1) 备份
        let mut backup_path: Option<String> = None;
        if file.is_file() {
            let dest = target
                .backup_path
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| planned_backup_path(&file));
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(&file, &dest) {
                Ok(_) => {
                    let bytes = crate::util::file_size(&dest).unwrap_or(0);
                    let _ = req.store.backup_add(
                        &target.file,
                        &dest.to_string_lossy(),
                        bytes,
                        &format!("{} 写入前备份", target.agent_name),
                    );
                    backup_path = Some(dest.to_string_lossy().to_string());
                }
                Err(e) => {
                    result.steps.push(crate::actions::StepResult {
                        target: target.file.clone(),
                        ok: false,
                        message: format!("备份失败，已中止写入：{}", e),
                    });
                    result.ok = false;
                    continue;
                }
            }
        }

        // 2) 重新求值并原子写入
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        let desired: BTreeMap<String, Value> = req
            .resources
            .iter()
            .filter(|r| r.enabled)
            .map(|r| {
                let style = req
                    .defs
                    .iter()
                    .find(|d| d.file.agent.id == target.agent_id)
                    .and_then(|d| d.file.mcp.iter().find(|m| m.root == target.root))
                    .map(|m| m.effective_style())
                    .unwrap_or("standard");
                (r.name.clone(), render_entry(r, style))
            })
            .collect();

        let new_text = if target.strategy == "merge-keys" {
            match merge::merge_json_keys(
                    &existing,
                    &target.root,
                    &desired,
                    &req.store
                        .sync_state_get(&target.agent_id, &target.file, &target.root),
                    req.overwrite_unmanaged,
                ) {
                Ok(o) => o.new_text,
                Err(e) => {
                    result.steps.push(crate::actions::StepResult {
                        target: target.file.clone(),
                        ok: false,
                        message: format!("合并失败：{}", e),
                    });
                    result.ok = false;
                    continue;
                }
            }
        } else {
            let list: Vec<&McpResource> = req.resources.iter().filter(|r| r.enabled).collect();
            let body = render_toml_block(&target.root, &list);
            let marker = if target.root.is_empty() {
                "mcp".to_string()
            } else {
                target.root.clone()
            };
            match merge::managed_block(&existing, &marker, &body) {
                Ok(o) => o.new_text,
                Err(e) => {
                    result.steps.push(crate::actions::StepResult {
                        target: target.file.clone(),
                        ok: false,
                        message: format!("生成托管块失败：{}", e),
                    });
                    result.ok = false;
                    continue;
                }
            }
        };

        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match atomic_write(&file, &new_text) {
            Ok(_) => {
                written += 1;
                // 3) 记录受管键，便于下次清理已移除项
                if target.strategy == "merge-keys" {
                    let keys: Vec<String> = desired.keys().cloned().collect();
                    let _ = req.store.sync_state_set(
                        &target.agent_id,
                        &target.file,
                        &target.root,
                        &keys,
                    );
                }
                let _ = req.store.sync_history_add(
                    &target.file,
                    &format!(
                        "{}：+{} ~{} -{}",
                        target.agent_name, target.added, target.updated, target.removed
                    ),
                    backup_path.as_deref(),
                    "applied",
                );
                result.steps.push(crate::actions::StepResult {
                    target: target.file.clone(),
                    ok: true,
                    message: format!(
                        "已写入（+{} ~{} -{}）；备份：{}",
                        target.added,
                        target.updated,
                        target.removed,
                        backup_path.as_deref().unwrap_or("（原文件不存在，无需备份）")
                    ),
                });
            }
            Err(e) => {
                result.ok = false;
                result.steps.push(crate::actions::StepResult {
                    target: target.file.clone(),
                    ok: false,
                    message: format!("写入失败：{}", e),
                });
            }
        }
    }

    for target in plan.targets.iter().filter(|t| !t.supported) {
        skipped += 1;
        result.steps.push(crate::actions::StepResult {
            target: target.file.clone(),
            ok: true,
            message: target.reason.clone().unwrap_or_else(|| "已跳过".into()),
        });
    }

    result.summary = format!("完成：写入 {} 个文件，跳过 {} 个", written, skipped);
    result.restore_hint = format!(
        "备份保存在 {}，可在「历史与审计 → 备份」中回滚",
        backup_root.to_string_lossy()
    );
    result.manifest = None;
    result
}

/// 原子写入：先写同目录临时文件再改名，避免半截文件
fn atomic_write(file: &Path, content: &str) -> Result<(), String> {
    let temp = file.with_extension(format!(
        "{}.agenthub-tmp",
        file.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "tmp".into())
    ));
    std::fs::write(&temp, content).map_err(|e| format!("写入临时文件失败：{}", e))?;
    std::fs::rename(&temp, file).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("替换目标文件失败：{}", e)
    })
}

/* -------------------------------------------------------- Provider 分发 */

pub struct ProviderSyncRequest<'a> {
    pub defs: &'a [LoadedDef],
    pub agents: &'a [AgentTarget],
    pub providers: &'a [crate::model::ProviderResource],
    /// 解析密钥明文（只在渲染写入内容的瞬间调用；返回值不落库）
    pub resolve_key: &'a dyn Fn(&str) -> Option<String>,
    pub agent_ids: &'a [String],
    pub overwrite_unmanaged: bool,
    pub store: &'a Store,
}

/// 把一个 provider 资源渲染成条目映射（键可含点号）
fn render_provider_entries(
    providers: &[crate::model::ProviderResource],
    write: &crate::model::ProviderWrite,
    resolve_key: &dyn Fn(&str) -> Option<String>,
    secrets: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, Value> {
    let mut desired: BTreeMap<String, Value> = BTreeMap::new();

    for provider in providers.iter().filter(|p| p.enabled) {
        let mut fields: Map<String, Value> = Map::new();
        for entry in &write.entries {
            let value: Option<Value> = match entry.from.as_str() {
                "baseUrl" => (!provider.base_url.is_empty()).then(|| json!(provider.base_url)),
                "name" => Some(json!(provider.name)),
                "models" => Some(json!(provider.models)),
                "apiKey" => match resolve_key(&provider.key_ref) {
                    Some(secret) => {
                        secrets.push(secret.clone());
                        Some(json!(secret))
                    }
                    None => {
                        warnings.push(format!(
                            "{} 的密钥不在保险库中（或无法解密），该字段已跳过",
                            provider.name
                        ));
                        None
                    }
                },
                other => {
                    warnings.push(format!("未知的字段来源 {}，已忽略", other));
                    None
                }
            };
            if let Some(value) = value {
                if write.object_per_provider {
                    // 以点号路径写进该 provider 的对象里
                    let mut wrapper: Map<String, Value> = Map::new();
                    let segments: Vec<&str> = entry.key.split('.').filter(|s| !s.is_empty()).collect();
                    let mut cursor = &mut wrapper;
                    for segment in &segments[..segments.len().saturating_sub(1)] {
                        cursor = cursor
                            .entry(segment.to_string())
                            .or_insert_with(|| Value::Object(Map::new()))
                            .as_object_mut()
                            .expect("已确保为对象");
                    }
                    if let Some(last) = segments.last() {
                        cursor.insert(last.to_string(), value);
                    }
                } else {
                    fields.insert(entry.key.clone(), value);
                }
            }
        }

        if write.object_per_provider {
            // 逐字段合并进 provider 对象
            let entry = desired
                .entry(provider.name.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(obj) = entry.as_object_mut() {
                for (key, value) in fields {
                    *obj = merge_into_map(std::mem::take(obj), &key, value);
                }
            }
        } else {
            for (key, value) in fields {
                desired.insert(key, value);
            }
        }
    }
    desired
}

/// 把点号路径的键写进一个对象（供 object_per_provider 场景拼装）
fn merge_into_map(mut obj: Map<String, Value>, key: &str, value: Value) -> Map<String, Value> {
    let segments: Vec<&str> = key.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return obj;
    }
    if segments.len() == 1 {
        obj.insert(segments[0].to_string(), value);
        return obj;
    }
    let mut cursor = &mut obj;
    for segment in &segments[..segments.len() - 1] {
        cursor = cursor
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("已确保为对象");
    }
    cursor.insert(segments[segments.len() - 1].to_string(), value);
    obj
}

fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.trim().is_empty() {
            continue;
        }
        out = out.replace(secret, &crate::util::mask_secret(secret));
    }
    out
}

pub fn plan_provider_sync(req: &ProviderSyncRequest) -> Result<SyncPlan, String> {
    let enabled: Vec<&crate::model::ProviderResource> =
        req.providers.iter().filter(|p| p.enabled).collect();
    if enabled.is_empty() {
        return Err("没有启用中的供应商资源可分发".to_string());
    }

    let mut targets: Vec<SyncTargetPlan> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for def in req.defs {
        let agent_id = &def.file.agent.id;
        if !req.agent_ids.is_empty() && !req.agent_ids.iter().any(|id| id == agent_id) {
            continue;
        }
        let Some(agent) = req.agents.iter().find(|a| &a.id == agent_id) else {
            continue;
        };
        if agent.status == "absent" || def.file.provider_write.is_empty() {
            continue;
        }
        if def.file.capabilities.max_tier == "observe" {
            warnings.push(format!("{} 的能力上限是 observe，已跳过", agent.name));
            continue;
        }

        for write in &def.file.provider_write {
            let file = resolve_file(&write.file);
            let strategy = if write.strategy.is_empty() {
                if write.format == "json" {
                    "merge-keys".to_string()
                } else {
                    "managed-block".to_string()
                }
            } else {
                write.strategy.clone()
            };
            let format = if write.format.is_empty() {
                file.extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default()
            } else {
                write.format.clone()
            };

            let mut plan = SyncTargetPlan {
                agent_id: agent_id.clone(),
                agent_name: agent.name.clone(),
                kind: "config".to_string(),
                file: file.to_string_lossy().to_string(),
                root: write.root.clone(),
                format: format.clone(),
                strategy: strategy.clone(),
                supported: strategy == "merge-keys" && format == "json",
                file_exists: file.is_file(),
                ..Default::default()
            };
            if !plan.supported {
                plan.reason = Some(format!(
                    "供应商写入暂只支持 JSON 结构合并（当前策略 {} + 格式 {}）",
                    strategy, format
                ));
                targets.push(plan);
                continue;
            }
            // 平铺式目标（例如 Claude Code 的 env）只能容纳一个供应商
            if !write.object_per_provider && enabled.len() > 1 {
                plan.supported = false;
                plan.reason = Some(format!(
                    "该目标把供应商字段平铺在节点下，只能承载一个供应商，当前启用了 {} 个 —— 请只启用一个后再分发",
                    enabled.len()
                ));
                targets.push(plan);
                continue;
            }

            let existing = if file.is_file() {
                std::fs::read_to_string(&file).unwrap_or_default()
            } else {
                String::new()
            };
            let managed_before =
                req.store
                    .sync_state_get(agent_id, &plan.file, &write.root);

            let mut secrets: Vec<String> = Vec::new();
            let mut local_warnings: Vec<String> = Vec::new();
            let desired = render_provider_entries(
                req.providers,
                write,
                req.resolve_key,
                &mut secrets,
                &mut local_warnings,
            );
            for warning in local_warnings {
                warnings.push(format!("{}：{}", agent.name, warning));
            }

            let outcome = merge::merge_json_keys(
                &existing,
                &write.root,
                &desired,
                &managed_before,
                req.overwrite_unmanaged,
            )?;
            let (added, updated, removed, skipped, unchanged) = outcome.counts();
            for warning in &outcome.warnings {
                warnings.push(format!("{}：{}", agent.name, warning));
            }
            if !secrets.is_empty() {
                warnings.push(format!(
                    "{}：本次会把 {} 个密钥明文写入 {} —— 这是该 Agent 的官方机制；界面上的变更与 diff 已做掩码处理",
                    agent.name,
                    secrets.len(),
                    plan.file
                ));
            }
            plan.changes = outcome
                .changes
                .into_iter()
                .map(|mut change| {
                    change.detail = redact(&change.detail, &secrets);
                    change
                })
                .collect();
            plan.added = added;
            plan.updated = updated;
            plan.removed = removed;
            plan.skipped = skipped;
            plan.unchanged = unchanged;
            plan.diff = redact(&merge::unified_diff(&existing, &outcome.new_text, 3), &secrets);
            plan.backup_path = Some(planned_backup_path(&file).to_string_lossy().to_string());
            targets.push(plan);
        }
    }

    if targets.is_empty() {
        return Err("没有匹配到任何可写入的供应商目标（请确认目标 Agent 已安装且定义里声明了 providerWrite）".to_string());
    }
    let changed = targets
        .iter()
        .filter(|t| t.supported && (t.added + t.updated + t.removed) > 0)
        .count();

    Ok(SyncPlan {
        capability: "file.merge_keys".into(),
        tier: "deploy".into(),
        tier_code: "T2".into(),
        title: "分发供应商到 Agent".into(),
        summary: format!(
            "{} 个供应商资源 → {} 个配置文件（其中 {} 个需要写入）",
            enabled.len(),
            targets.iter().filter(|t| t.supported).count(),
            changed
        ),
        servers: enabled.iter().map(|p| p.name.clone()).collect(),
        targets,
        warnings,
        confirm_hint: "T2 级操作：写入前自动备份目标文件；密钥明文写入后界面与日志均以掩码呈现".into(),
    })
}

pub fn apply_provider_sync(req: &ProviderSyncRequest) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "分发供应商到 Agent".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let plan = match plan_provider_sync(req) {
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

    let mut written = 0usize;
    let mut skipped = 0usize;
    for target in plan.targets.iter().filter(|t| t.supported) {
        if target.added + target.updated + target.removed == 0 {
            skipped += 1;
            result.steps.push(crate::actions::StepResult {
                target: target.file.clone(),
                ok: true,
                message: "已是最新，跳过写入".into(),
            });
            continue;
        }
        let file = PathBuf::from(&target.file);
        let existing = std::fs::read_to_string(&file).unwrap_or_default();

        let Some(write) = req
            .defs
            .iter()
            .find(|d| d.file.agent.id == target.agent_id)
            .and_then(|d| {
                d.file
                    .provider_write
                    .iter()
                    .find(|w| resolve_file(&w.file) == file)
            })
        else {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: target.file.clone(),
                ok: false,
                message: "找不到对应的写入声明".into(),
            });
            continue;
        };

        let mut secrets: Vec<String> = Vec::new();
        let mut ignored: Vec<String> = Vec::new();
        let desired =
            render_provider_entries(req.providers, write, req.resolve_key, &mut secrets, &mut ignored);
        let new_text = match merge::merge_json_keys(
            &existing,
            &write.root,
            &desired,
            &req.store
                .sync_state_get(&target.agent_id, &target.file, &write.root),
            req.overwrite_unmanaged,
        ) {
            Ok(o) => o.new_text,
            Err(e) => {
                result.ok = false;
                result.steps.push(crate::actions::StepResult {
                    target: target.file.clone(),
                    ok: false,
                    message: format!("合并失败：{}", e),
                });
                continue;
            }
        };

        // 备份 → 原子写入 → 记录受管键
        let backup = if file.is_file() {
            let dest = target
                .backup_path
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| planned_backup_path(&file));
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(&file, &dest) {
                Ok(_) => {
                    let bytes = crate::util::file_size(&dest).unwrap_or(0);
                    let _ = req.store.backup_add(
                        &target.file,
                        &dest.to_string_lossy(),
                        bytes,
                        &format!("{} 写入供应商前备份", target.agent_name),
                    );
                    Some(dest.to_string_lossy().to_string())
                }
                Err(e) => {
                    result.ok = false;
                    result.steps.push(crate::actions::StepResult {
                        target: target.file.clone(),
                        ok: false,
                        message: format!("备份失败，已中止写入：{}", e),
                    });
                    continue;
                }
            }
        } else {
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            None
        };

        match atomic_write(&file, &new_text) {
            Ok(_) => {
                written += 1;
                let keys: Vec<String> = desired.keys().cloned().collect();
                let _ = req
                    .store
                    .sync_state_set(&target.agent_id, &target.file, &write.root, &keys);
                let _ = req.store.sync_history_add(
                    &target.file,
                    &format!(
                        "{}：供应商 +{} ~{} -{}",
                        target.agent_name, target.added, target.updated, target.removed
                    ),
                    backup.as_deref(),
                    "applied",
                );
                result.steps.push(crate::actions::StepResult {
                    target: target.file.clone(),
                    ok: true,
                    message: format!(
                        "已写入（+{} ~{} -{}）；备份：{}",
                        target.added,
                        target.updated,
                        target.removed,
                        backup.as_deref().unwrap_or("（原文件不存在）")
                    ),
                });
            }
            Err(e) => {
                result.ok = false;
                result.steps.push(crate::actions::StepResult {
                    target: target.file.clone(),
                    ok: false,
                    message: format!("写入失败：{}", e),
                });
            }
        }
    }

    for target in plan.targets.iter().filter(|t| !t.supported) {
        skipped += 1;
        result.steps.push(crate::actions::StepResult {
            target: target.file.clone(),
            ok: true,
            message: target.reason.clone().unwrap_or_else(|| "已跳过".into()),
        });
    }

    result.summary = format!("完成：写入 {} 个文件，跳过 {} 个", written, skipped);
    result.restore_hint = "备份可在「历史与审计 → 配置备份」中回滚".into();
    result
}

/* -------------------------------------------------------------- 回滚 */

pub fn list_backups(store: &Store, limit: usize) -> Vec<BackupInfo> {
    store.backup_list(limit)
}

/// 用备份覆盖回目标文件（当前内容会被先备份一次）
pub fn restore_backup(store: &Store, id: i64) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "回滚备份".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let Some(info) = store.backup_list(500).into_iter().find(|b| b.id == id) else {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: id.to_string(),
            ok: false,
            message: "备份记录不存在".into(),
        });
        return result;
    };
    let backup = PathBuf::from(&info.backup_path);
    if !backup.is_file() {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: info.backup_path.clone(),
            ok: false,
            message: "备份文件已被删除".into(),
        });
        return result;
    }
    let target = PathBuf::from(&info.target);

    // 回滚前先把当前内容也备份，避免回滚本身不可逆
    if target.is_file() {
        let safety = planned_backup_path(&target);
        if let Some(parent) = safety.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&target, &safety).is_ok() {
            let bytes = crate::util::file_size(&safety).unwrap_or(0);
            let _ = store.backup_add(
                &info.target,
                &safety.to_string_lossy(),
                bytes,
                "回滚前自动备份当前内容",
            );
        }
    }

    match std::fs::copy(&backup, &target) {
        Ok(_) => {
            result.steps.push(crate::actions::StepResult {
                target: info.target.clone(),
                ok: true,
                message: format!("已用 {} 的备份覆盖回目标文件", info.created_at),
            });
            result.summary = format!("已回滚 {}", info.target);
            let _ = store.sync_history_add(
                &info.target,
                "回滚到历史备份",
                Some(&info.backup_path),
                "restored",
            );
        }
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: info.target.clone(),
                ok: false,
                message: format!("回滚失败：{}", e),
            });
        }
    }
    result.restore_hint = "回滚前已自动备份当前内容，可再次回滚".into();
    result
}

