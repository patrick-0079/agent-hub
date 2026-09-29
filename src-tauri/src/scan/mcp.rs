//! MCP 配置解析 —— 完全按定义文件声明的位置与节点提取，不做格式猜测。
//!
//! 定义里的 `[[mcp]]` 明确给出 file / root / format，内核只负责按 root 下钻、
//! 把每个条目规范化成统一结构（含 opencode 的 `command` 数组与 local/remote 类型）。

use crate::agentdef::LoadedDef;
use crate::capability::walk_json;
use crate::model::{AgentTarget, McpServerFound};
use crate::yaml;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub fn scan_mcp(
    defs: &[LoadedDef],
    agent_list: &[AgentTarget],
    warnings: &mut Vec<String>,
) -> Vec<McpServerFound> {
    let mut out: Vec<McpServerFound> = Vec::new();

    for def in defs {
        let agent_id = &def.file.agent.id;
        let Some(agent) = agent_list.iter().find(|a| &a.id == agent_id) else {
            continue;
        };
        // 有安装痕迹（已安装 / 仅配置 / 残留）都解析，便于发现陈旧配置
        if agent.status == "absent" {
            continue;
        }

        for source in &def.file.mcp {
            let path = resolve(&source.file);
            if !path.is_file() {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warnings.push(format!(
                        "{} 的 MCP 配置读取失败：{}（{}）",
                        agent.name,
                        path.to_string_lossy(),
                        e
                    ));
                    continue;
                }
            };
            let format = if source.format.is_empty() {
                path.extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default()
            } else {
                source.format.clone()
            };

            for (name, entry) in extract(&text, &format, &source.root) {
                out.push(build(agent_id, &agent.name, &path, &name, entry));
            }
        }
    }

    out.sort_by(|a, b| {
        (a.source_file.clone(), a.name.clone()).cmp(&(b.source_file.clone(), b.name.clone()))
    });
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn resolve(raw: &str) -> PathBuf {
    if raw.starts_with("./") || raw.starts_with(".\\") {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(raw.trim_start_matches("./").trim_start_matches(".\\"))
    } else {
        crate::util::expand_buf(raw)
    }
}

/// 按声明格式与节点路径提取条目
fn extract(text: &str, format: &str, root: &str) -> Vec<(String, Value)> {
    let node: Option<Value> = match format {
        "json" => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|v| walk_json(&v, root).cloned()),
        "toml" => toml::from_str::<toml::Value>(text)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok())
            .and_then(|v| walk_json(&v, root).cloned()),
        "yaml" | "yml" => {
            let entries = yaml::block_entries(text, root);
            if entries.is_empty() {
                None
            } else {
                let mut map = Map::new();
                for entry in entries {
                    map.insert(entry.name.clone(), yaml_entry_to_json(&entry));
                }
                Some(Value::Object(map))
            }
        }
        _ => None,
    };

    let Some(Value::Object(map)) = node else {
        return Vec::new();
    };
    map.into_iter()
        .filter(|(_, v)| looks_like_server(v))
        .collect()
}

fn yaml_entry_to_json(entry: &yaml::YamlEntry) -> Value {
    let mut map = Map::new();
    for (key, value) in &entry.fields {
        if key == "__value" {
            continue;
        }
        map.insert(key.clone(), json!(value));
    }
    if !entry.items.is_empty() {
        map.insert("args".to_string(), json!(entry.items));
    }
    Value::Object(map)
}

fn looks_like_server(value: &Value) -> bool {
    value.is_object()
        && (value.get("command").is_some()
            || value.get("url").is_some()
            || value.get("serverUrl").is_some()
            || value.get("type").is_some()
            || value.get("transport").is_some())
}

fn build(
    agent_id: &str,
    agent_name: &str,
    path: &std::path::Path,
    name: &str,
    entry: Value,
) -> McpServerFound {
    let mut command = entry
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut args: Vec<String> = entry
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // opencode / 部分 Agent 把 command 写成数组：首元素是程序，其余是参数
    if command.is_none() {
        if let Some(arr) = entry.get("command").and_then(|v| v.as_array()) {
            let mut items = arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string()));
            command = items.next();
            args.extend(items);
        }
    }

    let url = entry
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 传输类型：opencode 用 local/remote，其他 Agent 用 stdio/http/sse
    let declared = entry
        .get("type")
        .or_else(|| entry.get("transport"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let transport = match declared.as_str() {
        "local" => "stdio".to_string(),
        "remote" => "http".to_string(),
        "" => {
            if command.is_some() {
                "stdio".to_string()
            } else if let Some(u) = &url {
                if u.contains("/sse") {
                    "sse".to_string()
                } else {
                    "http".to_string()
                }
            } else {
                "unknown".to_string()
            }
        }
        other => other.to_string(),
    };

    // 环境变量键名：env（Claude 系）/ environment（opencode 系）
    let env_keys = entry
        .get("env")
        .or_else(|| entry.get("environment"))
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    // http/sse 请求头：只提取键名（值可能是凭证，与 env 同一纪律不读值）
    let header_keys = entry
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let file_str = path.to_string_lossy().to_string();
    McpServerFound {
        id: format!("{}::{}::{}", agent_id, file_str, name),
        name: name.to_string(),
        source_agent: agent_name.to_string(),
        source_agent_id: agent_id.to_string(),
        source_file: file_str,
        transport,
        command,
        args,
        url,
        env_keys,
        header_keys,
        raw: sanitize(&entry),
    }
}

/// 去掉超长字符串，避免把整个文件塞进快照
fn sanitize(value: &Value) -> Value {
    match value {
        Value::String(s) => {
            if s.chars().count() > 300 {
                json!(format!("{}…（已截断）", s.chars().take(300).collect::<String>()))
            } else {
                json!(s)
            }
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize).collect()),
        Value::Object(obj) => {
            let mut m = Map::new();
            for (k, v) in obj {
                m.insert(k.clone(), sanitize(v));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}