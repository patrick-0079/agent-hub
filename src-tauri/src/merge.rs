//! 配置合并引擎（T2 的写入内核）。
//!
//! 内核只提供两种写入策略，具体用哪个由 Agent 定义里的 `[[mcp]]` 声明：
//!
//! * **merge-keys** —— 结构化文件（JSON）的节点级合并：只在声明的 `root` 节点下
//!   增删改「本软件管理的键」，用户手写的其它内容原样保留。上次同步过、本次已移除的
//!   键会被清理（需要 `managed_before` 记录）。
//! * **managed-block** —— 文本文件的托管区块（TOML / env / text）：用标记包围，
//!   只替换标记之间的内容，标记之外一字不动。
//!
//! 同时提供行级 unified diff，供界面「写入前先看 diff」。

use crate::model::MergeChange;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub struct MergeOutcome {
    pub new_text: String,
    pub changes: Vec<MergeChange>,
    pub warnings: Vec<String>,
}

impl MergeOutcome {
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let count = |kind: &str| self.changes.iter().filter(|c| c.kind == kind).count();
        (
            count("add"),
            count("update"),
            count("remove"),
            count("skipped"),
            count("unchanged"),
        )
    }
}

/// 在对象内按点号路径取值（`options.baseURL` 这类嵌套键）
fn get_path<'a>(obj: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut current: &Value = obj.get(*segments.first()?)?;
    for segment in &segments[1..] {
        current = current.get(*segment)?;
    }
    Some(current)
}

/// 在对象内按点号路径设值，中间层不存在时自动创建
fn set_path(obj: &mut Map<String, Value>, path: &str, value: Value) -> Option<Value> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }
    if segments.len() == 1 {
        return obj.insert(segments[0].to_string(), value);
    }
    let mut current = obj;
    for segment in &segments[..segments.len() - 1] {
        let entry = current
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry.as_object_mut()?;
    }
    current.insert(segments[segments.len() - 1].to_string(), value)
}

/// 按点号路径删除，父对象空了不清理（保持用户其它键不动）
fn remove_path(obj: &mut Map<String, Value>, path: &str) -> Option<Value> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.len() <= 1 {
        return obj.remove(*segments.first()?);
    }
    let mut current = obj;
    for segment in &segments[..segments.len() - 1] {
        current = current.get_mut(*segment)?.as_object_mut()?;
    }
    current.remove(segments[segments.len() - 1])
}

/// 按点号路径下钻到节点；`create` 为真时缺失的中间节点会被创建
fn navigate<'v>(root: &'v mut Value, path: &str, create: bool) -> Option<&'v mut Value> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in segments {
        if !current.is_object() {
            if create {
                *current = Value::Object(Map::new());
            } else {
                return None;
            }
        }
        let obj = current.as_object_mut()?;
        if !obj.contains_key(segment) {
            if create {
                obj.insert(segment.to_string(), Value::Object(Map::new()));
            } else {
                return None;
            }
        }
        current = obj.get_mut(segment)?;
    }
    Some(current)
}

/// JSON 节点级合并。
///
/// * `desired` —— 本次要写入的键（键名 → 值）
/// * `managed_before` —— 上次同步时由本软件写入的键（用于清理已移除项）
/// * `overwrite_unmanaged` —— 是否允许覆盖「同名但从未由本软件管理」的键。
///   默认 **false**：这类键会被跳过并在 diff 中标注，避免把用户手写条目的特有字段冲掉。
pub fn merge_json_keys(
    existing: &str,
    root_path: &str,
    desired: &BTreeMap<String, Value>,
    managed_before: &[String],
    overwrite_unmanaged: bool,
) -> Result<MergeOutcome, String> {
    let mut root: Value = if existing.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(existing).map_err(|e| format!("目标文件不是合法 JSON：{}", e))?
    };

    let mut changes: Vec<MergeChange> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let node = navigate(&mut root, root_path, true)
        .ok_or_else(|| format!("无法定位节点 {}（中间层不是对象）", root_path))?;
    if !node.is_object() {
        *node = Value::Object(Map::new());
    }
    let obj = node.as_object_mut().expect("已确保为对象");

    // 1) 写入 / 更新期望的键
    let mut skipped_unmanaged: Vec<String> = Vec::new();
    for (key, value) in desired {
        let was_managed = managed_before.iter().any(|k| k == key);
        match get_path(obj, key) {
            Some(current) if current == value => changes.push(MergeChange {
                kind: "unchanged".into(),
                key: key.clone(),
                detail: "内容一致，无需修改".into(),
            }),
            Some(current) if !was_managed && !overwrite_unmanaged => {
                // 用户手写的条目：默认不动它，避免丢掉我们渲染不出来的字段
                skipped_unmanaged.push(key.clone());
                changes.push(MergeChange {
                    kind: "skipped".into(),
                    key: key.clone(),
                    detail: format!(
                        "已存在且非 AgentHub 管理，已跳过（保留原内容 {}）",
                        compact(current, 80)
                    ),
                });
            }
            Some(current) => {
                changes.push(MergeChange {
                    kind: "update".into(),
                    key: key.clone(),
                    detail: format!("{} → {}", compact(current, 90), compact(value, 90)),
                });
                set_path(obj, key, value.clone());
            }
            None => {
                changes.push(MergeChange {
                    kind: "add".into(),
                    key: key.clone(),
                    detail: compact(value, 120),
                });
                set_path(obj, key, value.clone());
            }
        }
    }
    if !skipped_unmanaged.is_empty() {
        warnings.push(format!(
            "{} 个同名条目已存在但不是 AgentHub 管理的，已跳过：{}。如需用资源库的定义覆盖它们，请在向导里勾选「覆盖同名非受管条目」",
            skipped_unmanaged.len(),
            skipped_unmanaged.join("、")
        ));
    }

    // 2) 清理上次由我们写入、本次已不在期望集合里的键
    for key in managed_before {
        if desired.contains_key(key) {
            continue;
        }
        if remove_path(obj, key).is_some() {
            changes.push(MergeChange {
                kind: "remove".into(),
                key: key.clone(),
                detail: "上次由 AgentHub 写入，本次已从资源库移除，因此清理".into(),
            });
        }
    }

    let mut new_text = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("序列化失败：{}", e))?;
    if existing.ends_with('\n') || existing.is_empty() {
        new_text.push('\n');
    }

    Ok(MergeOutcome {
        new_text,
        changes,
        warnings,
    })
}

fn compact(value: &Value, max: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text
    }
}

/* -------------------------------------------------------- 托管区块 */

const BLOCK_PREFIX: &str = "# BEGIN AgentHub managed:";
const BLOCK_SUFFIX: &str = "# END AgentHub managed:";

/// 文本文件的托管区块：只替换标记之间的内容
pub fn managed_block(
    existing: &str,
    marker: &str,
    body: &str,
) -> Result<MergeOutcome, String> {
    let begin = format!("{}{}", BLOCK_PREFIX, marker);
    let end = format!("{}{}", BLOCK_SUFFIX, marker);

    let normalized = existing.replace("\r\n", "\n");
    let block = format!("{}\n{}\n{}\n", begin, body.trim_end(), end);

    let (new_text, had_block) = if normalized.contains(&begin) {
        let start = normalized.find(&begin).unwrap();
        let after_begin = &normalized[start..];
        match after_begin.find(&end) {
            Some(rel_end) => {
                let end_pos = start + rel_end + end.len();
                let mut out = String::new();
                out.push_str(&normalized[..start]);
                out.push_str(&block);
                out.push_str(normalized[end_pos..].trim_start_matches('\n'));
                (out, true)
            }
            None => return Err("发现托管块起始标记但没有结束标记，已中止以免破坏文件".to_string()),
        }
    } else {
        let mut out = normalized.clone();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&block);
        (out, false)
    };

    let mut warnings = Vec::new();
    if !had_block {
        warnings.push("目标文件中尚未有托管块，将在文件末尾新增（不会改动任何原有内容）".to_string());
    }
    // 检查正文里的键是否与用户手写内容冲突（仅提示，不自动处理）
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && normalized.contains(trimmed) {
            warnings.push(format!(
                "「{}」在文件中已存在同名段落 —— TOML 不允许重复定义，请先在 diff 中确认",
                trimmed
            ));
        }
    }

    Ok(MergeOutcome {
        changes: vec![MergeChange {
            kind: if had_block { "update".into() } else { "add".into() },
            key: marker.to_string(),
            detail: format!("托管区块（{} 行）", body.lines().count()),
        }],
        warnings,
        new_text,
    })
}

/* ------------------------------------------------------------ diff */

/// 行级 unified diff（供界面「写入前先看 diff」）
pub fn unified_diff(old: &str, new: &str, context: usize) -> String {
    // 先绑定归一化结果，避免 lines() 借用到临时值
    let old_norm = old.replace("\r\n", "\n");
    let new_norm = new.replace("\r\n", "\n");
    let a: Vec<&str> = old_norm.lines().collect();
    let b: Vec<&str> = new_norm.lines().collect();
    if a == b {
        return String::new();
    }

    // LCS 动态规划（文件不大，直接算）
    let n = a.len();
    let m = b.len();
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    // 产出操作序列
    #[derive(PartialEq)]
    enum Op<'s> {
        Keep(&'s str),
        Del(&'s str),
        Add(&'s str),
    }
    let mut ops: Vec<Op> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(Op::Keep(a[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(Op::Del(a[i]));
            i += 1;
        } else {
            ops.push(Op::Add(b[j]));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Del(a[i]));
        i += 1;
    }
    while j < m {
        ops.push(Op::Add(b[j]));
        j += 1;
    }

    // 只保留变更附近 context 行
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Keep(_)))
        .map(|(idx, _)| idx)
        .collect();
    let mut keep = vec![false; ops.len()];
    for idx in changed {
        let lo = idx.saturating_sub(context);
        let hi = (idx + context).min(ops.len().saturating_sub(1));
        for k in lo..=hi {
            keep[k] = true;
        }
    }

    let mut out = String::new();
    let mut skipped = false;
    for (idx, op) in ops.iter().enumerate() {
        if !keep[idx] {
            if !skipped {
                out.push_str("@@ …\n");
                skipped = true;
            }
            continue;
        }
        skipped = false;
        match op {
            Op::Keep(line) => out.push_str(&format!(" {}\n", line)),
            Op::Del(line) => out.push_str(&format!("-{}\n", line)),
            Op::Add(line) => out.push_str(&format!("+{}\n", line)),
        }
    }
    out
}