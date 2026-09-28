//! Python 环境枚举：conda（含全部 env）、uv（受管解释器）、venv/virtualenv（pyvenv.cfg）。

use crate::model::{AppSettings, ExecutableInfo, PythonEnv};
use crate::util;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn scan_python_envs(
    settings: &AppSettings,
    executables: &[ExecutableInfo],
    warnings: &mut Vec<String>,
) -> Vec<PythonEnv> {
    let mut out: Vec<PythonEnv> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    scan_conda(executables, &mut out, &mut seen, warnings);
    scan_uv(executables, &mut out, &mut seen, warnings);
    scan_venvs(settings, &mut out, &mut seen, warnings);

    out.sort_by(|a, b| (a.manager.clone(), a.name.clone()).cmp(&(b.manager.clone(), b.name.clone())));
    out
}

fn exe_path(executables: &[ExecutableInfo], name: &str) -> Option<PathBuf> {
    executables
        .iter()
        .find(|e| e.name == name && e.found)
        .and_then(|e| e.path.clone())
        .map(PathBuf::from)
}

fn push_unique(out: &mut Vec<PythonEnv>, seen: &mut HashSet<String>, env: PythonEnv) {
    let key = env.path.to_ascii_lowercase();
    if seen.insert(key) {
        out.push(env);
    }
}

/// 目录名过于通用时补充父目录，避免多个 anaconda3 重名。
fn env_label(path: &Path) -> String {
    let leaf = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let generic = leaf.to_ascii_lowercase();
    if generic.starts_with("anaconda") || generic.starts_with("miniconda") {
        if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
            return format!("{}/{}", parent.to_string_lossy(), leaf);
        }
    }
    leaf
}

/* ----------------------------------------------------------------- conda */

fn scan_conda(
    executables: &[ExecutableInfo],
    out: &mut Vec<PythonEnv>,
    seen: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) {
    let Some(conda) = exe_path(executables, "conda") else {
        return;
    };
    let json = match util::run_capture(&conda, &["env", "list", "--json"], Duration::from_secs(60)) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("conda env list 执行失败：{}", e));
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => {
            warnings.push("conda env list 输出无法解析为 JSON".to_string());
            return;
        }
    };
    let active_prefix = std::env::var("CONDA_PREFIX").unwrap_or_default();

    if let Some(envs) = value.get("envs").and_then(|v| v.as_array()) {
        for item in envs {
            let Some(path_str) = item.as_str() else { continue };
            let path = PathBuf::from(path_str);
            let (py_version, pkg_count) = conda_meta_info(&path);
            push_unique(
                out,
                seen,
                PythonEnv {
                    id: format!("conda::{}", path.to_string_lossy()),
                    name: env_label(&path),
                    manager: "conda".to_string(),
                    path: path.to_string_lossy().to_string(),
                    python_version: py_version,
                    package_count: pkg_count,
                    active: !active_prefix.is_empty()
                        && path.to_string_lossy().eq_ignore_ascii_case(&active_prefix),
                    detail: Some("conda 环境".to_string()),
                },
            );
        }
    }
}

/// 从 `conda-meta/python-3.12.7-*.json` 推导解释器版本，并统计包数量。
fn conda_meta_info(env_path: &Path) -> (Option<String>, Option<usize>) {
    let meta_dir = env_path.join("conda-meta");
    let entries = match std::fs::read_dir(&meta_dir) {
        Ok(e) => e,
        Err(_) => return (None, None),
    };
    let mut count = 0usize;
    let mut version = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") {
            count += 1;
            if version.is_none() && name.starts_with("python-") {
                let rest = name.trim_start_matches("python-").trim_end_matches(".json");
                let v: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if !v.is_empty() {
                    version = Some(v);
                }
            }
        }
    }
    (version, Some(count))
}

/* -------------------------------------------------------------------- uv */

fn scan_uv(
    executables: &[ExecutableInfo],
    out: &mut Vec<PythonEnv>,
    seen: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) {
    let Some(uv) = exe_path(executables, "uv") else {
        return;
    };
    let text = match util::run_capture(
        &uv,
        &["python", "list", "--only-installed"],
        Duration::from_secs(40),
    ) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("uv python list 执行失败：{}", e));
            return;
        }
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Version") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        let id_token = tokens[0];
        let path_token = tokens.last().unwrap();
        if !path_token.contains('\\') && !path_token.contains('/') {
            continue; // 例如 "<download available>"
        }
        let version = id_token
            .split('-')
            .nth(1)
            .map(|s| s.to_string())
            .or_else(|| util::extract_version(id_token));
        let path = PathBuf::from(path_token);
        let is_venv = path
            .parent()
            .map(|p| p.join("pyvenv.cfg").is_file())
            .unwrap_or(false);
        push_unique(
            out,
            seen,
            PythonEnv {
                id: format!("uv::{}", path.to_string_lossy()),
                name: if is_venv {
                    env_label(path.parent().unwrap_or(&path))
                } else {
                    id_token.to_string()
                },
                manager: "uv".to_string(),
                path: path.to_string_lossy().to_string(),
                python_version: version,
                package_count: None,
                active: std::env::var("VIRTUAL_ENV")
                    .map(|v| v.eq_ignore_ascii_case(&path.to_string_lossy()))
                    .unwrap_or(false),
                detail: Some(if is_venv {
                    "uv 虚拟环境".to_string()
                } else {
                    "uv 受管解释器".to_string()
                }),
            },
        );
    }
}

/* ------------------------------------------------------------------ venv */

fn scan_venvs(
    settings: &AppSettings,
    out: &mut Vec<PythonEnv>,
    seen: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) {
    let depth = settings.scan_home_depth.clamp(1, 6) as usize;
    let mut roots: Vec<(PathBuf, usize)> = vec![
        (util::home_dir(), depth),
        (std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), 3),
    ];
    for extra in &settings.extra_scan_roots {
        roots.push((util::expand_buf(extra), 4));
    }

    let mut found: Vec<PathBuf> = Vec::new();
    for (root, d) in roots {
        if !root.is_dir() {
            continue;
        }
        found.extend(util::find_files(&root, "pyvenv.cfg", d, 1200, util::SKIP_DIRS));
        if found.len() >= 80 {
            warnings.push("虚拟环境数量较多，已截断至 80 个（可在设置中缩小扫描范围）".to_string());
            break;
        }
    }

    for cfg in found.into_iter().take(80) {
        let Some(env_dir) = cfg.parent() else { continue };
        let text = std::fs::read_to_string(&cfg).unwrap_or_default();
        let mut version = None;
        let mut home = None;
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_ascii_lowercase();
                let val = v.trim().to_string();
                match key.as_str() {
                    "version" => version = Some(val),
                    "home" => home = Some(val),
                    _ => {}
                }
            }
        }
        let pkg_count = site_packages_count(env_dir);
        push_unique(
            out,
            seen,
            PythonEnv {
                id: format!("venv::{}", env_dir.to_string_lossy()),
                name: env_label(env_dir),
                manager: "venv".to_string(),
                path: env_dir.to_string_lossy().to_string(),
                python_version: version,
                package_count: pkg_count,
                active: std::env::var("VIRTUAL_ENV")
                    .map(|v| v.eq_ignore_ascii_case(&env_dir.to_string_lossy()))
                    .unwrap_or(false),
                detail: home,
            },
        );
    }
}

fn site_packages_count(env_dir: &Path) -> Option<usize> {
    let candidates = [
        env_dir.join("Lib").join("site-packages"),
        env_dir.join("lib").join("site-packages"),
    ];
    for dir in candidates.iter().filter(|d| d.is_dir()) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let count = entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".dist-info")
                })
                .count();
            return Some(count);
        }
    }
    None
}