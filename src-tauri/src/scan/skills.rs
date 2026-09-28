//! Skill 扫描：发现 SKILL.md 目录、散装 .md，以及符号链接 / junction 部署形式。
//!
//! 链接形态很常见（把统一 Skill 库链接到各 Agent 目录）。链接目标被删除时 Agent
//! 会静默失效，因此这里显式检测并上报「失效链接」。

use crate::model::{AgentTarget, AppSettings, SkillFound};
use crate::util;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct LinkInfo {
    kind: Option<String>,
    target: Option<String>,
    broken: bool,
}

fn inspect_link(path: &Path) -> LinkInfo {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            return LinkInfo {
                kind: None,
                target: None,
                broken: false,
            }
        }
    };
    let file_type = meta.file_type();
    if !file_type.is_symlink() {
        return LinkInfo {
            kind: None,
            target: None,
            broken: false,
        };
    }
    let target = std::fs::read_link(path)
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    // Windows 上 junction 与目录符号链接都无法直接区分，统一标注为「目录链接」而非过度声称
    let kind = {
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt;
            if file_type.is_symlink_dir() {
                "junction"
            } else {
                "symlink"
            }
        }
        #[cfg(not(windows))]
        {
            "symlink"
        }
    };

    LinkInfo {
        kind: Some(kind.to_string()),
        target,
        broken: !path.exists(),
    }
}

/// 跨 Agent 共享 / 通用 Skill 库（不属于任何单一 Agent，单独纳管）
const SHARED_SKILL_DIRS: &[&str] = &["~/.agent/skills", "~/.agents/skills"];

pub fn scan_skills(
    settings: &AppSettings,
    defs: &[crate::agentdef::LoadedDef],
    agent_list: &[AgentTarget],
    warnings: &mut Vec<String>,
) -> Vec<SkillFound> {
    let mut out: Vec<SkillFound> = Vec::new();

    // 收集候选根目录。注意路径别名：例如 ~/.config/opencode/skills 可能是
    // 指向 ~/.agent/skills 的 junction —— 同一份内容会被扫两遍、计数虚高，
    // 批量操作还会把同一对象当成两个。这里按规范化路径去重，并优先保留真实目录。
    struct Root {
        path: PathBuf,
        canonical: String,
        agent_id: String,
        agent_name: String,
        is_link: bool,
    }

    let mut candidates: Vec<Root> = Vec::new();
    let mut push = |path: PathBuf, agent_id: &str, agent_name: &str| {
        if !path.is_dir() {
            return;
        }
        let is_link = std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let canonical = std::fs::canonicalize(&path)
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .to_ascii_lowercase();
        candidates.push(Root {
            path,
            canonical,
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            is_link,
        });
    };

    // 有痕迹（已安装 / 仅配置 / 残留）的 Agent 都扫描：残留 Agent 的失效链接
    // 恰恰是最需要用户处理的问题
    for def in defs {
        let agent_id = &def.file.agent.id;
        let Some(agent) = agent_list.iter().find(|a| &a.id == agent_id) else {
            continue;
        };
        if agent.status == "absent" {
            continue;
        }
        for rule in def.file.skill_paths() {
            push(
                crate::agentdef::resolve_path(&rule.path),
                agent_id,
                &agent.name,
            );
        }
    }

    // 通用 / 共享 Skill 库
    for raw in SHARED_SKILL_DIRS {
        push(util::expand_buf(raw), "shared", "共享 Skill 库");
    }

    // 用户自定义的 Skill 根目录（自定义 Agent 场景）
    for raw in &settings.extra_skill_roots {
        push(util::expand_buf(raw), "custom", "自定义目录");
    }

    // 按规范化路径归并：同组内优先非链接的那个（真实目录更能代表来源）
    let mut groups: std::collections::BTreeMap<String, Vec<Root>> = Default::default();
    for root in candidates {
        groups.entry(root.canonical.clone()).or_default().push(root);
    }

    for (_, mut group) in groups {
        group.sort_by_key(|r| r.is_link); // false（真实目录）排前面
        let keeper = group.remove(0);
        for alias in &group {
            warnings.push(format!(
                "{} 是指向同一位置的路径别名（{}），已跳过以免重复统计",
                alias.path.to_string_lossy(),
                keeper.path.to_string_lossy()
            ));
        }
        collect_dir(&keeper.path, &keeper.agent_id, &keeper.agent_name, &mut out, warnings);
    }

    out.sort_by(|a, b| {
        b.broken
            .cmp(&a.broken)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    out
}

fn collect_dir(
    dir: &Path,
    agent_id: &str,
    agent_name: &str,
    out: &mut Vec<SkillFound>,
    warnings: &mut Vec<String>,
) {
    collect_level(dir, agent_id, agent_name, None, out, warnings, 0);
}

fn find_manifest(dir: &Path) -> Option<PathBuf> {
    ["SKILL.md", "skill.md", "Skill.md"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// 递归收集。`depth` 只允许 0/1 两层：技能库常见 `<category>/<skill>` 结构，
/// 但不宜无限下钻（避免把仓库里的示例目录也当技能）。
fn collect_level(
    dir: &Path,
    agent_id: &str,
    agent_name: &str,
    category: Option<&str>,
    out: &mut Vec<SkillFound>,
    warnings: &mut Vec<String>,
    depth: usize,
) {
    if !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warnings.push(format!("Skill 目录不可读：{}（{}）", dir.to_string_lossy(), e));
            return;
        }
    };

    let mut broken: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if dir_name.starts_with('.') {
            continue;
        }

        let link = inspect_link(&path);

        // 失效链接：Agent 侧看起来有 Skill，实际加载不到任何内容
        if link.broken {
            broken.push(dir_name.clone());
            out.push(SkillFound {
                id: format!("{}::{}", agent_id, path.to_string_lossy()),
                name: dir_name.clone(),
                path: path.to_string_lossy().to_string(),
                dir_name,
                source_agent: agent_name.to_string(),
                source_agent_id: agent_id.to_string(),
                category: category.map(|c| c.to_string()),
                has_manifest: false,
                link_kind: link.kind.clone(),
                link_target: link.target.clone(),
                broken: true,
                ..Default::default()
            });
            continue;
        }

        if path.is_dir() {
            let Some(manifest) = find_manifest(&path) else {
                // 没有 SKILL.md：可能是分类目录，递归一层
                if depth == 0 {
                    collect_level(
                        &path,
                        agent_id,
                        agent_name,
                        Some(&dir_name),
                        out,
                        warnings,
                        1,
                    );
                }
                continue;
            };
            let text = std::fs::read_to_string(&manifest).unwrap_or_default();
            let (meta, _body) = parse_frontmatter(&text);
            let (file_count, bytes) = util::dir_stats(&path, 2000);
            let display_name = meta.get("name").cloned().unwrap_or_else(|| dir_name.clone());
            out.push(SkillFound {
                id: format!("{}::{}", agent_id, path.to_string_lossy()),
                name: display_name,
                path: path.to_string_lossy().to_string(),
                dir_name,
                source_agent: agent_name.to_string(),
                source_agent_id: agent_id.to_string(),
                description: meta.get("description").cloned(),
                when_to_use: meta
                    .get("when_to_use")
                    .or_else(|| meta.get("whenToUse"))
                    .cloned(),
                category: category.map(|c| c.to_string()),
                has_manifest: true,
                file_count,
                bytes,
                updated_at: util::file_mtime_iso(&manifest),
                link_kind: link.kind,
                link_target: link.target,
                broken: false,
            });
        } else if dir_name.to_ascii_lowercase().ends_with(".md")
            && !dir_name.eq_ignore_ascii_case("SKILL.md")
            && !dir_name.eq_ignore_ascii_case("README.md")
        {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let (meta, _body) = parse_frontmatter(&text);
            let stem = PathBuf::from(&dir_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| dir_name.clone());
            out.push(SkillFound {
                id: format!("{}::{}", agent_id, path.to_string_lossy()),
                name: meta.get("name").cloned().unwrap_or_else(|| stem.clone()),
                path: path.to_string_lossy().to_string(),
                dir_name: stem,
                source_agent: agent_name.to_string(),
                source_agent_id: agent_id.to_string(),
                description: meta.get("description").cloned(),
                when_to_use: meta.get("when_to_use").cloned(),
                category: category.map(|c| c.to_string()),
                has_manifest: false,
                file_count: 1,
                bytes: util::file_size(&path).unwrap_or(0),
                updated_at: util::file_mtime_iso(&path),
                link_kind: link.kind,
                link_target: link.target,
                broken: false,
            });
        }
    }

    if !broken.is_empty() {
        let sample = broken.iter().take(3).cloned().collect::<Vec<_>>().join("、");
        let target = std::fs::read_link(dir.join(&broken[0]))
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_string_lossy().to_string()))
            .unwrap_or_else(|| "（未知目标）".to_string());
        warnings.push(format!(
            "{} 中有 {} 个 Skill 链接失效，指向的目标目录不存在：{} —— 例如 {}。可用「重建链接」把它们指向现有技能库，或用「清理失效」删除。",
            dir.to_string_lossy(),
            broken.len(),
            target,
            sample
        ));
    }
}

/// 解析 Markdown frontmatter（`---` 包裹的简单 key: value）。
pub fn parse_frontmatter(text: &str) -> (HashMap<String, String>, String) {
    let mut meta = HashMap::new();
    let normalized = text.replace("\r\n", "\n");
    let trimmed = normalized.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return (meta, normalized);
    }
    let rest = &trimmed[3..];
    let Some(end) = rest.find("\n---") else {
        return (meta, normalized);
    };
    let header = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n').to_string();

    for line in header.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let k = key.trim().to_string();
            let mut v = value.trim().to_string();
            if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
                || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
            {
                v = v[1..v.len() - 1].to_string();
            }
            if !k.is_empty() {
                meta.insert(k, v);
            }
        }
    }
    (meta, body)
}