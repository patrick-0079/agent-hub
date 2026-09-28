//! 极简 YAML 块提取。
//!
//! 只支持「按缩进定位到某个节点，取其直接子键（含标量字段与列表项）」，
//! 足以覆盖各 Agent 的 MCP / provider 配置结构；完整 YAML 解析与模版渲染
//! 留待 M2 的 Adapter 引擎（Tera）统一处理。

#[derive(Debug, Clone, Default)]
pub struct YamlEntry {
    pub name: String,
    /// 直接子字段的标量值（按出现顺序）
    pub fields: Vec<(String, String)>,
    /// 形如 `- item` 的列表项
    pub items: Vec<String>,
}

impl YamlEntry {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

struct Line {
    indent: usize,
    key: String,
    value: String,
}

fn parse_lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let without_comment = match raw.find('#') {
            // 只把 ` #` 视为注释起点，避免误伤值里的 #
            Some(idx) if idx == 0 || raw.as_bytes()[idx - 1] == b' ' => &raw[..idx],
            _ => raw,
        };
        if without_comment.trim().is_empty() {
            continue;
        }
        let indent = without_comment.len() - without_comment.trim_start().len();
        let content = without_comment.trim();
        if let Some((k, v)) = content.split_once(':') {
            out.push(Line {
                indent,
                key: k.trim().trim_matches('"').to_string(),
                value: v.trim().trim_matches('"').to_string(),
            });
        } else {
            out.push(Line {
                indent,
                key: String::new(),
                value: content.trim_start_matches("- ").trim().trim_matches('"').to_string(),
            });
        }
    }
    out
}

/// 取 `root`（点号分隔）节点下的直接子条目。
pub fn block_entries(text: &str, root: &str) -> Vec<YamlEntry> {
    let lines = parse_lines(text);
    let segments: Vec<&str> = root.split('.').filter(|s| !s.is_empty()).collect();

    // 逐级定位 root 节点
    let mut cursor = 0usize;
    let mut block_indent: Option<usize> = None;
    for segment in &segments {
        let mut found = None;
        for (idx, line) in lines.iter().enumerate().skip(cursor) {
            let deeper = block_indent.map(|b| line.indent > b).unwrap_or(true);
            if line.key == *segment && deeper && line.value.is_empty() {
                found = Some((idx, line.indent));
                break;
            }
        }
        let Some((idx, indent)) = found else {
            return Vec::new();
        };
        cursor = idx + 1;
        block_indent = Some(indent);
    }

    let Some(base) = block_indent else {
        return Vec::new();
    };

    // 收集子条目：缩进大于 base，且条目缩进取最小值
    let tail: Vec<&Line> = {
        let mut collected = Vec::new();
        for line in lines.iter().skip(cursor) {
            if line.indent <= base {
                break;
            }
            collected.push(line);
        }
        collected
    };
    let Some(entry_indent) = tail.first().map(|l| l.indent) else {
        return Vec::new();
    };

    let mut entries: Vec<YamlEntry> = Vec::new();
    let mut current: Option<YamlEntry> = None;
    for line in tail {
        if line.indent == entry_indent && !line.key.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            let mut entry = YamlEntry {
                name: line.key.clone(),
                ..Default::default()
            };
            if !line.value.is_empty() {
                entry.fields.push(("__value".to_string(), line.value.clone()));
            }
            current = Some(entry);
        } else if let Some(entry) = current.as_mut() {
            if line.key.is_empty() {
                if !line.value.is_empty() {
                    entry.items.push(line.value.clone());
                }
            } else {
                entry.fields.push((line.key.clone(), line.value.clone()));
            }
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

/// 仅取 `root` 节点下的直接子键名（凭据类文件只读键名，不读值）
pub fn block_keys(text: &str, root: &str) -> Vec<String> {
    block_entries(text, root)
        .into_iter()
        .map(|e| e.name)
        .collect()
}