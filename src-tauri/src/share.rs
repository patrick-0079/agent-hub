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
    let detail = store
        .profile_detail(id)
        .ok_or_else(|| "档案不存在（可能已被删除）".to_string())?;
    let dir = exports_dir(store);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败：{}", e))?;

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
    Ok(ProfileExportOutcome {
        path: path.to_string_lossy().to_string(),
        name: detail.profile.name.clone(),
        items: detail.items.len(),
        exports_dir: dir.to_string_lossy().to_string(),
    })
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
