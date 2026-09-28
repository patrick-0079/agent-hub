//! npm / pnpm 全局包清单。

use crate::model::{ExecutableInfo, NpmPackage};
use crate::util;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub fn scan_npm_packages(
    executables: &[ExecutableInfo],
    warnings: &mut Vec<String>,
) -> Vec<NpmPackage> {
    let mut out: Vec<NpmPackage> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    if let Some(npm) = exe_of(executables, "npm") {
        let root = util::run_capture(&npm, &["root", "-g"], Duration::from_secs(30))
            .ok()
            .map(|s| s.trim().to_string());
        match util::run_capture(&npm, &["ls", "-g", "--depth=0", "--json"], Duration::from_secs(60)) {
            Ok(text) => parse_npm_json(&text, root.as_deref(), &mut out),
            Err(e) => errors.push(format!("npm: {}", e)),
        }
    }

    if let Some(pnpm) = exe_of(executables, "pnpm") {
        let root = util::run_capture(&pnpm, &["root", "-g"], Duration::from_secs(30))
            .ok()
            .map(|s| s.trim().to_string());
        match util::run_capture(&pnpm, &["ls", "-g", "--depth=0", "--json"], Duration::from_secs(60)) {
            Ok(text) => parse_pnpm_json(&text, root.as_deref(), &mut out),
            Err(e) => errors.push(format!("pnpm: {}", e)),
        }
    }

    if !errors.is_empty() {
        warnings.push(format!("全局包清单部分读取失败 —— {}", errors.join("；")));
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name && a.manager == b.manager);
    out
}

fn exe_of(executables: &[ExecutableInfo], name: &str) -> Option<PathBuf> {
    executables
        .iter()
        .find(|e| e.name == name && e.found)
        .and_then(|e| e.path.clone())
        .map(PathBuf::from)
}

fn mk(name: &str, version: &str, manager: &str, location: Option<&str>) -> NpmPackage {
    let lower = name.to_ascii_lowercase();
    NpmPackage {
        name: name.to_string(),
        version: version.to_string(),
        manager: manager.to_string(),
        scope: "global".to_string(),
        location: location.map(|s| s.to_string()),
        mcp_capable: lower.contains("mcp") || lower.starts_with("@modelcontextprotocol"),
    }
}

/// npm: `{ "dependencies": { "pkg": { "version": "1.2.3" } } }`
fn parse_npm_json(text: &str, root: Option<&str>, out: &mut Vec<NpmPackage>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(deps) = value.get("dependencies").and_then(|d| d.as_object()) else {
        return;
    };
    for (name, meta) in deps {
        let version = meta
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        out.push(mk(name, version, "npm", root));
    }
}

/// pnpm: `[ { "path": "...", "dependencies": { ... } } ]`
fn parse_pnpm_json(text: &str, root: Option<&str>, out: &mut Vec<NpmPackage>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let mut absorbers: Vec<&Value> = Vec::new();
    match &value {
        Value::Array(items) => absorbers.extend(items.iter()),
        Value::Object(_) => absorbers.push(&value),
        _ => return,
    }
    for node in absorbers {
        let location = node
            .get("path")
            .and_then(|v| v.as_str())
            .or(root)
            .map(|s| s.to_string());
        if let Some(deps) = node.get("dependencies").and_then(|d| d.as_object()) {
            for (name, meta) in deps {
                let version = meta
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let mut pkg = mk(name, version, "pnpm", location.as_deref());
                pkg.scope = "global".to_string();
                out.push(pkg);
            }
        }
    }
}