//! 环境档案的导出 / 导入（M1）。
//!
//! 导出文件是自包含 JSON（`.agenthub-profile.json`），统一落在数据目录的
//! `exports/` 子目录；拷给别人即可导入。导入时不携带任何密钥 —— Key 永远
//! 只在保险库里，导出文件里连 key_ref 都不含。

use crate::model::{ProfileItem, ProfileResource};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT_MARKER: &str = "agenthub-profile";
pub const FORMAT_VERSION: u32 = 1;

/// 导出文件结构（对用户可见的形状）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportFile {
    pub format: String,
    pub version: u32,
    pub profile: ProfileExportProfile,
    pub items: Vec<ProfileItem>,
    pub exported_at: String,
    pub app_version: String,
}

/// 导出文件里的档案元信息（不含 id —— 导入方重新编号）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportProfile {
    pub name: String,
    pub description: String,
    pub agents: Vec<String>,
}

/// 导入列表里的一条元信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportMeta {
    pub path: String,
    pub name: String,
    pub description: String,
    pub items: usize,
    /// skill | mcp | provider 各多少项
    pub skill_items: usize,
    pub mcp_items: usize,
    pub provider_items: usize,
    pub agents: usize,
    pub exported_at: String,
    pub bytes: u64,
}

/// 一次导出的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportOutcome {
    pub path: String,
    pub name: String,
    pub items: usize,
    pub exports_dir: String,
}

/// 导出目录：<数据目录>/exports
pub fn exports_dir(store: &Store) -> PathBuf {
    store.data_dir().join("exports")
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "profile".to_string()
    } else {
        trimmed
    }
}

/// 导出一个档案到 exports 目录（同名文件覆盖，原子写入）
pub fn export_profile(store: &Store, id: i64) -> Result<ProfileExportOutcome, String> {
    let dir = exports_dir(store);
    let (path, name, items) = export_profile_into(store, id, &dir)?;
    Ok(ProfileExportOutcome {
        path: path.to_string_lossy().to_string(),
        name,
        items,
        exports_dir: dir.to_string_lossy().to_string(),
    })
}

/// 把档案写进指定目录（迁移包导出复用）。返回 (文件路径, 名称, 项数)。
fn export_profile_into(
    store: &Store,
    id: i64,
    dir: &Path,
) -> Result<(PathBuf, String, usize), String> {
    let detail = store
        .profile_detail(id)
        .ok_or_else(|| "档案不存在（可能已被删除）".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建导出目录失败：{}", e))?;

    let file = ProfileExportFile {
        format: FORMAT_MARKER.to_string(),
        version: FORMAT_VERSION,
        profile: ProfileExportProfile {
            name: detail.profile.name.clone(),
            description: detail.profile.description.clone(),
            agents: detail.profile.agents.clone(),
        },
        items: detail.items.clone(),
        exported_at: crate::util::now_human(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let path = dir.join(format!(
        "{}.agenthub-profile.json",
        safe_name(&detail.profile.name)
    ));
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("序列化失败：{}", e))?;
    crate::sync::atomic_write(&path, &json).map_err(|e| format!("写入失败：{}", e))?;
    Ok((path, detail.profile.name.clone(), detail.items.len()))
}

/// 同 export_profile_into，但文件名用序号（迁移包内多个非 ASCII 同形名称不互相覆盖）
fn export_profile_indexed(
    store: &Store,
    id: i64,
    dir: &Path,
    index: usize,
) -> Result<(PathBuf, String, usize), String> {
    let detail = store
        .profile_detail(id)
        .ok_or_else(|| "档案不存在（可能已被删除）".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建导出目录失败：{}", e))?;

    let file = ProfileExportFile {
        format: FORMAT_MARKER.to_string(),
        version: FORMAT_VERSION,
        profile: ProfileExportProfile {
            name: detail.profile.name.clone(),
            description: detail.profile.description.clone(),
            agents: detail.profile.agents.clone(),
        },
        items: detail.items.clone(),
        exported_at: crate::util::now_human(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let path = dir.join(format!("profile-{:04}.agenthub-profile.json", index));
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("序列化失败：{}", e))?;
    crate::sync::atomic_write(&path, &json).map_err(|e| format!("写入失败：{}", e))?;
    Ok((path, detail.profile.name.clone(), detail.items.len()))
}

/// 扫描导出目录，列出全部可导入的档案文件（解析失败的不出现，计入 failed）
pub fn list_exports(store: &Store) -> (Vec<ProfileExportMeta>, usize) {
    let dir = exports_dir(store);
    let mut metas = Vec::new();
    let mut failed = 0usize;
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return (metas, failed),
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
                && p.file_name()
                    .map(|n| n.to_string_lossy().contains("agenthub-profile"))
                    .unwrap_or(false)
        })
        .collect();
    paths.sort();
    for path in paths {
        match read_export_file(&path) {
            Ok(file) => {
                let skill_items = file
                    .items
                    .iter()
                    .filter(|i| i.resource_type == "skill")
                    .count();
                let mcp_items = file
                    .items
                    .iter()
                    .filter(|i| i.resource_type == "mcp")
                    .count();
                let provider_items = file
                    .items
                    .iter()
                    .filter(|i| i.resource_type == "provider")
                    .count();
                metas.push(ProfileExportMeta {
                    path: path.to_string_lossy().to_string(),
                    name: file.profile.name.clone(),
                    description: file.profile.description.clone(),
                    items: file.items.len(),
                    skill_items,
                    mcp_items,
                    provider_items,
                    agents: file.profile.agents.len(),
                    exported_at: file.exported_at.clone(),
                    bytes: crate::util::file_size(&path).unwrap_or(0),
                });
            }
            Err(_) => failed += 1,
        }
    }
    (metas, failed)
}

fn read_export_file(path: &Path) -> Result<ProfileExportFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取失败：{}", e))?;
    let file: ProfileExportFile =
        serde_json::from_str(&text).map_err(|e| format!("不是有效的档案导出文件：{}", e))?;
    if file.format != FORMAT_MARKER {
        return Err("format 标记不匹配".to_string());
    }
    if file.version > FORMAT_VERSION {
        return Err(format!(
            "文件版本 {} 高于本软件支持的 {}（请升级软件）",
            file.version, FORMAT_VERSION
        ));
    }
    if file.profile.name.trim().is_empty() {
        return Err("档案名称为空".to_string());
    }
    Ok(file)
}

/// 导入一个导出文件。同名档案存在时自动加「（导入）」后缀，不覆盖现有数据。
pub fn import_profile(store: &Store, path: &Path) -> Result<ProfileResource, String> {
    let file = read_export_file(path)?;
    let taken: Vec<String> = store
        .profile_list()
        .into_iter()
        .map(|p| p.name.to_lowercase())
        .collect();
    let base = file.profile.name.trim().to_string();
    let mut name = base.clone();
    let mut suffix = 0usize;
    while taken.contains(&name.to_lowercase()) {
        suffix += 1;
        name = if suffix == 1 {
            format!("{}（导入）", base)
        } else {
            format!("{}（导入 {}）", base, suffix)
        };
    }
    let profile = ProfileResource {
        id: 0,
        name,
        description: file.profile.description.clone(),
        agents: file.profile.agents.clone(),
        counts: Default::default(),
        updated_at: String::new(),
    };
    let id = store
        .profile_save(&profile, &file.items)
        .map_err(|e| format!("导入落库失败：{}", e))?;
    store
        .profile_list()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "导入后读取档案失败".to_string())
}

/* ------------------------------------------------------------ 换机迁移 */

/// 迁移包目录：<数据目录>/exports/migrations/<migration-时间戳>
/// 一个包 = 一个自包含目录：设置 + 全部档案 + 全部自定义 Agent 定义。
/// 密钥（DPAPI 与用户绑定）**永不**进包 —— 换机后需重新录入。

pub const MIGRATION_MARKER: &str = "agenthub-migration";
pub const MIGRATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationManifest {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub app_version: String,
    pub settings_included: bool,
    pub profiles: Vec<String>,
    pub definitions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationOutcome {
    pub path: String,
    pub profile_count: usize,
    pub definition_count: usize,
    pub settings_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationMeta {
    pub path: String,
    pub exported_at: String,
    pub app_version: String,
    pub profile_count: usize,
    pub definition_count: usize,
    pub settings_included: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationImportSummary {
    pub profiles_imported: usize,
    pub definitions_imported: usize,
    pub settings_applied: bool,
    /// 跳过的项及原因（如同名定义已存在）
    pub skipped: Vec<String>,
}

fn migrations_root(store: &Store) -> PathBuf {
    exports_dir(store).join("migrations")
}

/// 导出迁移包：设置（可选）+ 全部档案 + 全部自定义 Agent 定义。
pub fn export_migration(store: &Store, include_settings: bool) -> Result<MigrationOutcome, String> {
    let settings = store.load_settings();
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let bundle = migrations_root(store).join(format!("migration-{}", stamp));
    std::fs::create_dir_all(&bundle).map_err(|e| format!("创建迁移包目录失败：{}", e))?;

    // 1) 设置（换机后首次启动的引导标记不带走，避免重复或跳过引导）
    if include_settings {
        let mut value =
            serde_json::to_value(&settings).map_err(|e| format!("设置序列化失败：{}", e))?;
        if let Some(obj) = value.as_object_mut() {
            obj.remove("onboardingDone");
        }
        let text = serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败：{}", e))?;
        crate::sync::atomic_write(&bundle.join("settings.json"), &text)
            .map_err(|e| format!("写设置失败：{}", e))?;
    }

    // 2) 全部档案（包内用序号命名，避免非 ASCII 名称经 safe_name 后互相覆盖）
    let profiles_dir = bundle.join("profiles");
    let mut profile_names = Vec::new();
    for (index, profile) in store.profile_list().into_iter().enumerate() {
        let (_, name, _) = export_profile_indexed(store, profile.id, &profiles_dir, index)?;
        profile_names.push(name);
    }

    // 3) 自定义 Agent 定义（只带走真改过的；未改动的导出副本目标机上有内置）
    let defs_dir = bundle.join("definitions");
    let mut definition_ids = Vec::new();
    let user_dir = crate::agentdef::user_dir(&settings);
    if user_dir.is_dir() {
        std::fs::create_dir_all(&defs_dir).map_err(|e| format!("创建定义目录失败：{}", e))?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(&user_dir)
            .map_err(|e| format!("读取定义目录失败：{}", e))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|x| x.eq_ignore_ascii_case("toml"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !crate::agentdef::is_custom_definition(&text) {
                continue;
            }
            let dest = defs_dir.join(path.file_name().unwrap_or_default());
            std::fs::copy(&path, &dest).map_err(|e| format!("复制定义失败：{}", e))?;
            definition_ids.push(
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
            );
        }
    }

    // 4) 清单
    let manifest = MigrationManifest {
        format: MIGRATION_MARKER.to_string(),
        version: MIGRATION_VERSION,
        exported_at: crate::util::now_human(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        settings_included: include_settings,
        profiles: profile_names.clone(),
        definitions: definition_ids.clone(),
    };
    let text =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("序列化失败：{}", e))?;
    crate::sync::atomic_write(&bundle.join("migration.json"), &text)
        .map_err(|e| format!("写清单失败：{}", e))?;

    Ok(MigrationOutcome {
        path: bundle.to_string_lossy().to_string(),
        profile_count: profile_names.len(),
        definition_count: definition_ids.len(),
        settings_included: include_settings,
    })
}

/// 列出全部迁移包（按导出时间倒序）
pub fn list_migrations(store: &Store) -> Vec<MigrationMeta> {
    let root = migrations_root(store);
    let mut metas = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return metas;
    };
    let mut bundles: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("migration.json").is_file())
        .collect();
    bundles.sort();
    bundles.reverse();
    for bundle in bundles {
        let Ok(text) = std::fs::read_to_string(bundle.join("migration.json")) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<MigrationManifest>(&text) else {
            continue;
        };
        if manifest.format != MIGRATION_MARKER {
            continue;
        }
        let bytes = dir_size(&bundle);
        metas.push(MigrationMeta {
            path: bundle.to_string_lossy().to_string(),
            exported_at: manifest.exported_at,
            app_version: manifest.app_version,
            profile_count: manifest.profiles.len(),
            definition_count: manifest.definitions.len(),
            settings_included: manifest.settings_included,
            bytes,
        });
    }
    metas
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// 导入迁移包。同名定义已存在时跳过（不覆盖目标机上的手改）；档案走同名后缀逻辑。
pub fn import_migration(
    store: &Store,
    path: &Path,
    include_settings: bool,
    include_profiles: bool,
    include_definitions: bool,
) -> Result<MigrationImportSummary, String> {
    let manifest_path = path.join("migration.json");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("读取迁移包失败：{}", e))?;
    let manifest: MigrationManifest = serde_json::from_str(&text)
        .map_err(|e| format!("迁移包清单损坏：{}", e))?;
    if manifest.format != MIGRATION_MARKER {
        return Err("不是 AgentHub 迁移包".to_string());
    }
    if manifest.version > MIGRATION_VERSION {
        return Err(format!(
            "迁移包版本 {} 高于本软件支持的 {}",
            manifest.version, MIGRATION_VERSION
        ));
    }
    let mut summary = MigrationImportSummary::default();

    // 定义要落到「目标机当前」的定义目录 —— 必须在应用源机设置之前取样，
    // 否则源机的 definitions_dir（旧机器路径）会覆盖目标机的
    let user_dir_before = crate::agentdef::user_dir(&store.load_settings());

    // 1) 设置
    if include_settings && manifest.settings_included {
        let settings_path = path.join("settings.json");
        if settings_path.is_file() {
            let raw = std::fs::read_to_string(&settings_path)
                .map_err(|e| format!("读取设置失败：{}", e))?;
            let mut value: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|e| format!("设置文件损坏：{}", e))?;
            // 保留目标机的引导标记
            let current = store.load_settings();
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "onboardingDone".to_string(),
                    serde_json::json!(current.onboarding_done),
                );
            }
            let settings: crate::model::AppSettings = serde_json::from_value(value)
                .map_err(|e| format!("设置内容不兼容：{}", e))?;
            store
                .save_settings(&settings)
                .map_err(|e| format!("应用设置失败：{}", e))?;
            summary.settings_applied = true;
        } else {
            summary.skipped.push("设置：包里没有 settings.json".to_string());
        }
    }

    // 2) 档案（同名走「（导入）」后缀，永不覆盖）
    if include_profiles {
        let profiles_dir = path.join("profiles");
        if profiles_dir.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&profiles_dir)
                .map_err(|e| format!("读取档案目录失败：{}", e))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| x == "json")
                        .unwrap_or(false)
                        && p.file_name()
                            .map(|n| n.to_string_lossy().contains("agenthub-profile"))
                            .unwrap_or(false)
                })
                .collect();
            files.sort();
            for file in files {
                match import_profile(store, &file) {
                    Ok(_) => summary.profiles_imported += 1,
                    Err(e) => summary
                        .skipped
                        .push(format!("档案 {}：{}", file.display(), e)),
                }
            }
        }
    }

    // 3) 定义（同名跳过，不覆盖目标机手改）
    if include_definitions {
        let defs_dir = path.join("definitions");
        if defs_dir.is_dir() {
            let user_dir = user_dir_before;
            std::fs::create_dir_all(&user_dir)
                .map_err(|e| format!("创建定义目录失败：{}", e))?;
            let mut files: Vec<PathBuf> = std::fs::read_dir(&defs_dir)
                .map_err(|e| format!("读取定义目录失败：{}", e))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| x.eq_ignore_ascii_case("toml"))
                        .unwrap_or(false)
                })
                .collect();
            files.sort();
            for file in files {
                let name = file.file_name().unwrap_or_default();
                let dest = user_dir.join(name);
                if dest.exists() {
                    summary.skipped.push(format!(
                        "定义 {}：目标机已存在（不覆盖手改）",
                        name.to_string_lossy()
                    ));
                    continue;
                }
                std::fs::copy(&file, &dest).map_err(|e| format!("复制定义失败：{}", e))?;
                summary.definitions_imported += 1;
            }
        }
    }

    Ok(summary)
}
