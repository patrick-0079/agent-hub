//! Python 环境的创建与删除（T3 能力 `py.env.create` / `py.env.remove`）。
//!
//! - 创建走 `uv venv`（uv 不在 PATH 时给出明确指引；conda 创建慢且参数形态
//!   各异，暂不启用，界面会明确说明）
//! - 创建后读 `pyvenv.cfg` 提取解释器版本，并写入 python_env 表标记为受管
//!   （放在扫描根目录之外也能在界面看到）
//! - 删除与所有破坏性操作一致：**整个目录移入回收站**（可恢复），同时清掉
//!   受管记录；绝不直接 rm

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCreatePlan {
    pub tier: String,
    pub tier_code: String,
    pub command: String,
    pub args: Vec<String>,
    pub target: String,
    pub target_exists: bool,
    pub python_note: String,
    pub uv_found: bool,
    pub message: String,
}

const CREATE_TIMEOUT: Duration = Duration::from_secs(180);

/// 解析 uv 可执行文件（PATH → ~/.local/bin → ~/.cargo/bin，与工具链扫描同序）
pub fn uv_path() -> Option<PathBuf> {
    // 与工具链扫描同一兜底顺序：PATH → ~/.local/bin → ~/.cargo/bin
    let extra = [
        crate::util::home_dir().join(".local").join("bin"),
        crate::util::home_dir().join(".cargo").join("bin"),
    ];
    crate::util::resolve_program("uv", &extra)
}

/// 计划：展示将执行的命令与目标目录状态（T3 需要用户确认）
pub fn plan_env_create(path: &str, python: Option<&str>) -> EnvCreatePlan {
    let target = crate::util::expand_path(path);
    let uv = uv_path();
    let mut args = vec!["venv".to_string(), target.clone()];
    if let Some(v) = python.map(str::trim).filter(|v| !v.is_empty()) {
        args.push("--python".to_string());
        args.push(v.to_string());
    }
    let python_note = match python.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!("指定 Python {}", v),
        None => "使用 uv 默认解析的 Python（首次可能需要下载解释器）".to_string(),
    };
    EnvCreatePlan {
        tier: "变更".into(),
        tier_code: "T3".into(),
        command: uv
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "uv".into()),
        args,
        target: target.clone(),
        target_exists: Path::new(&target).exists(),
        python_note,
        uv_found: uv.is_some(),
        message: if uv.is_none() {
            "未找到 uv：请先安装（https://docs.astral.sh/uv/），或到「设置 → 工具链」手动指定路径".into()
        } else if Path::new(&target).exists() {
            "目标目录已存在 —— 创建会失败，请换一个位置".into()
        } else {
            "将执行上面的命令创建虚拟环境（写入磁盘，可整体删除恢复）".into()
        },
    }
}

/// 执行创建：spawn `uv venv`，成功后读 pyvenv.cfg 并落库为受管环境
pub fn apply_env_create(
    store: &Store,
    path: &str,
    python: Option<&str>,
) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "创建 Python 环境（uv venv）".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let plan = plan_env_create(path, python);
    if !plan.uv_found {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: "uv".into(),
            ok: false,
            message: plan.message,
        });
        return result;
    }
    if plan.target_exists {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: plan.target.clone(),
            ok: false,
            message: "目标目录已存在，已中止".into(),
        });
        return result;
    }

    let uv = uv_path().expect("plan 已确认 uv 存在");
    let mut args: Vec<String> = vec!["venv".into(), plan.target.clone()];
    if let Some(v) = python.map(str::trim).filter(|v| !v.is_empty()) {
        args.push("--python".into());
        args.push(v.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = crate::util::run_capture(&uv, &arg_refs, CREATE_TIMEOUT);

    match output {
        Ok(text) => {
            let target = PathBuf::from(&plan.target);
            let cfg = target.join("pyvenv.cfg");
            if !cfg.is_file() {
                result.ok = false;
                result.steps.push(crate::actions::StepResult {
                    target: plan.target.clone(),
                    ok: false,
                    message: format!("命令退出但未生成 pyvenv.cfg —— 输出：{}", truncate(&text, 400)),
                });
                return result;
            }
            let version = read_pyvenv_version(&cfg);
            let name = target
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "uv-env".into());
            match store.python_env_upsert(&name, "uv", &plan.target, version.as_deref()) {
                Ok(_) => {
                    result.steps.push(crate::actions::StepResult {
                        target: plan.target.clone(),
                        ok: true,
                        message: format!(
                            "创建成功{}",
                            version
                                .as_deref()
                                .map(|v| format!("（Python {}）", v))
                                .unwrap_or_default()
                        ),
                    });
                    result.summary = format!(
                        "{} 已创建{}，已标记为受管环境",
                        plan.target,
                        version
                            .as_deref()
                            .map(|v| format!("（Python {}）", v))
                            .unwrap_or_default()
                    );
                    result.restore_hint =
                        "删除时整个目录会移入回收站，可随时恢复".into();
                }
                Err(e) => {
                    result.warnings.push(format!("受管记录落库失败：{}", e));
                    result.summary = format!("{} 已创建（受管记录落库失败：{}）", plan.target, e);
                }
            }
        }
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: plan.target.clone(),
                ok: false,
                message: e,
            });
        }
    }
    result
}

fn truncate(text: &str, max_chars: usize) -> String {
    let t = text.trim().replace(['\r', '\n'], " ");
    if t.chars().count() > max_chars {
        t.chars().take(max_chars).collect::<String>() + "…"
    } else {
        t
    }
}

fn read_pyvenv_version(cfg: &Path) -> Option<String> {
    let text = std::fs::read_to_string(cfg).ok()?;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            // 标准 venv 写 version；uv 写 version_info（如 3.13）
            let key = k.trim().to_ascii_lowercase();
            if key == "version" || key == "version_info" {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/* ---------------------------------------------------------------- npm 包 */

const NPM_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NpmInstallPlan {
    pub tier: String,
    pub tier_code: String,
    /// npm | pnpm
    pub manager: String,
    pub manager_path: String,
    pub found: bool,
    pub args: Vec<String>,
    pub packages: Vec<String>,
    pub message: String,
}

/// 解析包管理器可执行文件（与工具链扫描同一兜底目录）
pub fn resolve_package_manager(manager: &str) -> Option<PathBuf> {
    let extra: Vec<PathBuf> = match manager {
        "pnpm" => vec![crate::util::expand_buf("%APPDATA%/pnpm")],
        _ => vec![
            crate::util::expand_buf("%APPDATA%/npm"),
            crate::util::expand_buf("%PROGRAMDATA%/npm/npm"),
        ],
    };
    crate::util::resolve_program(manager, &extra)
}

fn npm_subcommand(manager: &str, action: &str) -> &'static str {
    match (manager, action) {
        ("pnpm", "install") => "add",
        ("pnpm", "remove") => "remove",
        (_, "install") => "install",
        _ => "uninstall",
    }
}

/// 安装计划（T3 先看将执行的命令）
pub fn plan_npm_install(manager: &str, packages: &[String]) -> NpmInstallPlan {
    let exe = resolve_package_manager(manager);
    let args: Vec<String> = [
        npm_subcommand(manager, "install").to_string(),
        "-g".to_string(),
    ]
    .into_iter()
    .chain(packages.iter().cloned())
    .collect();
    NpmInstallPlan {
        tier: "变更".into(),
        tier_code: "T3".into(),
        manager: manager.to_string(),
        manager_path: exe
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        found: exe.is_some(),
        args: args.clone(),
        packages: packages.to_vec(),
        message: if exe.is_none() {
            format!("未找到 {}：请先安装，或到「设置 → 工具链」确认", manager)
        } else {
            "将全局安装上面的包（真实执行包管理器，耗时取决于网络）".into()
        },
    }
}

/// 卸载计划（T3）
pub fn plan_npm_remove(manager: &str, package: &str) -> NpmInstallPlan {
    let exe = resolve_package_manager(manager);
    let args = vec![
        npm_subcommand(manager, "remove").to_string(),
        "-g".to_string(),
        package.to_string(),
    ];
    NpmInstallPlan {
        tier: "变更".into(),
        tier_code: "T3".into(),
        manager: manager.to_string(),
        manager_path: exe
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        found: exe.is_some(),
        args,
        packages: vec![package.to_string()],
        message: if exe.is_none() {
            format!("未找到 {}", manager)
        } else {
            "将从全局移除该包（包管理器 uninstall，可随时重新安装）".into()
        },
    }
}

/// 从 `ls -g --depth=0` 输出解析某个包的已装版本（形如 `├── pkg@1.2.3`）
pub fn parse_global_pkg_version(output: &str, name: &str) -> Option<String> {
    let token = format!("{}@", name);
    for line in output.lines() {
        if let Some(pos) = line.find(&token) {
            let rest = &line[pos + token.len()..];
            let version: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            if !version.is_empty() {
                return Some(version);
            }
        }
    }
    None
}

/// 执行全局安装：spawn 包管理器 → `ls -g` 解析版本 → 落库 npm_package。
/// `exe` 由调用方解析（命令层走 PATH 兜底，自检传 mock 脚本）。
pub fn apply_npm_install(
    store: &Store,
    exe: &Path,
    manager: &str,
    packages: &[String],
) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: format!("全局安装（{}）", manager),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let clean: Vec<String> = packages
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if clean.is_empty() {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: "(input)".into(),
            ok: false,
            message: "没有有效的包名".into(),
        });
        return result;
    }
    let mut args: Vec<String> = vec![npm_subcommand(manager, "install").into(), "-g".into()];
    args.extend(clean.clone());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let install_out = crate::util::run_capture(exe, &arg_refs, NPM_TIMEOUT);
    let install_text = match install_out {
        Ok(t) => t,
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: format!("{} install", manager),
                ok: false,
                message: e,
            });
            return result;
        }
    };

    // 读回版本
    let list_out = crate::util::run_capture(
        exe,
        &["ls", "-g", "--depth=0"],
        Duration::from_secs(60),
    )
    .unwrap_or_default();

    let mut installed = 0usize;
    for pkg in &clean {
        let version = parse_global_pkg_version(&list_out, pkg);
        match store.npm_package_upsert(pkg, version.as_deref(), manager, "global") {
            Ok(_) => {
                installed += 1;
                result.steps.push(crate::actions::StepResult {
                    target: pkg.clone(),
                    ok: true,
                    message: version
                        .map(|v| format!("已安装（{}）", v))
                        .unwrap_or_else(|| "已安装（版本未能解析，重新扫描后可见）".into()),
                });
            }
            Err(e) => {
                result.warnings.push(format!("{} 记录落库失败：{}", pkg, e));
            }
        }
    }
    result.summary = format!(
        "{} 个包已全局安装（{}）；输出：{}",
        installed,
        manager,
        truncate(&install_text, 200)
    );
    result.restore_hint = "卸载走包管理器 uninstall，可随时重装".into();
    result
}

/// 执行全局卸载并清除受管记录
pub fn apply_npm_remove(
    store: &Store,
    exe: &Path,
    manager: &str,
    package: &str,
) -> crate::actions::ActionResult {    let mut result = crate::actions::ActionResult {
        ok: true,
        title: format!("全局卸载（{}）", manager),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let pkg = package.trim();
    if pkg.is_empty() {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: "(input)".into(),
            ok: false,
            message: "包名为空".into(),
        });
        return result;
    }
    let output = crate::util::run_capture(
        exe,
        &[npm_subcommand(manager, "remove"), "-g", pkg],
        NPM_TIMEOUT,
    );
    match output {
        Ok(text) => {
            let _ = store.npm_package_delete(pkg, manager);
            result.steps.push(crate::actions::StepResult {
                target: pkg.to_string(),
                ok: true,
                message: "已卸载，受管记录已清除".into(),
            });
            result.summary = format!("{} 已卸载（{}）；输出：{}", pkg, manager, truncate(&text, 200));
            result.restore_hint = "需要时重新安装即可".into();
        }
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: pkg.to_string(),
                ok: false,
                message: e,
            });
        }
    }
    result
}

/* -------------------------------------------------------- 检查更新 */

/// 一个可更新的全局包（npm outdated 的解析结果）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NpmOutdated {
    pub name: String,
    pub current: String,
    pub wanted: String,
    pub latest: String,
}

/// 解析 `npm outdated -g --json` 的输出：
/// `{"pkg":{"current":"1.0.0","wanted":"1.1.0","latest":"2.0.0",...}}`，
/// 全部最新时输出 `{}`（注意此时 npm 退出码为 0，有更新时为 1 但 stdout 仍有 JSON）。
pub fn parse_npm_outdated(output: &str) -> Vec<NpmOutdated> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Vec::new(), // pnpm 等可能输出表格而非 JSON —— 无更新可报告
    };
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<NpmOutdated> = obj
        .iter()
        .filter(|(_, info)| info.is_object())
        .map(|(name, info)| {
            let get = |k: &str| {
                info.get(k)
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            NpmOutdated {
                name: name.clone(),
                current: get("current"),
                wanted: get("wanted"),
                latest: get("latest"),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 查询全局包可更新清单（npm；pnpm 视其版本对 --json 的支持，无法解析时返回空）
pub fn npm_outdated_list(exe: &Path, _manager: &str) -> Result<Vec<NpmOutdated>, String> {
    let output = crate::util::run_capture(
        exe,
        &["outdated", "-g", "--json"],
        Duration::from_secs(90),
    )?;
    Ok(parse_npm_outdated(&output))
}

/* ---------------------------------------------------------------- 删除 */

/// 删除受管环境：目录整体移入回收站（可恢复），并清掉受管记录
pub fn apply_env_remove(store: &Store, path: &str) -> crate::actions::ActionResult {
    let mut result = crate::actions::ActionResult {
        ok: true,
        title: "删除 Python 环境".into(),
        summary: String::new(),
        steps: Vec::new(),
        manifest: None,
        restore_hint: String::new(),
        warnings: Vec::new(),
    };
    let target = crate::util::expand_path(path);
    let dir = PathBuf::from(&target);
    if !dir.is_dir() {
        result.ok = false;
        result.steps.push(crate::actions::StepResult {
            target: target.clone(),
            ok: false,
            message: "目录不存在（可能已删除）".into(),
        });
        let _ = store.python_env_delete(&target);
        return result;
    }
    // 与所有删除一致：移入回收站而不是直接删
    match crate::actions::move_path_to_trash(
        &dir,
        "py-env",
        &format!(
            "python-env:{}",
            dir.file_name().map(|s| s.to_string_lossy()).unwrap_or_default()
        ),
    ) {
        Ok((entry_name, manifest, items)) => {
            let _ = store.python_env_delete(&target);
            result.steps.push(crate::actions::StepResult {
                target: target.clone(),
                ok: true,
                message: format!(
                    "已移入回收站条目「{}」（{} 个对象），可恢复",
                    entry_name,
                    items.len()
                ),
            });
            result.summary = format!(
                "{} 已移入回收站（{} 个对象），受管记录已清除",
                target,
                items.len()
            );
            result.restore_hint = "在「回收站」页可整条恢复到原位".into();
            result.manifest = Some(manifest);
        }
        Err(e) => {
            result.ok = false;
            result.steps.push(crate::actions::StepResult {
                target: target.clone(),
                ok: false,
                message: format!("移入回收站失败，已中止（目录保持原样）：{}", e),
            });
        }
    }
    result
}
