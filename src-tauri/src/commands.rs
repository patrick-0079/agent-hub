//! Tauri 命令层：GUI 的全部后端入口。

use crate::model::{AppSettings, ExecutableInfo, HostInfo, ScanSnapshot, SnapshotMeta, TextPreview};
use crate::scan::{self, Reporter};
use crate::store::Store;
use crate::util;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub store: Arc<Store>,
    pub settings: Mutex<AppSettings>,
    pub scanning: Arc<AtomicBool>,
    /// 密钥保险库（DPAPI 加密；明文永不落库）
    pub vault: Arc<crate::vault::Vault>,
}

/* ------------------------------------------------------------------ 基础 */

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> HostInfo {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let _ = settings;
    let data_dir = state.store.data_dir();
    util::host_info(
        env!("CARGO_PKG_VERSION"),
        &data_dir.to_string_lossy(),
        &data_dir.join("agenthub.db").to_string_lossy(),
    )
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    state
        .store
        .save_settings(&settings)
        .map_err(|e| format!("保存设置失败：{}", e))?;
    if let Ok(mut guard) = state.settings.lock() {
        *guard = settings.clone();
    }
    Ok(settings)
}

#[tauri::command]
pub fn reset_onboarding(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let mut settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    settings.onboarding_done = false;
    state
        .store
        .save_settings(&settings)
        .map_err(|e| format!("保存设置失败：{}", e))?;
    if let Ok(mut guard) = state.settings.lock() {
        *guard = settings.clone();
    }
    Ok(settings)
}

/* ------------------------------------------------------------------ 探测 */

/// 快速探测可执行文件（设置页「工具链探测面板」用，不触发全量扫描）。
#[tauri::command]
pub fn detect_executables(state: State<'_, AppState>) -> Vec<ExecutableInfo> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    scan::executables::scan(&settings)
}

/// 全量扫描：异步执行，过程中通过 `scan:progress` 事件实时推送进度。
#[tauri::command]
pub async fn run_scan(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSnapshot, String> {
    let store = state.store.clone();
    let scanning = state.scanning.clone();
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();

    if scanning.swap(true, Ordering::SeqCst) {
        return Err("已有扫描任务正在进行".to_string());
    }

    let data_dir = store.data_dir();
    let host = util::host_info(
        env!("CARGO_PKG_VERSION"),
        &data_dir.to_string_lossy(),
        &data_dir.join("agenthub.db").to_string_lossy(),
    );

    let app_handle = app.clone();
    let keep = settings.backup_retention.max(3) as usize;

    let result = tauri::async_runtime::spawn_blocking(move || {
        let emit_handle = app_handle.clone();
        let emit = move |p: crate::model::Progress| {
            let _ = emit_handle.emit("scan:progress", p);
        };
        let reporter = Reporter::new(&emit);
        let snapshot = scan::scan(&settings, &host, &reporter);
        if let Err(e) = store.save_snapshot(&snapshot) {
            let _ = app_handle.emit(
                "scan:progress",
                crate::model::Progress {
                    phase: "存储".to_string(),
                    message: format!("快照写入失败：{}", e),
                    current: 0,
                    total: 0,
                    level: "error".to_string(),
                    ts: util::now_human(),
                },
            );
        }
        let _ = store.prune_snapshots(keep.max(10));
        snapshot
    })
    .await;

    scanning.store(false, Ordering::SeqCst);

    match result {
        Ok(snapshot) => Ok(snapshot),
        Err(e) => Err(format!("扫描任务异常终止：{}", e)),
    }
}

#[tauri::command]
pub fn last_snapshot(state: State<'_, AppState>) -> Option<ScanSnapshot> {
    state.store.latest_snapshot()
}

#[tauri::command]
pub fn snapshot_history(state: State<'_, AppState>, limit: Option<usize>) -> Vec<SnapshotMeta> {
    state.store.snapshot_history(limit.unwrap_or(30))
}

/* ------------------------------------------------------------- 文件与预览 */

/// 读取文本预览（Skill 的 SKILL.md、配置文件等），带大小上限。
#[tauri::command]
pub fn read_text_preview(path: String, max_bytes: Option<usize>) -> TextPreview {
    let limit = max_bytes.unwrap_or(200 * 1024);
    let p = PathBuf::from(&path);
    if !p.exists() {
        return TextPreview {
            path,
            exists: false,
            error: Some("文件不存在".to_string()),
            ..Default::default()
        };
    }
    let bytes = util::file_size(&p).unwrap_or(0);
    match std::fs::read(&p) {
        Ok(raw) => {
            let truncated = raw.len() > limit;
            let slice = &raw[..raw.len().min(limit)];
            TextPreview {
                path: path.clone(),
                exists: true,
                bytes,
                truncated,
                text: String::from_utf8_lossy(slice).to_string(),
                error: None,
            }
        }
        Err(e) => TextPreview {
            path,
            exists: true,
            bytes,
            truncated: false,
            text: String::new(),
            error: Some(format!("读取失败：{}", e)),
        },
    }
}

/// 在文件管理器中定位路径（不存在则打开其父目录）。
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let target = if p.exists() {
        p.clone()
    } else {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p.clone())
    };
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.to_string_lossy()));
    }
    #[cfg(windows)]
    {
        let mut cmd = if target.is_dir() {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(&target);
            c
        } else {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(format!("/select,{}", target.to_string_lossy()));
            c
        };
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
        cmd.spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 列出目录内容（用于 Skill 详情中的文件清单）。
#[tauri::command]
pub fn list_dir(path: String, limit: Option<usize>) -> Vec<serde_json::Value> {
    let p = PathBuf::from(&path);
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&p) {
        for entry in entries.flatten().take(limit.unwrap_or(200)) {
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            out.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "isDir": is_dir,
                "size": size,
            }));
        }
    }
    out.sort_by(|a, b| {
        b["isDir"]
            .as_bool()
            .unwrap_or(false)
            .cmp(&a["isDir"].as_bool().unwrap_or(false))
            .then(a["name"].as_str().cmp(&b["name"].as_str()))
    });
    out
}

/* ------------------------------------------------------- Agent 定义与能力 */

/// 内核能力目录（含分级），GUI 直接渲染
#[tauri::command]
pub fn capability_catalog() -> Vec<crate::capability::CapabilitySpec> {
    crate::capability::catalog()
}

/// 已加载的 Agent 定义（内置 + 用户目录覆盖）
#[tauri::command]
pub fn agent_definitions(state: State<'_, AppState>) -> crate::agentdef::DefinitionsView {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    crate::agentdef::definitions_view(&settings)
}

/// 保存用户定义（覆盖同 id 的内置定义）。写入前先校验，并保留 .bak 副本。
#[tauri::command]
pub fn save_agent_definition(
    state: State<'_, AppState>,
    id: String,
    content: String,
) -> Result<crate::agentdef::DefinitionsView, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let parsed = crate::agentdef::parse(&content)?;
    if parsed.agent.id != id {
        return Err(format!(
            "定义中的 agent.id 为「{}」，与要保存的目标「{}」不一致",
            parsed.agent.id, id
        ));
    }
    let dir = crate::agentdef::user_dir(&settings);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建定义目录失败：{}", e))?;
    let target = dir.join(format!("{}.toml", id));
    if target.exists() {
        let _ = std::fs::copy(&target, dir.join(format!("{}.toml.bak", id)));
    }
    std::fs::write(&target, content).map_err(|e| format!("写入失败：{}", e))?;
    Ok(crate::agentdef::definitions_view(&settings))
}

/// 删除用户定义，回退到内置定义
#[tauri::command]
pub fn reset_agent_definition(
    state: State<'_, AppState>,
    id: String,
) -> Result<crate::agentdef::DefinitionsView, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let dir = crate::agentdef::user_dir(&settings);
    let target = dir.join(format!("{}.toml", id));
    if target.exists() {
        std::fs::remove_file(&target).map_err(|e| format!("删除失败：{}", e))?;
    }
    Ok(crate::agentdef::definitions_view(&settings))
}

/// 把内置定义导出到用户目录（不覆盖已有文件）
#[tauri::command]
pub fn seed_agent_definitions(state: State<'_, AppState>) -> Result<usize, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let dir = crate::agentdef::user_dir(&settings);
    crate::agentdef::seed_user_dir(&dir)
}

/* --------------------------------------------- Skill 导入 / 清理 / 删除 */

fn pick_library(
    settings: &crate::model::AppSettings,
    requested: Option<String>,
) -> std::path::PathBuf {
    if let Some(raw) = requested.filter(|s| !s.trim().is_empty()) {
        return crate::agentdef::resolve_path(&raw);
    }
    let libs = crate::actions::resolve_libraries(settings);
    libs.iter()
        .find(|p| p.is_dir())
        .cloned()
        .unwrap_or_else(|| libs[0].clone())
}

#[tauri::command]
pub fn skill_environment(state: State<'_, AppState>) -> crate::actions::SkillEnv {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    crate::actions::skill_env(&settings)
}

/// 发现来源目录下的 Skill（导入前预览用，只读）
#[tauri::command]
pub fn skill_discover(
    state: State<'_, AppState>,
    source: String,
    library: Option<String>,
) -> Result<Vec<crate::actions::DiscoveredSkill>, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let lib = pick_library(&settings, library);
    crate::actions::discover_skills(&crate::agentdef::resolve_path(&source), Some(&lib))
}

#[tauri::command]
pub fn skill_import_plan(
    state: State<'_, AppState>,
    source: String,
    library: Option<String>,
    mode: String,
    names: Vec<String>,
) -> Result<crate::actions::ActionPlan, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let lib = pick_library(&settings, library);
    crate::actions::plan_import(&crate::agentdef::resolve_path(&source), &lib, &mode, &names)
}

#[tauri::command]
pub fn skill_import_apply(
    state: State<'_, AppState>,
    source: String,
    library: Option<String>,
    mode: String,
    names: Vec<String>,
) -> crate::actions::ActionResult {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let lib = pick_library(&settings, library);
    crate::actions::apply_import(&crate::agentdef::resolve_path(&source), &lib, &mode, &names)
}

/// 克隆 Git 仓库到临时目录（T3：网络 + 外部程序）
#[tauri::command]
pub fn git_clone_repo(
    state: State<'_, AppState>,
    url: String,
    proxy: Option<String>,
) -> crate::actions::CloneOutcome {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let effective = proxy.unwrap_or(settings.network_proxy);
    crate::actions::clone_to_temp(&url, &effective)
}

#[tauri::command]
pub fn tmp_cleanup(path: String) -> crate::actions::ActionResult {
    crate::actions::cleanup_tmp(&std::path::PathBuf::from(path))
}

#[tauri::command]
pub fn skill_cleanup_plan(broken: Vec<crate::actions::BrokenRef>) -> crate::actions::ActionPlan {
    crate::actions::plan_cleanup_broken(&broken)
}

#[tauri::command]
pub fn skill_cleanup_apply(broken: Vec<crate::actions::BrokenRef>) -> crate::actions::ActionResult {
    crate::actions::apply_cleanup_broken(&broken)
}

#[tauri::command]
pub fn skill_relink_plan(
    broken: Vec<crate::actions::BrokenRef>,
    new_root: String,
) -> crate::actions::ActionPlan {
    crate::actions::plan_relink(&broken, &crate::agentdef::resolve_path(&new_root))
}

#[tauri::command]
pub fn skill_relink_apply(
    broken: Vec<crate::actions::BrokenRef>,
    new_root: String,
) -> crate::actions::ActionResult {
    crate::actions::apply_relink(&broken, &crate::agentdef::resolve_path(&new_root))
}

#[tauri::command]
pub fn skill_delete_plan(
    state: State<'_, AppState>,
    path: String,
    link_impact: usize,
) -> Result<crate::actions::ActionPlan, String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let libraries = crate::actions::resolve_libraries(&settings);
    crate::actions::plan_delete_skill(
        &crate::agentdef::resolve_path(&path),
        &libraries,
        link_impact,
    )
}

#[tauri::command]
pub fn skill_delete_apply(path: String) -> crate::actions::ActionResult {
    crate::actions::apply_delete_skill(&crate::agentdef::resolve_path(&path))
}

#[tauri::command]
pub fn trash_list() -> Vec<crate::actions::TrashEntry> {
    crate::actions::list_trash()
}

#[tauri::command]
pub fn trash_restore(name: String) -> crate::actions::ActionResult {
    crate::actions::restore_trash(&name)
}

/* -------------------------------------------------------- 回收站管理 */

#[tauri::command]
pub fn trash_stats() -> crate::actions::TrashStats {
    crate::actions::trash_stats()
}

#[tauri::command]
pub fn trash_detail(name: String) -> Result<crate::actions::TrashDetail, String> {
    crate::actions::trash_detail(&name)
}

/// 恢复条目内的部分对象（stored 为空 = 整条恢复）
#[tauri::command]
pub fn trash_restore_items(
    name: String,
    stored: Vec<String>,
) -> crate::actions::ActionResult {
    crate::actions::restore_trash_items(&name, &stored)
}

#[tauri::command]
pub fn trash_purge_plan(names: Vec<String>) -> Result<crate::actions::ActionPlan, String> {
    crate::actions::plan_purge_trash(&names)
}

#[tauri::command]
pub fn trash_purge_apply(names: Vec<String>) -> crate::actions::ActionResult {
    crate::actions::apply_purge_trash(&names)
}

#[tauri::command]
pub fn trash_purge_older_plan(days: u32) -> Result<crate::actions::ActionPlan, String> {
    crate::actions::plan_purge_older_than(days)
}

#[tauri::command]
pub fn trash_purge_older_apply(days: u32) -> crate::actions::ActionResult {
    crate::actions::apply_purge_older_than(days)
}

#[tauri::command]
pub fn manifests_list(limit: Option<usize>) -> Vec<crate::actions::ManifestInfo> {
    crate::actions::list_manifests(limit.unwrap_or(30))
}

#[tauri::command]
pub fn manifest_restore(path: String) -> crate::actions::ActionResult {
    crate::actions::restore_manifest(&std::path::PathBuf::from(path))
}

/* --------------------------------------------- MCP 资源库与分发（T2） */

/// 读取受管 MCP 资源
#[tauri::command]
pub fn mcp_resources(state: State<'_, AppState>) -> Vec<crate::model::McpResource> {
    state.store.mcp_list()
}

/// 新增 / 更新一条 MCP 资源，返回最新列表
#[tauri::command]
pub fn mcp_save(
    state: State<'_, AppState>,
    resource: crate::model::McpResource,
) -> Result<Vec<crate::model::McpResource>, String> {
    if resource.name.trim().is_empty() {
        return Err("MCP 名称不能为空".to_string());
    }
    if resource.transport == "stdio" && resource.command.trim().is_empty() {
        return Err("stdio 型 MCP 必须填写启动命令".to_string());
    }
    if resource.transport != "stdio" && resource.url.trim().is_empty() {
        return Err("http/sse 型 MCP 必须填写 URL".to_string());
    }
    state
        .store
        .mcp_upsert(&resource)
        .map_err(|e| format!("保存失败：{}", e))?;
    Ok(state.store.mcp_list())
}

#[tauri::command]
pub fn mcp_remove(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Vec<crate::model::McpResource>, String> {
    state.store.mcp_delete(id).map_err(|e| format!("删除失败：{}", e))?;
    Ok(state.store.mcp_list())
}

/// 批量导入（从扫描结果导入现有 MCP 条目）
#[tauri::command]
pub fn mcp_import(
    state: State<'_, AppState>,
    items: Vec<crate::model::McpResource>,
) -> Result<Vec<crate::model::McpResource>, String> {
    let mut imported = 0usize;
    for item in items {
        if item.name.trim().is_empty() {
            continue;
        }
        state
            .store
            .mcp_upsert(&item)
            .map_err(|e| format!("导入 {} 失败：{}", item.name, e))?;
        imported += 1;
    }
    if imported == 0 {
        return Err("没有可导入的条目".to_string());
    }
    Ok(state.store.mcp_list())
}

/// 准备分发所需的上下文（定义 + 最近一次扫描的 Agent 状态 + 资源库）
fn sync_context(
    state: &State<'_, AppState>,
    agent_ids: &[String],
) -> Result<(crate::agentdef::Loaded, crate::model::ScanSnapshot, Vec<crate::model::McpResource>), String> {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let loaded = crate::agentdef::load(&settings);
    let snapshot = state
        .store
        .latest_snapshot()
        .ok_or_else(|| "请先执行一次扫描 —— 需要它来判断各 Agent 的安装状态".to_string())?;
    let _ = agent_ids;
    let resources = state.store.mcp_list();
    Ok((loaded, snapshot, resources))
}

/// 生成分发计划（含每个文件的 diff，只读）
#[tauri::command]
pub fn mcp_sync_plan(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> Result<crate::model::SyncPlan, String> {
    let overwrite = overwrite_unmanaged.unwrap_or(false);
    let (loaded, snapshot, resources) = sync_context(&state, &agent_ids)?;
    crate::sync::plan_sync(&crate::sync::SyncRequest {
        defs: &loaded.defs,
        agents: &snapshot.agents,
        resources: &resources,
        agent_ids: &agent_ids,
        overwrite_unmanaged: overwrite,
        store: state.store.as_ref(),
    })
}

/// 执行分发（先备份、再原子写入）
#[tauri::command]
pub fn mcp_sync_apply(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> crate::actions::ActionResult {
    let overwrite = overwrite_unmanaged.unwrap_or(false);
    match sync_context(&state, &agent_ids) {
        Ok((loaded, snapshot, resources)) => crate::sync::apply_sync(&crate::sync::SyncRequest {
            defs: &loaded.defs,
            agents: &snapshot.agents,
            resources: &resources,
            agent_ids: &agent_ids,
            overwrite_unmanaged: overwrite,
            store: state.store.as_ref(),
        }),
        Err(e) => crate::actions::ActionResult {
            ok: false,
            title: "分发 MCP 到 Agent".into(),
            summary: e.clone(),
            steps: vec![crate::actions::StepResult {
                target: "(plan)".into(),
                ok: false,
                message: e,
            }],
            manifest: None,
            restore_hint: String::new(),
            warnings: Vec::new(),
        },
    }
}

/* --------------------------------------------------------------- 备份 */

#[tauri::command]
pub fn backups_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Vec<crate::model::BackupInfo> {
    state.store.backup_list(limit.unwrap_or(50))
}

#[tauri::command]
pub fn backup_restore(state: State<'_, AppState>, id: i64) -> crate::actions::ActionResult {
    crate::sync::restore_backup(&state.store, id)
}

/* ------------------------------------------ 供应商资源库与保险库（T2） */

/// 供应商密钥的保险库标识：与资源名绑定，重命名会另起一条
fn provider_key_ref(name: &str) -> String {
    format!("provider:{}", name)
}

/// 补全密钥状态（是否已存、掩码），明文永不返回
fn enrich_providers(
    mut list: Vec<crate::model::ProviderResource>,
    vault: &crate::vault::Vault,
) -> Vec<crate::model::ProviderResource> {
    for item in list.iter_mut() {
        let key_ref = if item.key_ref.is_empty() {
            provider_key_ref(&item.name)
        } else {
            item.key_ref.clone()
        };
        item.has_key = vault.has(&key_ref);
        item.masked_key = vault.masked(&key_ref);
        item.key_ref = key_ref;
    }
    list
}

#[tauri::command]
pub fn provider_resources(state: State<'_, AppState>) -> Vec<crate::model::ProviderResource> {
    enrich_providers(state.store.provider_list(), &state.vault)
}

/// 保存供应商；`api_key` 为 Some 时写入保险库（空串表示删除密钥）
#[tauri::command]
pub fn provider_save(
    state: State<'_, AppState>,
    resource: crate::model::ProviderResource,
    api_key: Option<String>,
) -> Result<Vec<crate::model::ProviderResource>, String> {
    if resource.name.trim().is_empty() {
        return Err("供应商名称不能为空".to_string());
    }
    let mut payload = resource;
    payload.key_ref = provider_key_ref(&payload.name);
    if let Some(secret) = api_key {
        state
            .vault
            .set(&payload.key_ref, &secret)
            .map_err(|e| format!("写入保险库失败：{}", e))?;
    }
    state
        .store
        .provider_upsert(&payload)
        .map_err(|e| format!("保存失败：{}", e))?;
    Ok(enrich_providers(state.store.provider_list(), &state.vault))
}

#[tauri::command]
pub fn provider_remove(
    state: State<'_, AppState>,
    id: i64,
    remove_key: Option<bool>,
) -> Result<Vec<crate::model::ProviderResource>, String> {
    if let Some(target) = state
        .store
        .provider_list()
        .into_iter()
        .find(|p| p.id == id)
    {
        if remove_key.unwrap_or(true) {
            let key_ref = if target.key_ref.is_empty() {
                provider_key_ref(&target.name)
            } else {
                target.key_ref.clone()
            };
            let _ = state.vault.remove(&key_ref);
        }
    }
    state
        .store
        .provider_delete(id)
        .map_err(|e| format!("删除失败：{}", e))?;
    Ok(enrich_providers(state.store.provider_list(), &state.vault))
}

/// 批量导入（从扫描到的供应商线索导入）
#[tauri::command]
pub fn provider_import(
    state: State<'_, AppState>,
    items: Vec<crate::model::ProviderResource>,
) -> Result<Vec<crate::model::ProviderResource>, String> {
    let mut imported = 0usize;
    for item in items {
        if item.name.trim().is_empty() {
            continue;
        }
        let mut payload = item;
        if payload.key_ref.is_empty() {
            payload.key_ref = provider_key_ref(&payload.name);
        }
        state
            .store
            .provider_upsert(&payload)
            .map_err(|e| format!("导入 {} 失败：{}", payload.name, e))?;
        imported += 1;
    }
    if imported == 0 {
        return Err("没有可导入的条目".to_string());
    }
    Ok(enrich_providers(state.store.provider_list(), &state.vault))
}

/// 显式查看明文（界面需二次确认；不写日志）
#[tauri::command]
pub fn provider_reveal_key(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let target = state
        .store
        .provider_list()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "供应商不存在".to_string())?;
    let key_ref = if target.key_ref.is_empty() {
        provider_key_ref(&target.name)
    } else {
        target.key_ref.clone()
    };
    state
        .vault
        .get(&key_ref)
        .ok_or_else(|| "保险库中没有该密钥，或密文无法解密（可能来自其它用户/机器）".to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub path: String,
    pub count: usize,
    pub healthy: bool,
    pub message: String,
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    let ids = state.vault.ids();
    match state.vault.verify() {
        Ok(count) => VaultStatus {
            path: crate::default_data_dir().join("vault.json").to_string_lossy().to_string(),
            count,
            healthy: true,
            message: format!("{} 个密钥均可解密（DPAPI · 与当前用户绑定）", count),
        },
        Err(e) => VaultStatus {
            path: crate::default_data_dir().join("vault.json").to_string_lossy().to_string(),
            count: ids.len(),
            healthy: false,
            message: e,
        },
    }
}

/* --------------------------------------- 供应商连通性测试（T3 · 网络） */

/// 测试一个供应商的连通性并把结果落库（provider.health 列）。
/// Key 从保险库解密后只在这一次请求的内存中存在。
#[tauri::command]
pub fn provider_test(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::probe::ProviderTestResult, String> {
    let target = state
        .store
        .provider_list()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "供应商不存在".to_string())?;
    let key_ref = if target.key_ref.is_empty() {
        provider_key_ref(&target.name)
    } else {
        target.key_ref.clone()
    };
    let key = state.vault.get(&key_ref);
    let proxy = state
        .settings
        .lock()
        .map(|s| s.network_proxy.clone())
        .unwrap_or_default();

    let mut result = crate::probe::test_provider(
        &target.base_url,
        &target.kind,
        key.as_deref(),
        &proxy,
    );
    result.provider_id = id;
    result.provider_name = target.name.clone();

    // 落库（解析失败也不影响返回结果本身）
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.store.provider_set_health(id, &json);
    }
    Ok(result)
}

/// 查询一个供应商的账户余额并把结果落库（provider.balance 列）。
/// 目前支持 DeepSeek（GET /user/balance）；其它类型返回 unsupported。
#[tauri::command]
pub fn provider_balance_query(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::probe::ProviderBalance, String> {
    let target = state
        .store
        .provider_list()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "供应商不存在".to_string())?;
    let key_ref = if target.key_ref.is_empty() {
        provider_key_ref(&target.name)
    } else {
        target.key_ref.clone()
    };
    let key = state.vault.get(&key_ref);
    let proxy = state
        .settings
        .lock()
        .map(|s| s.network_proxy.clone())
        .unwrap_or_default();

    let mut result = crate::probe::query_balance(
        &target.base_url,
        &target.kind,
        key.as_deref(),
        &proxy,
    );
    result.provider_id = id;
    result.provider_name = target.name.clone();

    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.store.provider_set_balance(id, &json);
    }
    Ok(result)
}

/* --------------------------------------------- 快照对比与档案导出导入 */

/// 测试一个 MCP 服务器的握手健康：真实启动（stdio）或发 HTTP initialize，
/// 结果落库 mcp_server.health。环境变量值里的 %VAR% 引用在展开瞬间注入。
#[tauri::command]
pub fn mcp_test(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::handshake::McpHandshakeResult, String> {
    let target = state
        .store
        .mcp_list()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "MCP 服务器不存在".to_string())?;
    let proxy = state
        .settings
        .lock()
        .map(|s| s.network_proxy.clone())
        .unwrap_or_default();
    let result = crate::handshake::handshake(&target, &proxy);
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.store.mcp_set_health(id, &json);
    }
    Ok(result)
}

/// 对比两份扫描快照（A 为旧、B 为新）
#[tauri::command]
pub fn snapshot_diff(
    state: State<'_, AppState>,
    id_a: i64,
    id_b: i64,
) -> Result<crate::snapdiff::SnapshotDiff, String> {
    let a = state
        .store
        .snapshot_by_id(id_a)
        .ok_or_else(|| "快照 A 不存在".to_string())?;
    let b = state
        .store
        .snapshot_by_id(id_b)
        .ok_or_else(|| "快照 B 不存在".to_string())?;
    let mut diff = crate::snapdiff::diff_snapshots(&a, &b);
    diff.a_id = id_a;
    diff.b_id = id_b;
    Ok(diff)
}

/// Agent 配置同步审计时间线（每次写入留痕：目标、变更摘要、备份、状态）
#[tauri::command]
pub fn sync_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Vec<crate::model::SyncHistoryEntry> {
    state.store.sync_history_list(limit.unwrap_or(50))
}

/* ------------------------------------------- Python 环境创建/删除（T3） */

#[tauri::command]
pub fn python_env_create_plan(
    path: String,
    python: Option<String>,
    manager: Option<String>,
) -> crate::runner::EnvCreatePlan {
    crate::runner::plan_env_create(&path, python.as_deref(), manager.as_deref())
}

#[tauri::command]
pub fn python_env_create_run(
    state: State<'_, AppState>,
    path: String,
    python: Option<String>,
    manager: Option<String>,
) -> crate::actions::ActionResult {
    crate::runner::apply_env_create(&state.store, &path, python.as_deref(), manager.as_deref())
}

/// 受管 Python 环境清单（与扫描结果在界面上合并）
#[tauri::command]
pub fn python_env_managed(state: State<'_, AppState>) -> Vec<crate::model::PythonEnv> {
    state.store.python_env_managed()
}

/// 删除受管环境：目录整体移入回收站（可恢复）并清除记录
#[tauri::command]
pub fn python_env_remove(
    state: State<'_, AppState>,
    path: String,
) -> crate::actions::ActionResult {
    crate::runner::apply_env_remove(&state.store, &path)
}

/// 模板试渲染（file.render 内核原语的 GUI 入口）：
/// 模板 + JSON 上下文 → 渲染文本；纯函数，不碰磁盘
#[tauri::command]
pub fn template_render(
    template: String,
    context_json: String,
) -> crate::template::TemplateRenderResult {
    crate::template::render_checked(&template, &context_json)
}

/* --------------------------------------------- npm 全局包安装/卸载（T3） */

#[tauri::command]
pub fn npm_install_plan(
    manager: String,
    packages: Vec<String>,
) -> crate::runner::NpmInstallPlan {
    crate::runner::plan_npm_install(&manager, &packages)
}

#[tauri::command]
pub fn npm_install_run(
    state: State<'_, AppState>,
    manager: String,
    packages: Vec<String>,
) -> Result<crate::actions::ActionResult, String> {
    let exe = crate::runner::resolve_package_manager(&manager)
        .ok_or_else(|| format!("未找到 {}（PATH 与兜底目录均未命中）", manager))?;
    Ok(crate::runner::apply_npm_install(
        &state.store,
        &exe,
        &manager,
        &packages,
    ))
}

#[tauri::command]
pub fn npm_remove_plan(manager: String, package: String) -> crate::runner::NpmInstallPlan {
    crate::runner::plan_npm_remove(&manager, &package)
}

#[tauri::command]
pub fn npm_remove_run(
    state: State<'_, AppState>,
    manager: String,
    package: String,
) -> Result<crate::actions::ActionResult, String> {
    let exe = crate::runner::resolve_package_manager(&manager)
        .ok_or_else(|| format!("未找到 {}（PATH 与兜底目录均未命中）", manager))?;
    Ok(crate::runner::apply_npm_remove(
        &state.store,
        &exe,
        &manager,
        &package,
    ))
}

/// 全局包可更新清单（npm outdated -g --json；只读）
#[tauri::command]
pub fn npm_outdated(manager: String) -> Result<Vec<crate::runner::NpmOutdated>, String> {
    let exe = crate::runner::resolve_package_manager(&manager)
        .ok_or_else(|| format!("未找到 {}（PATH 与兜底目录均未命中）", manager))?;
    crate::runner::npm_outdated_list(&exe, &manager)
}

/// 导出档案到数据目录 exports/（自包含 JSON，可拷给他人导入）
#[tauri::command]
pub fn profile_export(
    state: State<'_, AppState>,
    id: i64,
) -> Result<crate::share::ProfileExportOutcome, String> {
    crate::share::export_profile(&state.store, id)
}

/// 列出 exports/ 目录里可导入的档案文件
#[tauri::command]
pub fn profile_export_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::share::ProfileExportMeta>, String> {
    let (mut metas, failed) = crate::share::list_exports(&state.store);
    let _ = failed;
    metas.sort_by(|a, b| b.exported_at.cmp(&a.exported_at));
    Ok(metas)
}

/// 从导出文件导入档案（同名自动加后缀，不覆盖现有数据）
#[tauri::command]
pub fn profile_import(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::model::ProfileDetail, String> {
    let imported = crate::share::import_profile(&state.store, std::path::Path::new(&path))?;
    state
        .store
        .profile_detail(imported.id)
        .ok_or_else(|| "导入成功但读取详情失败".to_string())
}

/* ------------------------------------------------------- 换机迁移（M4） */

/// 导出迁移包：设置（可选）+ 全部档案 + 全部自定义 Agent 定义。
/// 密钥与 DPAPI 绑定当前用户，永不进包 —— 换机后重新录入。
#[tauri::command]
pub fn migration_export(
    state: State<'_, AppState>,
    include_settings: Option<bool>,
) -> Result<crate::share::MigrationOutcome, String> {
    crate::share::export_migration(&state.store, include_settings.unwrap_or(true))
}

/// 列出全部迁移包（按导出时间倒序）
#[tauri::command]
pub fn migration_list(
    state: State<'_, AppState>,
) -> Vec<crate::share::MigrationMeta> {
    crate::share::list_migrations(&state.store)
}

/// 导入迁移包（各部分可选；同名定义跳过不覆盖，档案走同名后缀）
#[tauri::command]
pub fn migration_import(
    state: State<'_, AppState>,
    path: String,
    include_settings: Option<bool>,
    include_profiles: Option<bool>,
    include_definitions: Option<bool>,
) -> Result<crate::share::MigrationImportSummary, String> {
    // 导入设置后刷新内存中的副本（代理等立即生效）
    let summary = crate::share::import_migration(
        &state.store,
        std::path::Path::new(&path),
        include_settings.unwrap_or(true),
        include_profiles.unwrap_or(true),
        include_definitions.unwrap_or(true),
    )?;
    if summary.settings_applied {
        if let Ok(mut guard) = state.settings.lock() {
            *guard = state.store.load_settings();
        }
    }
    Ok(summary)
}

/* --------------------------------------------------- 供应商分发（T2） */

fn provider_sync_context(
    state: &State<'_, AppState>,
    agent_ids: &[String],
) -> Result<
    (
        crate::agentdef::Loaded,
        crate::model::ScanSnapshot,
        Vec<crate::model::ProviderResource>,
    ),
    String,
> {
    let _ = agent_ids;
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let loaded = crate::agentdef::load(&settings);
    let snapshot = state
        .store
        .latest_snapshot()
        .ok_or_else(|| "请先执行一次扫描 —— 需要它来判断各 Agent 的安装状态".to_string())?;
    let providers = enrich_providers(state.store.provider_list(), &state.vault);
    Ok((loaded, snapshot, providers))
}

#[tauri::command]
pub fn provider_sync_plan(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> Result<crate::model::SyncPlan, String> {
    let (loaded, snapshot, providers) = provider_sync_context(&state, &agent_ids)?;
    let vault = state.vault.clone();
    let resolve = move |key_ref: &str| vault.get(key_ref);
    crate::sync::plan_provider_sync(&crate::sync::ProviderSyncRequest {
        defs: &loaded.defs,
        agents: &snapshot.agents,
        providers: &providers,
        resolve_key: &resolve,
        agent_ids: &agent_ids,
        overwrite_unmanaged: overwrite_unmanaged.unwrap_or(false),
        store: state.store.as_ref(),
    })
}

#[tauri::command]
pub fn provider_sync_apply(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> crate::actions::ActionResult {
    let fail = |message: String| crate::actions::ActionResult {
        ok: false,
        title: "分发供应商到 Agent".into(),
        summary: message.clone(),
        steps: vec![crate::actions::StepResult {
            target: "(plan)".into(),
            ok: false,
            message,
        }],
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    match provider_sync_context(&state, &agent_ids) {
        Ok((loaded, snapshot, providers)) => {
            let vault = state.vault.clone();
            let resolve = move |key_ref: &str| vault.get(key_ref);
            crate::sync::apply_provider_sync(&crate::sync::ProviderSyncRequest {
                defs: &loaded.defs,
                agents: &snapshot.agents,
                providers: &providers,
                resolve_key: &resolve,
                agent_ids: &agent_ids,
                overwrite_unmanaged: overwrite_unmanaged.unwrap_or(false),
                store: state.store.as_ref(),
            })
        }
        Err(e) => fail(e),
    }
}

/* ------------------------------------------------- Profile 档案（T2） */

#[tauri::command]
pub fn profile_list(state: State<'_, AppState>) -> Vec<crate::model::ProfileResource> {
    state.store.profile_list()
}

#[tauri::command]
pub fn profile_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Option<crate::model::ProfileDetail> {
    state.store.profile_detail(id)
}

#[tauri::command]
pub fn profile_save(
    state: State<'_, AppState>,
    profile: crate::model::ProfileResource,
    items: Vec<crate::model::ProfileItem>,
) -> Result<Vec<crate::model::ProfileResource>, String> {
    if profile.name.trim().is_empty() {
        return Err("档案名称不能为空".to_string());
    }
    state
        .store
        .profile_save(&profile, &items)
        .map_err(|e| format!("保存档案失败：{}", e))?;
    Ok(state.store.profile_list())
}

#[tauri::command]
pub fn profile_delete(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Vec<crate::model::ProfileResource>, String> {
    state
        .store
        .profile_delete(id)
        .map_err(|e| format!("删除档案失败：{}", e))?;
    Ok(state.store.profile_list())
}

/// 把档案里的资源项解析成真实资源（找不到的会被跳过并计入告警）
fn resolve_profile_resources(
    state: &State<'_, AppState>,
    profile_id: i64,
) -> Result<
    (
        crate::agentdef::Loaded,
        crate::model::ScanSnapshot,
        Vec<crate::model::McpResource>,
        Vec<crate::model::ProviderResource>,
        Vec<crate::profile::ProfileSkill>,
        Vec<String>,
    ),
    String,
> {
    let detail = state
        .store
        .profile_detail(profile_id)
        .ok_or_else(|| "档案不存在".to_string())?;
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let loaded = crate::agentdef::load(&settings);
    let snapshot = state
        .store
        .latest_snapshot()
        .ok_or_else(|| "请先执行一次扫描 —— 需要它来判断各 Agent 的安装状态".to_string())?;

    let all_mcp = state.store.mcp_list();
    let all_providers = enrich_providers(state.store.provider_list(), &state.vault);
    let mut warnings: Vec<String> = Vec::new();
    let mut mcp: Vec<crate::model::McpResource> = Vec::new();
    let mut providers: Vec<crate::model::ProviderResource> = Vec::new();
    let mut skills: Vec<crate::profile::ProfileSkill> = Vec::new();

    for item in &detail.items {
        match item.resource_type.as_str() {
            "mcp" => match all_mcp.iter().find(|r| r.name == item.resource_ref) {
                Some(found) => mcp.push(found.clone()),
                None => warnings.push(format!("MCP 资源「{}」已不存在，已跳过", item.resource_ref)),
            },
            "provider" => match all_providers.iter().find(|p| p.name == item.resource_ref) {
                Some(found) => providers.push(found.clone()),
                None => warnings.push(format!("供应商「{}」已不存在，已跳过", item.resource_ref)),
            },
            "skill" => {
                let path = std::path::PathBuf::from(&item.resource_ref);
                if !path.is_dir() {
                    warnings.push(format!("Skill 路径不存在，已跳过：{}", item.resource_ref));
                    continue;
                }
                skills.push(crate::profile::ProfileSkill {
                    path: item.resource_ref.clone(),
                    name: item.display.clone(),
                });
            }
            other => warnings.push(format!("未知的资源类型：{}", other)),
        }
    }

    Ok((loaded, snapshot, mcp, providers, skills, warnings))
}

#[tauri::command]
pub fn profile_apply_plan(
    state: State<'_, AppState>,
    profile_id: i64,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> Result<crate::model::SyncPlan, String> {
    let (loaded, snapshot, mcp, providers, skills, warnings) =
        resolve_profile_resources(&state, profile_id)?;
    let vault = state.vault.clone();
    let resolve = move |key_ref: &str| vault.get(key_ref);
    let mut plan = crate::profile::plan_profile_apply(&crate::profile::ProfileApplyRequest {
        defs: &loaded.defs,
        agents: &snapshot.agents,
        mcp_resources: &mcp,
        providers: &providers,
        skills: &skills,
        resolve_key: &resolve,
        agent_ids: &agent_ids,
        overwrite_unmanaged: overwrite_unmanaged.unwrap_or(false),
        store: state.store.as_ref(),
    })?;
    plan.warnings.extend(warnings);
    Ok(plan)
}

#[tauri::command]
pub fn profile_apply_run(
    state: State<'_, AppState>,
    profile_id: i64,
    agent_ids: Vec<String>,
    overwrite_unmanaged: Option<bool>,
) -> crate::actions::ActionResult {
    match resolve_profile_resources(&state, profile_id) {
        Ok((loaded, snapshot, mcp, providers, skills, warnings)) => {
            let vault = state.vault.clone();
            let resolve = move |key_ref: &str| vault.get(key_ref);
            let mut result =
                crate::profile::apply_profile_apply(&crate::profile::ProfileApplyRequest {
                    defs: &loaded.defs,
                    agents: &snapshot.agents,
                    mcp_resources: &mcp,
                    providers: &providers,
                    skills: &skills,
                    resolve_key: &resolve,
                    agent_ids: &agent_ids,
                    overwrite_unmanaged: overwrite_unmanaged.unwrap_or(false),
                    store: state.store.as_ref(),
                });
            result.warnings.extend(warnings);
            result
        }
        Err(e) => crate::actions::ActionResult {
            ok: false,
            title: "应用环境档案".into(),
            summary: e.clone(),
            steps: vec![crate::actions::StepResult {
                target: "(plan)".into(),
                ok: false,
                message: e,
            }],
            manifest: None,
            restore_hint: String::new(),
            warnings: Vec::new(),
        },
    }
}

/* --------------------------------------------------------------- 前端启动 */

#[tauri::command]
pub fn frontend_ready(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}