//! 模型供应商线索 —— 内核级环境变量探测 + 定义文件声明的配置来源。
//!
//! **内核不变式**：密钥类值永不落库。凡是键名命中 key / token / secret / password
//! 的条目，无论定义声明的类型是什么，一律脱敏并重新归类为 api-key。

use crate::agentdef::LoadedDef;
use crate::capability::walk_json;
use crate::model::{AgentTarget, ProviderHint};
use crate::util;
use crate::yaml;
use serde_json::Value;

struct EnvSpec {
    var: &'static str,
    label: &'static str,
    kind: &'static str,
}

/// 内核级：进程环境变量里的供应商配置（对所有 Agent 子进程同样生效）
const ENV_SPECS: &[EnvSpec] = &[
    EnvSpec { var: "OPENAI_API_KEY", label: "OpenAI", kind: "api-key" },
    EnvSpec { var: "OPENAI_BASE_URL", label: "OpenAI Base URL", kind: "base-url" },
    EnvSpec { var: "OPENAI_MODEL", label: "OpenAI 默认模型", kind: "model" },
    EnvSpec { var: "ANTHROPIC_API_KEY", label: "Anthropic", kind: "api-key" },
    EnvSpec { var: "ANTHROPIC_BASE_URL", label: "Anthropic Base URL", kind: "base-url" },
    EnvSpec { var: "ANTHROPIC_MODEL", label: "Anthropic 默认模型", kind: "model" },
    EnvSpec { var: "DEEPSEEK_API_KEY", label: "DeepSeek", kind: "api-key" },
    EnvSpec { var: "DEEPSEEK_BASE_URL", label: "DeepSeek Base URL", kind: "base-url" },
    EnvSpec { var: "GEMINI_API_KEY", label: "Google Gemini", kind: "api-key" },
    EnvSpec { var: "GOOGLE_API_KEY", label: "Google API", kind: "api-key" },
    EnvSpec { var: "OPENROUTER_API_KEY", label: "OpenRouter", kind: "api-key" },
    EnvSpec { var: "AZURE_OPENAI_API_KEY", label: "Azure OpenAI", kind: "api-key" },
    EnvSpec { var: "AZURE_OPENAI_ENDPOINT", label: "Azure OpenAI Endpoint", kind: "base-url" },
    EnvSpec { var: "MISTRAL_API_KEY", label: "Mistral", kind: "api-key" },
    EnvSpec { var: "GROQ_API_KEY", label: "Groq", kind: "api-key" },
    EnvSpec { var: "XAI_API_KEY", label: "xAI", kind: "api-key" },
    EnvSpec { var: "MOONSHOT_API_KEY", label: "Moonshot", kind: "api-key" },
    EnvSpec { var: "DASHSCOPE_API_KEY", label: "DashScope (阿里)", kind: "api-key" },
    EnvSpec { var: "SILICONFLOW_API_KEY", label: "SiliconFlow", kind: "api-key" },
    EnvSpec { var: "OLLAMA_HOST", label: "Ollama", kind: "base-url" },
];

pub fn scan_providers(
    defs: &[LoadedDef],
    agent_list: &[AgentTarget],
    warnings: &mut Vec<String>,
) -> Vec<ProviderHint> {
    let mut out: Vec<ProviderHint> = Vec::new();

    // 1) 环境变量（内核级，不依赖任何 Agent 定义）
    for spec in ENV_SPECS {
        if let Ok(value) = std::env::var(spec.var) {
            if value.trim().is_empty() {
                continue;
            }
            out.push(ProviderHint {
                id: format!("env::{}", spec.var),
                label: spec.label.to_string(),
                kind: spec.kind.to_string(),
                value_masked: present(&value, spec.kind),
                source: "环境变量".to_string(),
                env_var: Some(spec.var.to_string()),
            });
        }
    }

    // 2) 定义文件声明的来源
    for def in defs {
        let agent_id = &def.file.agent.id;
        let Some(agent) = agent_list.iter().find(|a| &a.id == agent_id) else {
            continue;
        };
        if agent.status == "absent" {
            continue;
        }

        for source in &def.file.provider {
            let path = crate::agentdef::resolve_path(&source.file);
            if !path.is_file() {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warnings.push(format!(
                        "{} 的供应商配置读取失败：{}（{}）",
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
            let source_label = if source.label.is_empty() {
                agent.name.clone()
            } else {
                source.label.clone()
            };

            for (name, entry) in provider_entries(&text, &format, &source.root) {
                // 只读键名（凭据类文件）：值永不进入内存快照
                if source.keys_only {
                    out.push(ProviderHint {
                        id: format!("{}::{}::keys::{}", agent_id, source_label, name),
                        label: format!("{} · {}", source_label, name),
                        kind: source.kind.clone(),
                        value_masked: "••••••••（仅读取键名，值不落库）".to_string(),
                        source: path.to_string_lossy().to_string(),
                        env_var: Some(name),
                    });
                    continue;
                }

                let own_value = entry
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        entry
                            .get("value")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();

                let base_url = if source.base_url_key.is_empty() {
                    String::new()
                } else {
                    walk_json(&entry, &source.base_url_key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                let models = if source.models_key.is_empty() {
                    0
                } else {
                    entry
                        .get(&source.models_key)
                        .and_then(|v| v.as_object())
                        .map(|m| m.len())
                        .unwrap_or(0)
                };

                let raw_value = if !base_url.is_empty() {
                    base_url
                } else {
                    own_value
                };

                // 内核安全不变式：键名像密钥就一定脱敏
                let (kind, value) = if looks_secret(&name) {
                    ("api-key".to_string(), util::mask_secret(&raw_value))
                } else {
                    (source.kind.clone(), present(&raw_value, &source.kind))
                };

                let value_masked = if value.is_empty() {
                    if models > 0 {
                        format!("（已配置，声明 {} 个模型）", models)
                    } else {
                        "（已配置）".to_string()
                    }
                } else if models > 0 {
                    format!("{} （声明 {} 个模型）", value, models)
                } else {
                    value
                };

                out.push(ProviderHint {
                    id: format!("{}::{}::{}", agent_id, source_label, name),
                    label: format!("{} · {}", source_label, name),
                    kind,
                    value_masked,
                    source: path.to_string_lossy().to_string(),
                    env_var: None,
                });
            }
        }
    }

    out.sort_by(|a, b| a.label.cmp(&b.label));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// 按声明格式与节点路径取出条目
fn provider_entries(text: &str, format: &str, root: &str) -> Vec<(String, Value)> {
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
                let mut map = serde_json::Map::new();
                for entry in entries {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in &entry.fields {
                        obj.insert(
                            if k == "__value" { "value" } else { k }.to_string(),
                            serde_json::json!(v),
                        );
                    }
                    map.insert(entry.name.clone(), Value::Object(obj));
                }
                Some(Value::Object(map))
            }
        }
        _ => None,
    };

    match node {
        Some(Value::Object(map)) => map.into_iter().collect(),
        _ => Vec::new(),
    }
}

fn looks_secret(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("key") || k.contains("token") || k.contains("secret") || k.contains("password")
}

fn present(value: &str, kind: &str) -> String {
    match kind {
        "api-key" | "credential" => util::mask_secret(value),
        _ => {
            if value.chars().count() > 140 {
                format!("{}…", value.chars().take(140).collect::<String>())
            } else {
                value.to_string()
            }
        }
    }
}