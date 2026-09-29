//! 反向生成向导（M3.5）：指向一个已有配置文件 → 解析成树 → 勾选节点
//! → 生成 Agent 定义草稿 TOML。
//!
//! 这是「接入自定义 Agent」的最短路径：不用手写 TOML，从现成配置反推。
//! 生成的草稿保证能通过 agentdef::parse（自检钉死这一点）。

use serde::{Deserialize, Serialize};
use std::path::Path;

/* ---------------------------------------------------------------- 配置树 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigNode {
    /// 节点名（根节点为文件名）
    pub key: String,
    /// object | array | value
    pub kind: String,
    /// 值类型：string | number | bool | object | array | null
    pub value_type: String,
    /// 值的截断预览（对象显示键数，数组显示长度）
    pub preview: String,
    /// object 的子节点（深度与数量上限保护）
    pub children: Vec<ConfigNode>,
    /// 对象的子项看起来像 MCP 服务器（有 command / url），界面据此高亮
    pub has_mcp_shape: bool,
}

const MAX_DEPTH: usize = 6;
const MAX_CHILDREN: usize = 40;
const MAX_TOTAL: usize = 400;

/// 解析配置文件（json / toml）为树。读取失败或格式不支持都给出明确错误。
pub fn parse_config_tree(path: &str) -> Result<ConfigNode, String> {
    let file = Path::new(path);
    if !file.is_file() {
        return Err(format!("文件不存在：{}", path));
    }
    let text =
        std::fs::read_to_string(file).map_err(|e| format!("读取失败：{}", e))?;
    let format = file
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let value: serde_json::Value = match format.as_str() {
        "json" => serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败：{}", e))?,
        "toml" => toml::from_str::<toml::Value>(&text)
            .map_err(|e| format!("TOML 解析失败：{}", e))
            .and_then(|v| serde_json::to_value(v).map_err(|e| format!("TOML 转换失败：{}", e)))?,
        other => {
            return Err(format!(
                "暂不支持的格式 .{}（当前支持 JSON 与 TOML）",
                other
            ))
        }
    };
    let name = file
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".into());
    let mut total = 0usize;
    Ok(build_node(&name, &value, 0, &mut total))
}

fn build_node(key: &str, value: &serde_json::Value, depth: usize, total: &mut usize) -> ConfigNode {
    *total += 1;
    let mut node = ConfigNode {
        key: key.to_string(),
        ..Default::default()
    };
    match value {
        serde_json::Value::Object(map) => {
            node.kind = "object".into();
            node.value_type = "object".into();
            node.preview = format!("{} 个键", map.len());
            node.has_mcp_shape = object_has_mcp_shape(map);
            if depth < MAX_DEPTH && *total < MAX_TOTAL {
                for (k, v) in map.iter().take(MAX_CHILDREN) {
                    node.children.push(build_node(k, v, depth + 1, total));
                }
            }
        }
        serde_json::Value::Array(arr) => {
            node.kind = "array".into();
            node.value_type = "array".into();
            node.preview = format!("{} 项", arr.len());
            if depth < MAX_DEPTH && *total < MAX_TOTAL {
                for (i, v) in arr.iter().take(MAX_CHILDREN).enumerate() {
                    node.children
                        .push(build_node(&format!("[{}]", i), v, depth + 1, total));
                }
            }
        }
        other => {
            node.kind = "value".into();
            node.value_type = type_name(other).to_string();
            node.preview = preview_text(other);
        }
    }
    node
}

fn type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
    }
}

fn preview_text(value: &serde_json::Value) -> String {
    let text = match value {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if text.chars().count() > 60 {
        format!("{}…", text.chars().take(60).collect::<String>())
    } else {
        text
    }
}

/// 对象的子项是否「长得像 MCP 服务器集合」：子对象带 command 或 url
fn object_has_mcp_shape(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    let mut serverish = 0;
    let mut objects = 0;
    for v in map.values() {
        if let Some(child) = v.as_object() {
            objects += 1;
            if child.contains_key("command") || child.contains_key("url") {
                serverish += 1;
            }
        }
    }
    objects >= 1 && serverish == objects
}

/* ------------------------------------------------------------ 草稿生成 */

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DraftRequest {
    pub file: String,
    pub format: String,
    pub agent_id: String,
    pub agent_name: String,
    /// cli | ide | extension | host
    pub kind: String,
    /// 选中的 MCP 配置节点（点号路径，如 "mcpServers" 或 "mcp"）
    pub mcp_root: Option<String>,
    /// 选中的供应商线索节点
    pub provider_root: Option<String>,
    /// Skills 目录（可选）
    pub skills_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftOutcome {
    pub content: String,
    /// 保存用的文件 id（= agent id）
    pub id: String,
    /// 草稿里的声明块数量
    pub mcp_declared: bool,
    pub provider_declared: bool,
}

fn toml_str(raw: &str) -> String {
    // 简单但正确的 TOML 基本字符串转义
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{}\"", escaped)
}

fn slug_id(raw: &str) -> String {
    let cleaned: String = raw
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
        "my-agent".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

/// 从配置反推 Agent 定义草稿。草稿必须能通过 agentdef::parse。
pub fn generate_definition_draft(req: &DraftRequest) -> Result<DraftOutcome, String> {
    let id = slug_id(if req.agent_id.trim().is_empty() {
        req.agent_name.trim()
    } else {
        req.agent_id.trim()
    });
    if id.is_empty() {
        return Err("请填写 Agent 名称或 id".to_string());
    }
    let name = if req.agent_name.trim().is_empty() {
        id.clone()
    } else {
        req.agent_name.trim().to_string()
    };
    let kind = match req.kind.as_str() {
        "cli" | "ide" | "extension" | "host" => req.kind.clone(),
        _ => "cli".to_string(),
    };
    let format = if req.format.trim().is_empty() {
        let ext = Path::new(&req.file)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "json".into());
        if ext == "toml" { "toml" } else { "json" }.to_string()
    } else {
        req.format.trim().to_ascii_lowercase()
    };
    if !matches!(format.as_str(), "json" | "toml") {
        return Err(format!("不支持的格式 {}（json | toml）", format));
    }
    let mcp_root = req.mcp_root.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let provider_root = req
        .provider_root
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let skills_dir = req
        .skills_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut out = String::new();
    out.push_str(&format!(
        "# AgentHub · Agent 定义 —— {}（反向生成草稿）\n# 保存到用户定义目录后生效；按需微调证据与授权。\n\n",
        id
    ));
    out.push_str(&format!(
        "[agent]\nid = {}\nname = {}\nvendor = \"\"\nkind = {}\nadapter = \"\"\n",
        toml_str(&id),
        toml_str(&name),
        toml_str(&kind)
    ));
    out.push('\n');
    out.push_str(&format!(
        "[[paths]]\nlabel = \"主配置\"\npath = {}\nrole = \"config\"\nformat = {}\n",
        toml_str(&req.file),
        toml_str(&format)
    ));
    out.push('\n');
    if let Some(root) = mcp_root {
        out.push_str(&format!(
            "[[mcp]]\nfile = {}\nroot = {}\nformat = {}\n",
            toml_str(&req.file),
            toml_str(root),
            toml_str(&format)
        ));
        out.push('\n');
    }
    if let Some(root) = provider_root {
        out.push_str(&format!(
            "[[provider]]\nfile = {}\nroot = {}\nformat = \"json\"\nkind = \"base-url\"\nbaseUrlKey = \"\"\nlabel = \"provider\"\n",
            toml_str(&req.file),
            toml_str(root)
        ));
        out.push('\n');
    }
    if let Some(dir) = skills_dir {
        out.push_str(&format!(
            "[[paths]]\nlabel = \"Skills 目录\"\npath = {}\nrole = \"skills\"\ndeploy = \"link\"\n",
            toml_str(dir)
        ));
        out.push('\n');
    }
    out.push_str(&format!(
        "[capabilities]\nmaxTier = \"parse\"{}\n",
        if skills_dir.is_some() {
            "\nskillMethods = [\"link\", \"copy\"]"
        } else {
            ""
        }
    ));

    // 草稿必须可解析 —— 在这里就把关，不把坏草稿交给用户
    crate::agentdef::parse(&out).map_err(|e| format!("生成的草稿未通过校验（这是 bug）：{}", e))?;

    Ok(DraftOutcome {
        content: out,
        id,
        mcp_declared: mcp_root.is_some(),
        provider_declared: provider_root.is_some(),
    })
}
