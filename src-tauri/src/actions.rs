//! T2 部署 / T3 变更动作层。
//!
//! 与 `capability.rs` 的只读求值分开：这里全是**会改磁盘**的操作，统一遵守三条不变式：
//!
//! 1. **先出计划**：每个操作都能 `plan_*` 出「将发生什么」，界面必须展示后才允许执行
//! 2. **先留凭据**：动手前写 manifest JSON（记录每一步的操作、来源、目标），据此可恢复
//! 3. **删除不毁灭**：Skill 目录是**移入回收站**而非彻底删除；链接删除只删链接本身，
//!    绝不跟随目标；且只允许删除链接、或位于已声明库目录内的内容

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/* --------------------------------------------------------------- 视图结构 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    /// create-link | copy-dir | delete-link | move-to-trash | clone-repo | skip
    pub action: String,
    pub target: String,
    pub source: Option<String>,
    pub detail: String,
    /// safe | destructive
    pub risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlan {
    pub capability: String,
    pub tier: String,
    pub tier_code: String,
    pub title: String,
    pub summary: String,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
    /// 需要在界面上二次确认的提示语
    pub confirm_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub target: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub ok: bool,
    pub title: String,
    pub summary: String,
    pub steps: Vec<StepResult>,
    /// manifest 路径（可用于恢复）
    pub manifest: Option<String>,
    pub restore_hint: String,
    pub warnings: Vec<String>,
}

impl ActionResult {
    fn new(title: impl Into<String>) -> Self {
        Self {
            ok: true,
            title: title.into(),
            summary: String::new(),
            steps: Vec::new(),
            manifest: None,
            restore_hint: String::new(),
            warnings: Vec::new(),
        }
    }

    fn step(&mut self, target: impl Into<String>, ok: bool, message: impl Into<String>) {
        if !ok {
            self.ok = false;
        }
        self.steps.push(StepResult {
            target: target.into(),
            ok,
            message: message.into(),
        });
    }
}

/* ------------------------------------------------------------ 数据目录 */

pub fn data_dir() -> PathBuf {
    crate::default_data_dir()
}

fn trash_root() -> PathBuf {
    data_dir().join("trash")
}

fn manifest_root() -> PathBuf {
    data_dir().join("manifests")
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/* ------------------------------------------------------------ 环境信息 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub path: String,
    pub exists: bool,
    pub skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestInfo {
    pub path: String,
    pub op: String,
    pub created_at: String,
    pub summary: String,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEnv {
    pub libraries: Vec<LibraryInfo>,
    pub git: Option<String>,
    pub trash: Vec<TrashEntry>,
    pub trash_dir: String,
    pub manifest_dir: String,
    pub tmp_dir: String,
    pub proxy: String,
}

pub fn default_libraries() -> Vec<String> {
    vec![
        "~/.agent/skills".to_string(),
        "~/.agents/skills".to_string(),
        "~/.config/opencode/skills".to_string(),
    ]
}

/// 解析技能库目录（导入目标 / 删除白名单）
pub fn resolve_libraries(settings: &crate::model::AppSettings) -> Vec<PathBuf> {
    let raw = if settings.skill_libraries.is_empty() {
        default_libraries()
    } else {
        settings.skill_libraries.clone()
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for item in raw {
        let p = crate::agentdef::resolve_path(&item);
        if !out.iter().any(|x| x == &p) {
            out.push(p);
        }
    }
    out
}

pub fn tmp_root() -> PathBuf {
    data_dir().join("tmp")
}

pub fn skill_env(settings: &crate::model::AppSettings) -> SkillEnv {
    let libraries = resolve_libraries(settings)
        .into_iter()
        .map(|p| {
            let skill_count = std::fs::read_dir(&p)
                .map(|it| {
                    it.flatten()
                        .filter(|e| {
                            let c = e.path();
                            c.is_dir()
                                && (c.join("SKILL.md").is_file() || c.join("skill.md").is_file())
                        })
                        .count()
                })
                .unwrap_or(0);
            LibraryInfo {
                exists: p.is_dir(),
                path: p.to_string_lossy().to_string(),
                skill_count,
            }
        })
        .collect();

    SkillEnv {
        libraries,
        git: git_available(),
        trash: list_trash(),
        trash_dir: trash_root().to_string_lossy().to_string(),
        manifest_dir: manifest_root().to_string_lossy().to_string(),
        tmp_dir: tmp_root().to_string_lossy().to_string(),
        proxy: settings.network_proxy.clone(),
    }
}

pub fn list_manifests(limit: usize) -> Vec<ManifestInfo> {
    let mut out: Vec<ManifestInfo> = Vec::new();
    let Ok(entries) = std::fs::read_dir(manifest_root()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(manifest) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Manifest>(&t).ok())
        {
            out.push(ManifestInfo {
                path: path.to_string_lossy().to_string(),
                op: manifest.op,
                created_at: manifest.created_at,
                summary: manifest.summary,
                entries: manifest.entries.len(),
            });
        }
    }
    out.sort_by(|a, b| b.path.cmp(&a.path));
    out.truncate(limit);
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneOutcome {
    pub result: ActionResult,
    /// 克隆成功的临时目录路径（供后续导入使用）
    pub path: Option<String>,
}

/// 克隆到我们自己的临时目录；返回路径供后续导入使用
pub fn clone_to_temp(url: &str, proxy: &str) -> CloneOutcome {
    let dest = tmp_root().join(format!("git-{}", timestamp()));
    let result = clone_repo(url, proxy, &dest);
    let path = if result.ok {
        Some(dest.to_string_lossy().to_string())
    } else {
        None
    };
    CloneOutcome { result, path }
}

/// 清理我们自己的临时克隆目录（只允许操作 tmp 根目录下的内容）
pub fn cleanup_tmp(path: &Path) -> ActionResult {
    let mut result = ActionResult::new("清理临时目录");
    let root = tmp_root();
    if !path.starts_with(&root) {
        result.step(
            path.to_string_lossy().to_string(),
            false,
            "拒绝清理：只允许清理 AgentHub 自己的临时目录",
        );
        return result;
    }
    match std::fs::remove_dir_all(path) {
        Ok(_) => {
            result.step(path.to_string_lossy().to_string(), true, "已删除临时目录");
            result.summary = "临时目录已清理".into();
        }
        Err(e) => result.step(path.to_string_lossy().to_string(), false, format!("删除失败：{}", e)),
    }
    result.restore_hint = "临时目录仅用于克隆，删除后可从来源重新导入".into();
    result
}

/* -------------------------------------------------------- 发现 Skill */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredSkill {
    pub name: String,
    pub path: String,
    pub has_manifest: bool,
    pub description: Option<String>,
    pub file_count: usize,
    pub bytes: u64,
    /// 目标库中是否已存在同名 Skill
    pub conflict: bool,
    /// 是否为链接（导入时按链接处理）
    pub is_link: bool,
}

/// 在一个来源目录中发现可导入的 Skill。
///
/// 支持三种形态：
/// * 目录本身就是一个 Skill（含 SKILL.md）
/// * 目录下若干子目录各是一个 Skill
/// * 目录下含有 `skills/` 容器（如 anthropics/skills 仓库）
pub fn discover_skills(source: &Path, library: Option<&Path>) -> Result<Vec<DiscoveredSkill>, String> {
    if !source.is_dir() {
        return Err(format!("来源目录不存在：{}", source.to_string_lossy()));
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if source.join("SKILL.md").is_file() || source.join("skill.md").is_file() {
        roots.push(source.to_path_buf());
    } else if source.join("skills").is_dir() {
        roots.push(source.join("skills"));
    } else {
        roots.push(source.to_path_buf());
    }

    let mut out: Vec<DiscoveredSkill> = Vec::new();
    for root in roots {
        let is_single = root == source && (source.join("SKILL.md").is_file() || source.join("skill.md").is_file());
        if is_single {
            out.push(describe_skill(&root, library)?);
            continue;
        }
        let entries = std::fs::read_dir(&root)
            .map_err(|e| format!("无法读取 {}：{}", root.to_string_lossy(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !path.is_dir() {
                continue;
            }
            let has_manifest = path.join("SKILL.md").is_file() || path.join("skill.md").is_file();
            if has_manifest {
                out.push(describe_skill(&path, library)?);
            }
        }
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name);
    if out.is_empty() {
        return Err("该目录下没有发现任何包含 SKILL.md 的 Skill".to_string());
    }
    Ok(out)
}

fn describe_skill(path: &Path, library: Option<&Path>) -> Result<DiscoveredSkill, String> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let manifest = ["SKILL.md", "skill.md"]
        .iter()
        .map(|m| path.join(m))
        .find(|p| p.is_file());
    // 先取布尔值，manifest 随后会被 and_then 消耗
    let has_manifest = manifest.is_some();
    let description = manifest.and_then(|m| {
        std::fs::read_to_string(&m).ok().map(|text| {
            let (meta, _) = crate::scan::skills::parse_frontmatter(&text);
            meta.get("description").cloned().unwrap_or_default()
        })
    });
    let (file_count, bytes) = crate::util::dir_stats(path, 5000);
    let conflict = library
        .map(|lib| {
            let candidate = lib.join(&name);
            candidate.exists() && !is_link(&candidate)
        })
        .unwrap_or(false);
    Ok(DiscoveredSkill {
        name,
        path: path.to_string_lossy().to_string(),
        has_manifest,
        description,
        file_count,
        bytes,
        conflict,
        is_link: is_link(path),
    })
}

/* -------------------------------------------------------------- 链接操作 */

pub fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// 传给 cmd.exe 的路径必须是本地分隔符：cmd 会把 `/` 当成开关分隔符
/// （曾导致 `mklink` 报 `Invalid switch - "000-xxx"`）
fn native_path(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    if cfg!(windows) {
        s.replace('/', "\\")
    } else {
        s
    }
}

/// 创建目录链接。Windows 优先用 junction（**无需管理员权限**），失败再退回符号链接。
pub fn create_dir_link(link: &Path, target: &Path) -> Result<(), String> {
    if link.exists() || is_link(link) {
        return Err(format!("目标位置已存在：{}", link.to_string_lossy()));
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建父目录失败：{}", e))?;
    }

    #[cfg(windows)]
    {
        // mklink /J 创建 junction：不要求开发者模式；路径统一为本地分隔符
        let out = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(native_path(link))
            .arg(native_path(target))
            .output()
            .map_err(|e| format!("调用 mklink 失败：{}", e))?;
        if out.status.success() {
            return Ok(());
        }
        // 退回符号链接（需要开发者模式或管理员）
        match std::os::windows::fs::symlink_dir(target, link) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "创建链接失败：{}（mklink 输出：{}）",
                e,
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).map_err(|e| format!("创建符号链接失败：{}", e))
    }
}

/// 只删除链接本身，绝不跟随目标
pub fn remove_link(path: &Path) -> Result<(), String> {
    if !is_link(path) {
        return Err(format!(
            "拒绝删除：{} 不是符号链接/junction（避免误删真实目录）",
            path.to_string_lossy()
        ));
    }
    #[cfg(windows)]
    {
        // junction 与目录符号链接都用 remove_dir 删链接本身
        std::fs::remove_dir(path)
            .or_else(|_| std::fs::remove_file(path))
            .map_err(|e| format!("删除链接失败：{}", e))
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(path).map_err(|e| format!("删除链接失败：{}", e))
    }
}

/* -------------------------------------------------------------- 目录复制 */

/// 跨卷安全的目录移动：同卷用 rename，跨卷回退为「复制 + 删除」
fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| format!("清理原目录失败：{}", e))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(usize, u64), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("创建目录失败：{}", e))?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src, dst)) = stack.pop() {
        for entry in std::fs::read_dir(&src)
            .map_err(|e| format!("读取 {} 失败：{}", src.to_string_lossy(), e))?
            .flatten()
        {
            let sp = entry.path();
            let dp = dst.join(entry.file_name());
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.is_dir() {
                std::fs::create_dir_all(&dp).map_err(|e| e.to_string())?;
                stack.push((sp, dp));
            } else {
                std::fs::copy(&sp, &dp).map_err(|e| {
                    format!("复制 {} 失败：{}", sp.to_string_lossy(), e)
                })?;
                files += 1;
                bytes += meta.len();
            }
        }
    }
    Ok((files, bytes))
}

/* ------------------------------------------------------------ manifest */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    /// create-link | copy-dir | delete-link | move-to-trash | failed
    pub action: String,
    pub target: String,
    pub source: Option<String>,
    /// 原路径（用于恢复）
    pub original: Option<String>,
    /// 在回收站条目内的相对路径（仅 move-to-trash）
    #[serde(default)]
    pub stored: Option<String>,
    /// link | dir
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub op: String,
    pub created_at: String,
    pub summary: String,
    pub entries: Vec<ManifestEntry>,
}

fn write_manifest(op: &str, summary: &str, entries: Vec<ManifestEntry>) -> Option<String> {
    let dir = manifest_root();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}-{}.json", timestamp(), op));
    let manifest = Manifest {
        op: op.to_string(),
        created_at: crate::util::now_human(),
        summary: summary.to_string(),
        entries,
    };
    let json = serde_json::to_string_pretty(&manifest).ok()?;
    std::fs::write(&path, json).ok()?;
    Some(path.to_string_lossy().to_string())
}

/* -------------------------------------------------------------- 回收站 */

/// 回收站条目内的一个被删对象
struct TrashItem {
    path: PathBuf,
    /// link | dir
    kind: String,
}

/// 对象身份：父目录的规范化路径 + 文件名（大小写不敏感）。
///
/// 用于识别「同一对象的不同路径别名」—— 例如 `~/.config/opencode/skills` 是指向
/// `~/.agent/skills` 的 junction 时，两条路径下的同一个链接其实是同一个对象。
fn identity_key(path: &Path) -> String {
    let parent = path
        .parent()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    format!("{}\u{1}{}", parent.to_string_lossy().to_ascii_lowercase(), name)
}

fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    if cleaned.is_empty() {
        "item".to_string()
    } else {
        cleaned
    }
}

/// 移动链接本身（不是它的目标）。跨卷时退化为「在新位置重建链接 + 删除原链接」。
fn move_link(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目标目录失败：{}", e))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // 跨卷：读出原目标后在回收站里重建同样的链接，再删掉原链接
    let target = std::fs::read_link(from).map_err(|e| format!("读取链接目标失败：{}", e))?;
    create_dir_link(to, &target)?;
    remove_link(from)
}

/// 公开的单对象删除入口（供 runner 等模块复用）：
/// 链接或目录整体移入回收站，返回 (条目名, 清单路径, 清单条目)。
pub fn move_path_to_trash(
    path: &Path,
    op: &str,
    summary: &str,
) -> Result<(String, String, Vec<ManifestEntry>), String> {
    let kind = if is_link(path) { "link" } else { "dir" };
    move_to_trash(
        vec![TrashItem {
            path: path.to_path_buf(),
            kind: kind.to_string(),
        }],
        op,
        summary,
    )
}

/// 把若干对象移入**同一个**回收站条目（批量删除只产生一条记录，便于整体恢复）。
///
/// 返回 (条目名, 清单路径, 清单条目)
fn move_to_trash(
    items: Vec<TrashItem>,
    op: &str,
    summary: &str,
) -> Result<(String, String, Vec<ManifestEntry>), String> {
    if items.is_empty() {
        return Err("没有需要移入回收站的对象".to_string());
    }
    let hint = items
        .first()
        .and_then(|i| i.path.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| op.to_string());
    let entry_name = format!("{}-{}", timestamp(), sanitize_name(&hint));
    let entry_dir = trash_root().join(&entry_name);
    let items_dir = entry_dir.join("items");
    std::fs::create_dir_all(&items_dir)
        .map_err(|e| format!("创建回收站目录失败：{}", e))?;

    let mut entries: Vec<ManifestEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();
    let mut stored_index = 0usize;
    for item in items.iter() {
        // 同一对象经由不同路径别名出现时只处理一次
        if !seen.insert(identity_key(&item.path)) {
            entries.push(ManifestEntry {
                action: "skipped-alias".to_string(),
                target: item.path.to_string_lossy().to_string(),
                source: None,
                original: Some(item.path.to_string_lossy().to_string()),
                stored: None,
                kind: Some(item.kind.clone()),
            });
            continue;
        }
        let leaf = item
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("item-{}", stored_index));
        let stored_rel = PathBuf::from("items").join(format!("{:03}-{}", stored_index, sanitize_name(&leaf)));
        let stored_rel_str = stored_rel.to_string_lossy().to_string();
        let dest = entry_dir.join(&stored_rel);
        let outcome = if item.kind == "link" {
            move_link(&item.path, &dest)
        } else {
            move_dir(&item.path, &dest)
        };
        let mut entry = ManifestEntry {
            action: if outcome.is_ok() {
                "move-to-trash".to_string()
            } else {
                "failed".to_string()
            },
            target: item.path.to_string_lossy().to_string(),
            source: None,
            original: Some(item.path.to_string_lossy().to_string()),
            stored: if outcome.is_ok() {
                Some(stored_rel_str)
            } else {
                None
            },
            kind: Some(item.kind.clone()),
        };
        if outcome.is_ok() && item.kind == "link" {
            entry.source = std::fs::read_link(&dest)
                .ok()
                .map(|p| p.to_string_lossy().to_string());
        }
        if let Err(e) = &outcome {
            // 失败原因写进 target 之外的字段，避免丢失诊断信息
            entry.action = format!("failed: {}", e);
        } else {
            stored_index += 1;
        }
        entries.push(entry);
    }

    let manifest = Manifest {
        op: op.to_string(),
        created_at: crate::util::now_human(),
        summary: summary.to_string(),
        entries,
    };
    let manifest_path = entry_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入回收站清单失败：{}", e))?;

    Ok((
        entry_name,
        manifest_path.to_string_lossy().to_string(),
        manifest.entries,
    ))
}

/// 供其它模块复用的目录复制（Profile 部署 Skill 时会用到）
pub fn copy_skill_dir(from: &Path, to: &Path) -> Result<(usize, u64), String> {
    copy_dir(from, to)
}

/// 供其它模块复用：写一份可恢复清单
pub fn write_manifest_public(
    op: &str,
    summary: &str,
    entries: Vec<ManifestEntry>,
) -> Option<String> {
    write_manifest(op, summary, entries)
}

/* -------------------------------------------------------------- 导入 Skill */

fn tier_of(capability: &str) -> (&'static str, &'static str) {
    match capability {
        "skill.copy" | "skill.link" => ("deploy", "T2"),
        "skill.relink" => ("deploy", "T2"),
        "path.delete" => ("mutate", "T3"),
        "git.clone" => ("mutate", "T3"),
        _ => ("deploy", "T2"),
    }
}

pub fn plan_import(
    source: &Path,
    library: &Path,
    mode: &str,
    names: &[String],
) -> Result<ActionPlan, String> {
    let discovered = discover_skills(source, Some(library))?;
    let selected: Vec<&DiscoveredSkill> = if names.is_empty() {
        discovered.iter().collect()
    } else {
        discovered
            .iter()
            .filter(|s| names.iter().any(|n| n == &s.name))
            .collect()
    };
    if selected.is_empty() {
        return Err("没有选中任何 Skill".to_string());
    }

    let mut items: Vec<PlanItem> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let (mut create, mut skip) = (0usize, 0usize);

    for skill in &selected {
        let target = library.join(&skill.name);
        if target.exists() || is_link(&target) {
            skip += 1;
            items.push(PlanItem {
                action: "skip".into(),
                target: target.to_string_lossy().to_string(),
                source: Some(skill.path.clone()),
                detail: "目标库中已存在同名 Skill，将跳过（不覆盖）".into(),
                risk: "safe".into(),
            });
            continue;
        }
        create += 1;
        items.push(PlanItem {
            action: if mode == "link" { "create-link".into() } else { "copy-dir".into() },
            target: target.to_string_lossy().to_string(),
            source: Some(skill.path.clone()),
            detail: format!(
                "{}（{} 个文件 / {}）",
                if mode == "link" { "创建链接指向来源目录" } else { "复制到技能库" },
                skill.file_count,
                crate::util::format_bytes(skill.bytes)
            ),
            risk: "safe".into(),
        });
    }

    if mode == "link" {
        warnings.push(
            "链接模式下，来源目录一旦移动或删除，Skill 会在所有 Agent 侧静默失效（可在「清理失效」里处理）".into(),
        );
    }
    if skip > 0 {
        warnings.push(format!("{} 个同名 Skill 会被跳过，已存在的内容不会被改动", skip));
    }

    let (tier, code) = tier_of(if mode == "link" { "skill.link" } else { "skill.copy" });
    Ok(ActionPlan {
        capability: if mode == "link" { "skill.link".into() } else { "skill.copy".into() },
        tier: tier.into(),
        tier_code: code.into(),
        title: "导入 Skill".into(),
        summary: format!(
            "将 {} 个 Skill {}到 {}（跳过 {} 个）",
            create,
            if mode == "link" { "链接" } else { "复制" },
            library.to_string_lossy(),
            skip
        ),
        items,
        warnings,
        confirm_hint: format!("{} 级操作：会在目标库写入 {} 项，执行前会先记录可恢复清单", code, create),
    })
}

pub fn apply_import(
    source: &Path,
    library: &Path,
    mode: &str,
    names: &[String],
) -> ActionResult {
    let mut result = ActionResult::new("导入 Skill");
    let plan = match plan_import(source, library, mode, names) {
        Ok(p) => p,
        Err(e) => {
            result.step(source.to_string_lossy().to_string(), false, e);
            return result;
        }
    };

    // 先写清单，再动手
    let entries: Vec<ManifestEntry> = plan
        .items
        .iter()
        .filter(|i| i.action != "skip")
        .map(|i| ManifestEntry {
            action: i.action.clone(),
            target: i.target.clone(),
            source: i.source.clone(),
            original: None,
            ..Default::default()
        })
        .collect();
    result.manifest = write_manifest("skill-import", &plan.summary, entries);

    std::fs::create_dir_all(library).ok();
    let mut done = 0usize;

    for item in &plan.items {
        if item.action == "skip" {
            result.step(item.target.clone(), true, "已存在，跳过");
            continue;
        }
        let src = PathBuf::from(item.source.clone().unwrap_or_default());
        let dst = PathBuf::from(&item.target);
        let outcome = if item.action == "create-link" {
            create_dir_link(&dst, &src).map(|_| "已创建链接".to_string())
        } else {
            copy_dir(&src, &dst).map(|(f, b)| {
                format!("已复制 {} 个文件 / {}", f, crate::util::format_bytes(b))
            })
        };
        match outcome {
            Ok(msg) => {
                done += 1;
                result.step(item.target.clone(), true, msg);
            }
            Err(e) => result.step(item.target.clone(), false, e),
        }
    }

    result.summary = format!(
        "{} 完成：成功 {} 项{}",
        result.title,
        done,
        if plan.items.iter().any(|i| i.action == "skip") {
            "，部分同名项已跳过"
        } else {
            ""
        }
    );
    result.restore_hint = if result.manifest.is_some() {
        "已记录清单，可在「内容目录」中依清单撤销本次导入（删除新建的链接/副本）".into()
    } else {
        "未能写入清单，请勿依赖自动恢复".into()
    };
    if done == 0 && result.ok {
        result.warnings.push("没有实际执行任何导入".into());
    }
    result
}

/* ------------------------------------------------------ 清理失效链接 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenRef {
    pub path: String,
    pub target: String,
    pub kind: String,
    /// 归属（Agent 名或共享库）
    pub owner: String,
    pub name: String,
}

pub fn plan_cleanup_broken(broken: &[BrokenRef]) -> ActionPlan {
    let mut items: Vec<PlanItem> = Vec::new();
    let mut by_owner: std::collections::BTreeMap<String, usize> = Default::default();
    let mut by_target: std::collections::BTreeMap<String, usize> = Default::default();
    let mut seen: std::collections::HashSet<String> = Default::default();
    let mut merged = 0usize;

    for item in broken {
        *by_owner.entry(item.owner.clone()).or_insert(0) += 1;
        *by_target.entry(item.target.clone()).or_insert(0) += 1;
        // 路径别名会让同一个对象出现多次，计划里也要合并
        if !seen.insert(identity_key(Path::new(&item.path))) {
            merged += 1;
            continue;
        }
        items.push(PlanItem {
            action: "move-to-trash".into(),
            target: item.path.clone(),
            source: Some(item.target.clone()),
            detail: format!(
                "把失效{}移入回收站（原指向不存在的 {}，可整体恢复）",
                if item.kind == "junction" { "目录链接" } else { "符号链接" },
                item.target
            ),
            risk: "destructive".into(),
        });
    }

    let mut warnings: Vec<String> = Vec::new();
    if merged > 0 {
        warnings.push(format!(
            "{} 项与其他条目指向同一对象（路径别名，例如 junction 指向的同一目录），已合并为一次操作",
            merged
        ));
    }
    for (target, count) in by_target.iter().filter(|(_, c)| **c >= 3) {
        warnings.push(format!(
            "{} 个链接共同指向缺失目录 {} —— 若该目录能被恢复（例如把技能库导入到此路径），这些链接会自动重新生效，无需清理",
            count, target
        ));
    }

    let (tier, code) = tier_of("path.delete");
    ActionPlan {
        capability: "path.delete".into(),
        tier: tier.into(),
        tier_code: code.into(),
        title: "清理失效 Skill 链接".into(),
        summary: format!(
            "将清理 {} 个失效链接（涉及 {} 个来源目录{}）",
            items.len(),
            by_owner.len(),
            if merged > 0 {
                format!("，另有 {} 项为重复别名", merged)
            } else {
                String::new()
            }
        ),
        items,
        warnings,
        confirm_hint: format!(
            "{} 级操作：这批链接会整体移入**一个**回收站条目，可一键整批恢复；链接目标与真实目录都不会被触碰",
            code
        ),
    }
}

pub fn apply_cleanup_broken(broken: &[BrokenRef]) -> ActionResult {
    let mut result = ActionResult::new("清理失效 Skill 链接");
    if broken.is_empty() {
        result.step("(空)".to_string(), true, "没有失效链接需要清理");
        result.summary = "没有失效链接需要清理".to_string();
        return result;
    }

    let items: Vec<TrashItem> = broken
        .iter()
        .map(|b| TrashItem {
            path: PathBuf::from(&b.path),
            kind: "link".to_string(),
        })
        .collect();
    let summary = format!("清理 {} 个失效 Skill 链接", broken.len());

    match move_to_trash(items, "skill-cleanup-broken", &summary) {
        Ok((entry, manifest, entries)) => {
            let mut done = 0usize;
            let mut merged = 0usize;
            for item in &entries {
                if item.stored.is_some() {
                    done += 1;
                    result.step(
                        item.target.clone(),
                        true,
                        format!(
                            "已移入回收站{}",
                            item.source
                                .as_ref()
                                .map(|t| format!("（原指向 {}）", t))
                                .unwrap_or_default()
                        ),
                    );
                } else if item.action == "skipped-alias" {
                    merged += 1;
                    result.step(
                        item.target.clone(),
                        true,
                        "与另一条目指向同一对象（路径别名），已合并处理",
                    );
                } else {
                    result.step(item.target.clone(), false, item.action.clone());
                }
            }
            result.summary = format!(
                "已清理 {} 个失效链接{}，全部移入回收站条目 {}（可整体恢复）",
                done,
                if merged > 0 {
                    format!("（另有 {} 项为重复别名，未重复处理）", merged)
                } else {
                    String::new()
                },
                entry
            );
            result.manifest = Some(manifest);
        }
        Err(e) => result.step("(batch)".to_string(), false, e),
    }
    result.step(
        "(安全)".to_string(),
        true,
        "链接目标与真实目录均未被触碰",
    );
    result.restore_hint = "在「历史与审计 → 回收站」中可把这一批链接原样恢复到原位（指向关系保留）".into();
    result
}

/* -------------------------------------------------------- 重建失效链接 */

/// 在技能库中按名字查找 Skill。技能库常见 `<category>/<skill>` 结构，
/// 因此除根目录外还要下探一层分类目录。
pub fn find_skill_in(root: &Path, name: &str) -> Option<PathBuf> {
    let direct = root.join(name);
    if direct.is_dir() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    let mut best: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let category = entry.path();
        if !category.is_dir() || is_link(&category) {
            continue;
        }
        let candidate = category.join(name);
        if candidate.is_dir() {
            // 优先返回确实含 SKILL.md 的那个
            if candidate.join("SKILL.md").is_file() || candidate.join("skill.md").is_file() {
                return Some(candidate);
            }
            best.get_or_insert(candidate);
        }
    }
    best
}

pub fn plan_relink(broken: &[BrokenRef], new_root: &Path) -> ActionPlan {
    let mut items: Vec<PlanItem> = Vec::new();
    let mut missing = 0usize;
    for item in broken {
        let candidate = find_skill_in(new_root, &item.name);
        let ok = candidate.is_some();
        if !ok {
            missing += 1;
        }
        items.push(PlanItem {
            action: if ok { "create-link".into() } else { "skip".into() },
            target: item.path.clone(),
            source: candidate
                .as_ref()
                .map(|c| c.to_string_lossy().to_string())
                .or_else(|| Some(new_root.join(&item.name).to_string_lossy().to_string())),
            detail: match &candidate {
                Some(c) => format!("把链接重新指向 {}", c.to_string_lossy()),
                None => format!("新库中没有 {} ，跳过", item.name),
            },
            risk: "safe".into(),
        });
    }
    let (tier, code) = tier_of("skill.relink");
    ActionPlan {
        capability: "skill.relink".into(),
        tier: tier.into(),
        tier_code: code.into(),
        title: "重建失效链接".into(),
        summary: format!(
            "将把 {} 个失效链接重新指向 {}（{} 个在新库中找不到，会跳过）",
            broken.len() - missing,
            new_root.to_string_lossy(),
            missing
        ),
        items,
        warnings: vec![
            "重建会先删除原失效链接再创建新链接；原链接信息已记录在清单中".into(),
        ],
        confirm_hint: format!("{} 级操作：仅调整链接指向，不复制文件内容", code),
    }
}

pub fn apply_relink(broken: &[BrokenRef], new_root: &Path) -> ActionResult {
    let mut result = ActionResult::new("重建失效链接");
    let plan = plan_relink(broken, new_root);
    let entries: Vec<ManifestEntry> = broken
        .iter()
        .map(|b| ManifestEntry {
            action: "delete-link".into(),
            target: b.path.clone(),
            source: Some(b.target.clone()),
            original: Some(b.path.clone()),
            ..Default::default()
        })
        .collect();
    result.manifest = write_manifest("skill-relink", &plan.summary, entries);

    let mut done = 0usize;
    for item in broken {
        let Some(candidate) = find_skill_in(new_root, &item.name) else {
            result.step(item.path.clone(), true, "新库中没有该 Skill，跳过");
            continue;
        };
        let path = PathBuf::from(&item.path);
        if let Err(e) = remove_link(&path) {
            result.step(item.path.clone(), false, format!("删除原链接失败：{}", e));
            continue;
        }
        match create_dir_link(&path, &candidate) {
            Ok(_) => {
                done += 1;
                result.step(item.path.clone(), true, format!("已指向 {}", candidate.to_string_lossy()));
            }
            Err(e) => result.step(item.path.clone(), false, e),
        }
    }
    result.summary = format!("重建完成：{} 个链接已指向新库", done);
    result.restore_hint = "如需还原，可依清单把链接指回原目标路径".into();
    result
}

/* -------------------------------------------------------- 删除 Skill */

/// 删除前必须确认路径位于已声明的技能库内，避免误删任意目录
pub fn plan_delete_skill(path: &Path, libraries: &[PathBuf], link_impact: usize) -> Result<ActionPlan, String> {
    if !path.exists() && !is_link(path) {
        return Err(format!("路径不存在：{}", path.to_string_lossy()));
    }
    let inside = libraries.iter().any(|lib| path.starts_with(lib));
    if !inside {
        return Err(format!(
            "拒绝删除：{} 不在任何已声明的技能库目录内（技能库：{}）",
            path.to_string_lossy(),
            libraries
                .iter()
                .map(|l| l.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("、")
        ));
    }

    let is_l = is_link(path);
    let (tier, code) = tier_of("path.delete");
    let mut warnings: Vec<String> = Vec::new();
    if link_impact > 0 {
        warnings.push(format!(
            "检测到 {} 个链接指向该 Skill：删除后这些链接会立即失效",
            link_impact
        ));
    }

    Ok(ActionPlan {
        capability: "path.delete".into(),
        tier: tier.into(),
        tier_code: code.into(),
        title: "删除 Skill".into(),
        summary: if is_l {
            format!("将删除链接 {}", path.to_string_lossy())
        } else {
            format!("将把 {} 移入 AgentHub 回收站（可恢复）", path.to_string_lossy())
        },
        items: vec![PlanItem {
            action: "move-to-trash".into(),
            target: path.to_string_lossy().to_string(),
            source: None,
            detail: if is_l {
                "把链接本身移入回收站（链接目标不受影响，可恢复）".into()
            } else {
                "把目录移入回收站（内容完整保留，可恢复）".into()
            },
            risk: "destructive".into(),
        }],
        warnings,
        confirm_hint: format!(
            "{} 级操作：{}都先进回收站，可在「历史与审计 → 回收站」一键恢复",
            code,
            if is_l { "链接" } else { "目录" }
        ),
    })
}

pub fn apply_delete_skill(path: &Path) -> ActionResult {
    let mut result = ActionResult::new("删除 Skill");
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());
    // 链接与真实目录统一进回收站：链接保留指向关系，目录保留完整内容
    let kind = if is_link(path) { "link" } else { "dir" };
    let summary = format!(
        "删除 Skill {}（{}）",
        name,
        if kind == "link" { "链接" } else { "目录" }
    );

    match move_to_trash(
        vec![TrashItem {
            path: path.to_path_buf(),
            kind: kind.to_string(),
        }],
        "skill-delete",
        &summary,
    ) {
        Ok((entry, manifest, _)) => {
            result.step(
                path.to_string_lossy().to_string(),
                true,
                format!("已移入回收站条目 {}", entry),
            );
            result.summary = format!(
                "{} 已移入回收站（{}）{}",
                name,
                entry,
                if kind == "link" {
                    "，链接目标未受影响"
                } else {
                    ""
                }
            );
            result.manifest = Some(manifest);
        }
        Err(e) => result.step(path.to_string_lossy().to_string(), false, e),
    }
    result.restore_hint =
        "在「历史与审计 → 回收站」中可恢复到原位置；链接会原样恢复其指向关系".into();
    result
}

/* ------------------------------------------------------- 依清单恢复 */

/// 按 manifest 撤销一次操作：
/// * `create-link` / `copy-dir`（导入留下的）→ 删除或移入回收站
/// * `delete-link`（清理/重建时删掉的）→ 按原目标重建链接
/// * `move-to-trash` → 从回收站移回原路径
pub fn restore_manifest(manifest_path: &Path) -> ActionResult {
    // 若清单本身就位于回收站条目内（删除 / 清理产生），直接整体恢复该条目。
    // 批量条目有多个对象、无法靠单一路径定位，这种走法最可靠。
    if let Some(parent) = manifest_path.parent() {
        if parent.starts_with(trash_root()) {
            if let Some(name) = parent.file_name().map(|s| s.to_string_lossy().to_string()) {
                let mut outcome = restore_trash(&name);
                outcome.title = "依清单恢复（回收站条目）".to_string();
                return outcome;
            }
        }
    }

    let mut result = ActionResult::new("依清单恢复");
    let manifest = match std::fs::read_to_string(manifest_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Manifest>(&t).ok())
    {
        Some(m) => m,
        None => {
            result.step(
                manifest_path.to_string_lossy().to_string(),
                false,
                "清单读取失败或格式不正确",
            );
            return result;
        }
    };

    for entry in &manifest.entries {
        let target = PathBuf::from(&entry.target);
        match entry.action.as_str() {
            "create-link" => match remove_link(&target) {
                Ok(_) => result.step(entry.target.clone(), true, "已撤销链接创建"),
                Err(e) => result.step(entry.target.clone(), false, e),
            },
            "copy-dir" => {
                if !target.exists() {
                    result.step(entry.target.clone(), true, "目标已不存在，无需处理");
                    continue;
                }
                let name = target
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "skill".into());
                let entry_dir = trash_root().join(format!("{}-undo-{}", timestamp(), name));
                let content = entry_dir.join("content");
                let _ = std::fs::create_dir_all(&entry_dir);
                let undo = Manifest {
                    op: "undo-import".into(),
                    created_at: crate::util::now_human(),
                    summary: format!("撤销导入 {}", name),
                    entries: vec![ManifestEntry {
                        action: "move-to-trash".into(),
                        target: entry.target.clone(),
                        source: None,
                        original: Some(entry.target.clone()),
                        ..Default::default()
                    }],
                };
                let _ = std::fs::write(
                    entry_dir.join("manifest.json"),
                    serde_json::to_string_pretty(&undo).unwrap_or_default(),
                );
                match std::fs::rename(&target, &content) {
                    Ok(_) => result.step(
                        entry.target.clone(),
                        true,
                        format!("已移入回收站：{}", entry_dir.to_string_lossy()),
                    ),
                    Err(e) => result.step(entry.target.clone(), false, format!("移入回收站失败：{}", e)),
                }
            }
            "delete-link" => {
                let Some(source) = entry.source.clone() else {
                    result.step(entry.target.clone(), false, "清单缺少原链接目标");
                    continue;
                };
                if !PathBuf::from(&source).exists() {
                    result.step(
                        entry.target.clone(),
                        false,
                        format!("原目标不存在，无法重建：{}", source),
                    );
                    continue;
                }
                if target.exists() || is_link(&target) {
                    result.step(entry.target.clone(), true, "目标位置已存在，跳过");
                    continue;
                }
                match create_dir_link(&target, &PathBuf::from(&source)) {
                    Ok(_) => result.step(entry.target.clone(), true, format!("已重建链接 → {}", source)),
                    Err(e) => result.step(entry.target.clone(), false, e),
                }
            }
            "move-to-trash" => {
                // 在回收站里找到对应条目再移回
                let found = list_trash()
                    .into_iter()
                    .find(|t| t.original.as_deref() == Some(entry.target.as_str()));
                match found {
                    Some(item) => {
                        let restored = restore_trash(&item.name);
                        for step in restored.steps {
                            result.steps.push(step);
                        }
                    }
                    None => result.step(entry.target.clone(), false, "回收站中找不到对应条目"),
                }
            }
            other => result.step(entry.target.clone(), false, format!("未知的操作类型：{}", other)),
        }
    }

    let ok = result.steps.iter().filter(|s| s.ok).count();
    result.summary = format!("恢复完成：{} / {} 项成功", ok, result.steps.len());
    result.restore_hint = "如已确认结果，可删除该清单文件".into();
    result
}

/* ---------------------------------------------------------- 回收站 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub name: String,
    pub path: String,
    /// 单对象条目为原路径；批量条目为 None（用 summary 展示）
    pub original: Option<String>,
    pub created_at: String,
    pub size: u64,
    /// 条目内的对象数量（批量清理会产生多对象条目）
    pub item_count: usize,
    /// 条目摘要
    pub summary: String,
    /// link | dir | mixed | unknown
    pub kind: String,
}

pub fn list_trash() -> Vec<TrashEntry> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(trash_root()) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("manifest.json");
        let manifest = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|t| serde_json::from_str::<Manifest>(&t).ok());

        let moved: Vec<&ManifestEntry> = manifest
            .as_ref()
            .map(|m| m.entries.iter().filter(|e| e.stored.is_some()).collect())
            .unwrap_or_default();
        let item_count = moved.len();

        // 单对象条目展示原路径；多对象条目展示摘要
        let original = if item_count == 1 {
            moved[0].original.clone()
        } else {
            None
        };
        let kind = {
            let mut kinds: Vec<String> = moved
                .iter()
                .filter_map(|e| e.kind.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            kinds.sort();
            match kinds.len() {
                0 => "unknown".to_string(),
                1 => kinds.remove(0),
                _ => "mixed".to_string(),
            }
        };
        let summary = manifest
            .as_ref()
            .map(|m| m.summary.clone())
            .unwrap_or_default();
        let created_at = manifest
            .as_ref()
            .map(|m| m.created_at.clone())
            .unwrap_or_default();

        let (_, bytes) = crate::util::dir_stats(&dir.join("items"), 20000);
        out.push(TrashEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: dir.to_string_lossy().to_string(),
            original,
            created_at,
            size: bytes,
            item_count,
            summary,
            kind,
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

/// 恢复整条回收站条目（等价于恢复其中全部对象）
pub fn restore_trash(name: &str) -> ActionResult {
    restore_trash_items(name, &[])
}

fn trash_entry_dir(name: &str) -> PathBuf {
    trash_root().join(name)
}

/// 回收站条目详情：里面有哪些对象、原位置在哪、是否还能恢复
pub fn trash_detail(name: &str) -> Result<TrashDetail, String> {
    let entry_dir = trash_entry_dir(name);
    if !entry_dir.is_dir() {
        return Err(format!("回收站条目不存在：{}", name));
    }
    let entry = list_trash()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("无法读取回收站条目：{}", name))?;
    let manifest = std::fs::read_to_string(entry_dir.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Manifest>(&t).ok());

    let mut items: Vec<TrashItemInfo> = Vec::new();
    if let Some(manifest) = manifest {
        for item in manifest.entries.iter().filter(|e| e.stored.is_some()) {
            let stored = item.stored.clone().unwrap_or_default();
            let stored_path = entry_dir.join(&stored);
            let original = item.original.clone().unwrap_or_default();
            let original_path = PathBuf::from(&original);
            let size = if stored_path.is_dir() {
                crate::util::dir_stats(&stored_path, 20000).1
            } else {
                0
            };
            items.push(TrashItemInfo {
                stored,
                name: stored_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                kind: item.kind.clone().unwrap_or_else(|| "unknown".to_string()),
                original: original.clone(),
                link_target: item.source.clone(),
                size,
                restorable: !(original_path.exists() || is_link(&original_path)),
                removed_at: manifest.created_at.clone(),
            });
        }
    }

    Ok(TrashDetail { entry, items })
}

/// 恢复条目内的**部分**对象（stored 为空表示整条恢复）
pub fn restore_trash_items(name: &str, stored_list: &[String]) -> ActionResult {
    let mut result = ActionResult::new("从回收站恢复");
    let entry_dir = trash_entry_dir(name);
    if !entry_dir.is_dir() {
        result.step(name.to_string(), false, "回收站条目不存在");
        return result;
    }
    let manifest = match std::fs::read_to_string(entry_dir.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Manifest>(&t).ok())
    {
        Some(m) => m,
        None => {
            result.step(name.to_string(), false, "回收站清单缺失或损坏，无法自动恢复");
            return result;
        }
    };

    let selected: Vec<&ManifestEntry> = manifest
        .entries
        .iter()
        .filter(|e| e.stored.is_some())
        .filter(|e| {
            stored_list.is_empty()
                || e.stored
                    .as_deref()
                    .map(|s| stored_list.iter().any(|x| x == s))
                    .unwrap_or(false)
        })
        .collect();

    if selected.is_empty() {
        result.step(name.to_string(), false, "没有匹配到可恢复的对象");
        return result;
    }

    let mut restored = 0usize;
    for entry in &selected {
        let Some(original) = entry.original.clone() else {
            result.step(entry.target.clone(), false, "清单缺少原路径");
            continue;
        };
        let stored_path = entry_dir.join(entry.stored.clone().unwrap_or_default());
        let target = PathBuf::from(&original);
        if target.exists() || is_link(&target) {
            result.step(original, false, "原位置已被占用，跳过");
            continue;
        }
        let is_link_item = entry.kind.as_deref() == Some("link") || is_link(&stored_path);
        let outcome = if is_link_item {
            move_link(&stored_path, &target)
        } else {
            move_dir(&stored_path, &target)
        };
        match outcome {
            Ok(_) => {
                restored += 1;
                result.step(
                    original,
                    true,
                    if is_link_item {
                        "已恢复链接（指向关系原样保留）"
                    } else {
                        "已恢复目录到原路径"
                    },
                );
            }
            Err(e) => result.step(original, false, format!("恢复失败：{}", e)),
        }
    }

    // 整条恢复且全部成功时清掉空条目；部分恢复则保留剩余对象
    if restored == selected.len() && selected.len() == manifest.entries.iter().filter(|e| e.stored.is_some()).count() {
        let _ = std::fs::remove_dir_all(&entry_dir);
        result.summary = format!("已恢复全部 {} 项，回收站条目已清空", restored);
    } else {
        result.summary = format!("已恢复 {} / {} 项，其余仍保留在回收站", restored, selected.len());
    }
    result.restore_hint = "恢复后原路径的内容与删除前一致（链接保留原指向）".into();
    result
}

/* ------------------------------------------------------ 回收站管理 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItemInfo {
    pub stored: String,
    pub name: String,
    /// link | dir
    pub kind: String,
    pub original: String,
    pub link_target: Option<String>,
    pub size: u64,
    /// 原位置是否仍然空着（能否恢复）
    pub restorable: bool,
    pub removed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashDetail {
    pub entry: TrashEntry,
    pub items: Vec<TrashItemInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashStats {
    pub entries: usize,
    pub items: usize,
    pub bytes: u64,
    pub oldest: Option<String>,
    pub newest: Option<String>,
    pub dir: String,
}

pub fn trash_stats() -> TrashStats {
    let entries = list_trash();
    TrashStats {
        entries: entries.len(),
        items: entries.iter().map(|e| e.item_count).sum(),
        bytes: entries.iter().map(|e| e.size).sum(),
        oldest: entries.last().map(|e| e.name.clone()),
        newest: entries.first().map(|e| e.name.clone()),
        dir: trash_root().to_string_lossy().to_string(),
    }
}

/// 永久删除若干回收站条目（不可恢复）
pub fn plan_purge_trash(names: &[String]) -> Result<ActionPlan, String> {
    let all = list_trash();
    let mut items: Vec<PlanItem> = Vec::new();
    let mut bytes = 0u64;
    let mut count = 0usize;
    for name in names {
        let Some(entry) = all.iter().find(|e| &e.name == name) else {
            continue;
        };
        bytes += entry.size;
        count += entry.item_count;
        items.push(PlanItem {
            action: "purge".into(),
            target: entry.path.clone(),
            source: None,
            detail: format!(
                "永久删除：{}（{} 个对象 / {}）—— 不可恢复",
                entry.summary,
                entry.item_count,
                crate::util::format_bytes(entry.size)
            ),
            risk: "destructive".into(),
        });
    }
    if items.is_empty() {
        return Err("没有匹配到可删除的回收站条目".to_string());
    }
    let (tier, code) = tier_of("path.delete");
    Ok(ActionPlan {
        capability: "trash.purge".into(),
        tier: tier.into(),
        tier_code: code.into(),
        title: "永久删除回收站条目".into(),
        summary: format!(
            "将永久删除 {} 个条目（共 {} 个对象 / {}）",
            items.len(),
            count,
            crate::util::format_bytes(bytes)
        ),
        items,
        warnings: vec![
            "永久删除**无法撤销**，也不会再记录清单。若不确定，请先「恢复」再处理。".to_string(),
        ],
        confirm_hint: format!("{} 级操作：{}", code, "此操作不可恢复"),
    })
}

pub fn apply_purge_trash(names: &[String]) -> ActionResult {
    let mut result = ActionResult::new("永久删除回收站条目");
    let plan = match plan_purge_trash(names) {
        Ok(p) => p,
        Err(e) => {
            result.step("(batch)".to_string(), false, e);
            return result;
        }
    };

    let root = trash_root();
    let mut removed = 0usize;
    let mut freed = 0u64;
    for item in &plan.items {
        let path = PathBuf::from(&item.target);
        // 只允许删除回收站目录内的内容
        if !path.starts_with(&root) {
            result.step(item.target.clone(), false, "拒绝删除：不在回收站目录内");
            continue;
        }
        let size = crate::util::dir_stats(&path, 20000).1;
        match std::fs::remove_dir_all(&path) {
            Ok(_) => {
                removed += 1;
                freed += size;
                result.step(item.target.clone(), true, "已永久删除");
            }
            Err(e) => result.step(item.target.clone(), false, format!("删除失败：{}", e)),
        }
    }
    result.summary = format!(
        "已永久删除 {} 个条目，释放 {}",
        removed,
        crate::util::format_bytes(freed)
    );
    result.restore_hint = "永久删除的内容无法恢复".into();
    result
}

/// 清理 N 天前的回收站条目（N = 0 表示全部）
pub fn plan_purge_older_than(days: u32) -> Result<ActionPlan, String> {
    let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
    let names: Vec<String> = list_trash()
        .into_iter()
        .filter(|e| {
            if days == 0 {
                return true;
            }
            // 条目名形如 20260928-183456-xxx
            let stamp = e.name.split('-').next().unwrap_or("");
            chrono::NaiveDateTime::parse_from_str(
                &format!("{}{}", stamp, "000000"),
                "%Y%m%d%H%M%S",
            )
            .ok()
            .and_then(|naive| naive.and_local_timezone(chrono::Local).single())
            .map(|dt| dt < cutoff)
            .unwrap_or(false)
        })
        .map(|e| e.name)
        .collect();
    if names.is_empty() {
        return Err(if days == 0 {
            "回收站已经是空的".to_string()
        } else {
            format!("没有早于 {} 天的回收站条目", days)
        });
    }
    plan_purge_trash(&names)
}

pub fn apply_purge_older_than(days: u32) -> ActionResult {
    let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
    let names: Vec<String> = list_trash()
        .into_iter()
        .filter(|e| {
            if days == 0 {
                return true;
            }
            let stamp = e.name.split('-').next().unwrap_or("");
            chrono::NaiveDateTime::parse_from_str(&format!("{}{}", stamp, "000000"), "%Y%m%d%H%M%S")
                .ok()
                .and_then(|naive| naive.and_local_timezone(chrono::Local).single())
                .map(|dt| dt < cutoff)
                .unwrap_or(false)
        })
        .map(|e| e.name)
        .collect();
    let mut result = apply_purge_trash(&names);
    result.title = if days == 0 {
        "清空回收站".to_string()
    } else {
        format!("清理 {} 天前的回收站条目", days)
    };
    result
}

/* ------------------------------------------------------- Git 仓库导入 */

/// 通过 git 克隆仓库到临时目录（T3：执行外部程序 + 网络访问）
pub fn clone_repo(url: &str, proxy: &str, dest: &Path) -> ActionResult {
    let mut result = ActionResult::new("克隆 Git 仓库");
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }

    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1", "--no-tags"])
        .arg(url)
        .arg(dest);
    // 代理：只作用于本次子进程，不改动全局 git 配置
    if !proxy.trim().is_empty() {
        cmd.env("HTTP_PROXY", proxy)
            .env("HTTPS_PROXY", proxy)
            .env("ALL_PROXY", proxy)
            .env("http_proxy", proxy)
            .env("https_proxy", proxy);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    match cmd.output() {
        Ok(out) if out.status.success() => {
            result.step(
                url.to_string(),
                true,
                format!("已克隆到 {}", dest.to_string_lossy()),
            );
            result.summary = format!("仓库已克隆到临时目录：{}", dest.to_string_lossy());
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: String = err.lines().rev().take(4).collect::<Vec<_>>().join(" / ");
            result.summary = "克隆失败".into();
            result.warnings.push(format!(
                "git 退出码 {:?}；{} —— 如需代理请在设置里配置（当前：{}）",
                out.status.code(),
                tail.trim(),
                if proxy.trim().is_empty() { "未设置" } else { proxy }
            ));
            result.step(url.to_string(), false, "git clone 未成功");
        }
        Err(e) => {
            result.summary = "无法调用 git".into();
            result.step(url.to_string(), false, format!("启动 git 失败：{}", e));
        }
    }
    result
}

/// 供 UI 判断 git 是否可用
pub fn git_available() -> Option<String> {
    let path = crate::util::resolve_program("git", &[])?;
    let version = crate::util::run_capture(&path, &["--version"], Duration::from_secs(6))
        .ok()
        .and_then(|out| crate::util::extract_version(&out));
    Some(format!(
        "{} {}",
        path.to_string_lossy(),
        version.map(|v| format!("v{}", v)).unwrap_or_default()
    ))
}