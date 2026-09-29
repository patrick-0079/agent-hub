//! 快照对比（M1）：任意两次扫描之间的资源级差异。
//!
//! 纯函数、只读：输入两份 `ScanSnapshot`，输出按资源类型分组的
//! 新增 / 移除 / 变化 三类条目。每类资源用稳定的主键对齐：
//! - Agent → agent id
//! - Skill → 规范化路径
//! - MCP   → 来源 Agent + 名称
//! - 供应商线索 → hint id
//! - Python 环境 → 路径
//! - npm 包 → 管理器 + 包名

use crate::model::ScanSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub a_id: i64,
    pub a_at: String,
    pub b_id: i64,
    pub b_at: String,
    /// 有差异的资源类型（无差异的不出现）
    pub sections: Vec<DiffSection>,
    /// 一句话汇总
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffSection {
    /// agents | skills | mcp | providers | python | npm
    pub resource: String,
    pub title: String,
    pub added: Vec<DiffEntry>,
    pub removed: Vec<DiffEntry>,
    pub changed: Vec<DiffEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    /// 稳定主键（路径 / 组合键）
    pub key: String,
    /// 展示名
    pub label: String,
    /// 变化说明（changed 必有；added/removed 可为补充信息）
    pub detail: String,
}

fn status_label(status: &str) -> &str {
    match status {
        "installed" => "已安装",
        "configured" => "仅发现配置",
        "leftover" => "残留",
        _ => "未发现",
    }
}

fn bytes_label(bytes: u64) -> String {
    crate::util::format_bytes(bytes)
}

/// 对比两份快照。`a` 为旧、`b` 为新（新增 = b 有 a 没有）。
pub fn diff_snapshots(a: &ScanSnapshot, b: &ScanSnapshot) -> SnapshotDiff {
    let mut diff = SnapshotDiff {
        a_at: a.scanned_at.clone(),
        b_at: b.scanned_at.clone(),
        ..Default::default()
    };
    
    /* ---------------- Agent：状态 / 版本变化 ---------------- */
    {
        let map_a: std::collections::HashMap<&str, _> =
            a.agents.iter().map(|x| (x.id.as_str(), x)).collect();
        let map_b: std::collections::HashMap<&str, _> =
            b.agents.iter().map(|x| (x.id.as_str(), x)).collect();
        let mut sec = DiffSection {
            resource: "agents".into(),
            title: "Agent".into(),
            ..Default::default()
        };
        let mut keys: Vec<&str> = map_a.keys().copied().collect();
        keys.extend(map_b.keys().copied());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(key), map_b.get(key)) {
                (Some(old), Some(new)) => {
                    let mut changes: Vec<String> = Vec::new();
                    if old.status != new.status {
                        changes.push(format!(
                            "{} → {}",
                            status_label(&old.status),
                            status_label(&new.status)
                        ));
                    }
                    if old.cli_version != new.cli_version {
                        changes.push(format!(
                            "版本 {} → {}",
                            old.cli_version.as_deref().unwrap_or("—"),
                            new.cli_version.as_deref().unwrap_or("—")
                        ));
                    }
                    if old.skill_count != new.skill_count {
                        changes.push(format!("Skills {} → {}", old.skill_count, new.skill_count));
                    }
                    if old.mcp_count != new.mcp_count {
                        changes.push(format!("MCP {} → {}", old.mcp_count, new.mcp_count));
                    }
                    if !changes.is_empty() {
                        let entry = DiffEntry {
                            key: key.to_string(),
                            label: new.name.clone(),
                            detail: changes.join(" · "),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: new.name.clone(),
                        detail: format!("新发现（{}）", status_label(&new.status)),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: old.name.clone(),
                        detail: format!("不再出现（原为 {}）", status_label(&old.status)),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    /* ---------------- Skill：路径为主键 ---------------- */
    {
        let map_a: std::collections::HashMap<&str, _> =
            a.skills.iter().map(|x| (x.path.as_str(), x)).collect();
        let map_b: std::collections::HashMap<&str, _> =
            b.skills.iter().map(|x| (x.path.as_str(), x)).collect();
        let mut sec = DiffSection {
            resource: "skills".into(),
            title: "Skill".into(),
            ..Default::default()
        };
        let mut keys: Vec<&str> = map_a.keys().copied().collect();
        keys.extend(map_b.keys().copied());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(key), map_b.get(key)) {
                (Some(old), Some(new)) => {
                    let mut changes: Vec<String> = Vec::new();
                    if old.broken != new.broken {
                        changes.push(if new.broken {
                            "链接已失效".to_string()
                        } else {
                            "链接已恢复".to_string()
                        });
                    }
                    if old.bytes != new.bytes {
                        changes.push(format!(
                            "{} → {}",
                            bytes_label(old.bytes),
                            bytes_label(new.bytes)
                        ));
                    }
                    if old.link_kind != new.link_kind {
                        changes.push(format!(
                            "形态 {} → {}",
                            old.link_kind.as_deref().unwrap_or("目录"),
                            new.link_kind.as_deref().unwrap_or("目录")
                        ));
                    }
                    if !changes.is_empty() {
                        let entry = DiffEntry {
                            key: key.to_string(),
                            label: new.name.clone(),
                            detail: changes.join(" · "),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let owner = if new.source_agent.is_empty() {
                        String::new()
                    } else {
                        format!(" @ {}", new.source_agent)
                    };
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: new.name.clone(),
                        detail: format!("新增{}{}", owner, if new.broken { "（失效链接）" } else { "" }),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: old.name.clone(),
                        detail: "已移除".into(),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    /* ---------------- MCP：来源 Agent + 名称 ---------------- */
    {
        let key_of = |x: &crate::model::McpServerFound| format!("{}/{}", x.source_agent_id, x.name);
        let map_a: std::collections::HashMap<String, _> =
            a.mcp_servers.iter().map(|x| (key_of(x), x)).collect();
        let map_b: std::collections::HashMap<String, _> =
            b.mcp_servers.iter().map(|x| (key_of(x), x)).collect();
        let mut sec = DiffSection {
            resource: "mcp".into(),
            title: "MCP 服务器".into(),
            ..Default::default()
        };
        let mut keys: Vec<String> = map_a.keys().cloned().collect();
        keys.extend(map_b.keys().cloned());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(&key), map_b.get(&key)) {
                (Some(old), Some(new)) => {
                    let mut changes: Vec<String> = Vec::new();
                    if old.transport != new.transport {
                        changes.push(format!(
                            "传输方式 {} → {}",
                            old.transport, new.transport
                        ));
                    }
                    if old.command != new.command {
                        changes.push(format!(
                            "命令 {} → {}",
                            old.command.as_deref().unwrap_or("—"),
                            new.command.as_deref().unwrap_or("—")
                        ));
                    }
                    if old.url != new.url {
                        changes.push(format!(
                            "URL {} → {}",
                            old.url.as_deref().unwrap_or("—"),
                            new.url.as_deref().unwrap_or("—")
                        ));
                    }
                    if !changes.is_empty() {
                        let entry = DiffEntry {
                            key: key.clone(),
                            label: format!("{} @ {}", new.name, new.source_agent),
                            detail: changes.join(" · "),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let entry = DiffEntry {
                        key: key.clone(),
                        label: format!("{} @ {}", new.name, new.source_agent),
                        detail: format!("新增（{}）", new.transport),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.clone(),
                        label: format!("{} @ {}", old.name, old.source_agent),
                        detail: "已移除".into(),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    /* ---------------- 供应商线索 ---------------- */
    {
        let map_a: std::collections::HashMap<&str, _> =
            a.provider_hints.iter().map(|x| (x.id.as_str(), x)).collect();
        let map_b: std::collections::HashMap<&str, _> =
            b.provider_hints.iter().map(|x| (x.id.as_str(), x)).collect();
        let mut sec = DiffSection {
            resource: "providers".into(),
            title: "供应商线索".into(),
            ..Default::default()
        };
        let mut keys: Vec<&str> = map_a.keys().copied().collect();
        keys.extend(map_b.keys().copied());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(key), map_b.get(key)) {
                (Some(_), Some(new)) => {
                    let old = map_a.get(key).unwrap();
                    if old.value_masked != new.value_masked || old.kind != new.kind {
                        let entry = DiffEntry {
                            key: key.to_string(),
                            label: new.label.clone(),
                            detail: format!(
                                "{} {} → {}",
                                new.kind, old.value_masked, new.value_masked
                            ),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: new.label.clone(),
                        detail: format!("新增线索（{}）", new.kind),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: old.label.clone(),
                        detail: "已移除".into(),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    /* ---------------- Python 环境 ---------------- */
    {
        let map_a: std::collections::HashMap<&str, _> =
            a.python_envs.iter().map(|x| (x.path.as_str(), x)).collect();
        let map_b: std::collections::HashMap<&str, _> =
            b.python_envs.iter().map(|x| (x.path.as_str(), x)).collect();
        let mut sec = DiffSection {
            resource: "python".into(),
            title: "Python 环境".into(),
            ..Default::default()
        };
        let mut keys: Vec<&str> = map_a.keys().copied().collect();
        keys.extend(map_b.keys().copied());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(key), map_b.get(key)) {
                (Some(old), Some(new)) => {
                    let mut changes: Vec<String> = Vec::new();
                    if old.python_version != new.python_version {
                        changes.push(format!(
                            "Python {} → {}",
                            old.python_version.as_deref().unwrap_or("—"),
                            new.python_version.as_deref().unwrap_or("—")
                        ));
                    }
                    if old.package_count != new.package_count {
                        changes.push(format!(
                            "包数量 {} → {}",
                            old.package_count.map(|n| n.to_string()).unwrap_or("—".into()),
                            new.package_count.map(|n| n.to_string()).unwrap_or("—".into())
                        ));
                    }
                    if !changes.is_empty() {
                        let entry = DiffEntry {
                            key: key.to_string(),
                            label: format!("{}（{}）", new.name, new.manager),
                            detail: changes.join(" · "),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: format!("{}（{}）", new.name, new.manager),
                        detail: format!(
                            "新增{}",
                            new.python_version
                                .as_ref()
                                .map(|v| format!("（Python {}）", v))
                                .unwrap_or_default()
                        ),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.to_string(),
                        label: format!("{}（{}）", old.name, old.manager),
                        detail: "已移除".into(),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    /* ---------------- npm 全局包 ---------------- */
    {
        let key_of = |x: &crate::model::NpmPackage| format!("{}/{}", x.manager, x.name);
        let map_a: std::collections::HashMap<String, _> =
            a.npm_packages.iter().map(|x| (key_of(x), x)).collect();
        let map_b: std::collections::HashMap<String, _> =
            b.npm_packages.iter().map(|x| (key_of(x), x)).collect();
        let mut sec = DiffSection {
            resource: "npm".into(),
            title: "npm 包".into(),
            ..Default::default()
        };
        let mut keys: Vec<String> = map_a.keys().cloned().collect();
        keys.extend(map_b.keys().cloned());
        keys.sort();
        keys.dedup();
        for key in keys {
            match (map_a.get(&key), map_b.get(&key)) {
                (Some(old), Some(new)) => {
                    if old.version != new.version {
                        let entry = DiffEntry {
                            key: key.clone(),
                            label: new.name.clone(),
                            detail: format!("版本 {} → {}", old.version, new.version),
                        };
                        sec.changed.push(entry);
                    }
                }
                (None, Some(new)) => {
                    let entry = DiffEntry {
                        key: key.clone(),
                        label: new.name.clone(),
                        detail: format!("新增（{} {}）", new.manager, new.version),
                    };
                    sec.added.push(entry);
                }
                (Some(old), None) => {
                    let entry = DiffEntry {
                        key: key.clone(),
                        label: old.name.clone(),
                        detail: "已卸载".into(),
                    };
                    sec.removed.push(entry);
                }
                _ => {}
            }
        }
        if !sec.added.is_empty() || !sec.removed.is_empty() || !sec.changed.is_empty() {
            diff.sections.push(sec);
        }
    }

    let added_total: usize = diff.sections.iter().map(|s| s.added.len()).sum();
    let removed_total: usize = diff.sections.iter().map(|s| s.removed.len()).sum();
    let changed_total: usize = diff.sections.iter().map(|s| s.changed.len()).sum();
    diff.summary = if added_total == 0 && removed_total == 0 && changed_total == 0 {
        "两次扫描之间没有发现资源变化".to_string()
    } else {
        format!(
            "新增 {} · 移除 {} · 变化 {}（A {} → B {}）",
            added_total, removed_total, changed_total, a.scanned_at, b.scanned_at
        )
    };
    diff
}
