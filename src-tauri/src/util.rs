//! 通用工具：路径展开、进程探测与执行、文件统计、主机信息。

use crate::model::{ExecutableInfo, HostInfo};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Windows: 隐藏子进程控制台窗口，避免探测时闪黑框。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

/* --------------------------------------------------------------- 路径展开 */

fn expand_windows_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '%' {
            if let Some(end) = bytes[i + 1..].iter().position(|c| *c == '%') {
                let name: String = bytes[i + 1..i + 1 + end].iter().collect();
                if !name.is_empty() && !name.contains(' ') {
                    if let Ok(v) = std::env::var(&name) {
                        out.push_str(&v);
                        i = i + end + 2;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn expand_unix_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            let braced = chars[i + 1] == '{';
            let start = if braced { i + 2 } else { i + 1 };
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
            {
                end += 1;
            }
            if end > start {
                let name: String = chars[start..end].iter().collect();
                let consumed_end = if braced && end < chars.len() && chars[end] == '}' {
                    end + 1
                } else {
                    end
                };
                if let Ok(v) = std::env::var(&name) {
                    out.push_str(&v);
                    i = consumed_end;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 展开 `~`、`%APPDATA%`、`$HOME` 等占位符，并统一为本地路径分隔符。
pub fn expand_path(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s == "~" {
        s = home_dir().to_string_lossy().to_string();
    } else if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        s = format!("{}/{}", home_dir().to_string_lossy(), rest);
    }
    s = expand_windows_vars(&s);
    s = expand_unix_vars(&s);
    if cfg!(windows) {
        s = s.replace('/', "\\");
    }
    s
}

/// 展开普通值里的 `%VAR%` / `$VAR` / `${VAR}` 引用（不做路径分隔符转换）。
/// MCP 环境变量值就是这种形态：界面里存引用，实际传给进程前在这里展开。
pub fn expand_value(raw: &str) -> String {
    let s = raw.trim().to_string();
    let s = expand_windows_vars(&s);
    expand_unix_vars(&s)
}

pub fn expand_buf(raw: &str) -> PathBuf {
    PathBuf::from(expand_path(raw))
}

/* ------------------------------------------------------------ 进程与版本 */

/// 按 PATHEXT 在 PATH + 额外目录中解析可执行文件。
pub fn resolve_program(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|x| !x.trim().is_empty())
            .map(|x| x.trim().to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };

    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    dirs.extend(extra_dirs.iter().cloned());

    for dir in &dirs {
        for ext in &exts {
            let cand = dir.join(format!("{}{}", name, ext));
            if cand.is_file() {
                return Some(cand);
            }
        }
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// 执行命令并捕获输出，带超时保护（超时则终止子进程）。
pub fn run_capture(program: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法启动 {}: {}", program.display(), e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let h_out = stdout.map(|mut s| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            b
        })
    });
    let h_err = stderr.map(|mut s| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            b
        })
    });

    let deadline = Instant::now() + timeout;
    let mut ok = false;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                ok = status.success();
                break;
            }
            Ok(None) => {}
            Err(_) => break,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let out = h_out.and_then(|h| h.join().ok()).unwrap_or_default();
    let err = h_err.and_then(|h| h.join().ok()).unwrap_or_default();
    let out_s = String::from_utf8_lossy(&out).to_string();
    let err_s = String::from_utf8_lossy(&err).to_string();

    if timed_out {
        return Err(format!("命令超时（{}s）", timeout.as_secs()));
    }
    if ok || !out_s.trim().is_empty() {
        Ok(out_s)
    } else {
        Err(err_s.trim().to_string())
    }
}

/// 从 `--version` 之类的输出中提取语义化版本号。
pub fn extract_version(output: &str) -> Option<String> {
    for line in output.lines().take(6) {
        for tok in line.split_whitespace() {
            let t = tok.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+' && c != '_'
            });
            let t = t.strip_prefix('v').unwrap_or(t);
            let digit_run = t.chars().take_while(|c| c.is_ascii_digit()).count();
            if digit_run > 0 && t.contains('.') && t.len() <= 32 {
                let cleaned: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '+')
                    .collect();
                if cleaned.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    let s = s.trim_end_matches(['\\', '/']).to_string();
    if cfg!(windows) {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

/// 探测一个可执行文件：解析位置（用户覆盖 > PATH > 已知目录）并读取版本。
pub fn probe_executable(
    name: &str,
    display: &str,
    category: &str,
    version_args: &[&str],
    extra_dirs: &[&str],
    used_by: &[&str],
    hint: &str,
    overrides: &HashMap<String, String>,
) -> ExecutableInfo {
    let mut info = ExecutableInfo {
        name: name.to_string(),
        display_name: display.to_string(),
        category: category.to_string(),
        used_by: used_by.iter().map(|s| s.to_string()).collect(),
        hint: (!hint.is_empty()).then(|| hint.to_string()),
        ..Default::default()
    };

    let override_path = overrides
        .get(name)
        .map(|p| expand_buf(p))
        .filter(|p| p.is_file());

    let (path, source) = if let Some(p) = override_path {
        (p, "user-override".to_string())
    } else {
        let extra: Vec<PathBuf> = extra_dirs.iter().map(|d| expand_buf(d)).collect();
        let path_dirs: Vec<PathBuf> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        match resolve_program(name, &extra) {
            Some(p) => {
                let parent = p.parent().map(norm).unwrap_or_default();
                let on_path = path_dirs.iter().any(|d| norm(d) == parent);
                let src = if on_path { "PATH" } else { "known-location" };
                (p, src.to_string())
            }
            None => {
                info.found = false;
                return info;
            }
        }
    };

    info.found = true;
    info.source = Some(source);
    // 空 version_args 表示该程序不探测版本（例如不带参数会进入交互式 TUI）
    if !version_args.is_empty() {
        if let Ok(out) = run_capture(&path, version_args, Duration::from_secs(8)) {
            info.version = extract_version(&out);
        }
    }
    info.path = Some(path.to_string_lossy().to_string());
    info
}

/* --------------------------------------------------------------- 文件统计 */

/// 统计目录内文件数与总字节数（带条目上限，避免超大目录卡死）。
///
/// **不递归进入符号链接 / junction**：否则回收站里的链接会按目标内容虚报体积，
/// 同一份内容也会被多个链接重复计数。
pub fn dir_stats(path: &Path, limit: usize) -> (usize, u64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![(path.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 4 || count >= limit {
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                // 跳过链接：不跟随、也不计入
                let is_link = std::fs::symlink_metadata(&p)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                if !is_link {
                    stack.push((p, depth + 1));
                }
            } else {
                count += 1;
                bytes += meta.len();
                if count >= limit {
                    break;
                }
            }
        }
    }
    (count, bytes)
}

pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "AppData",
    "Application Data",
    "Library",
    "Windows",
    "ProgramData",
    "$Recycle.Bin",
    "System Volume Information",
    ".cache",
    ".gradle",
    ".m2",
    ".nuget",
    "site-packages",
    "__pycache__",
    ".vscode",
    "OneDrive",
];

/// 在 root 下递归查找指定文件名，受深度与访问目录数限制。
pub fn find_files(
    root: &Path,
    target_name: &str,
    max_depth: usize,
    max_visits: usize,
    skip: &[&str],
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    let mut visits = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth || visits > max_visits {
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        visits += 1;
        for entry in entries.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if skip.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                // 跳过隐藏目录，但保留常见的 Agent / 虚拟环境目录
                if name.starts_with('.')
                    && !matches!(
                        name.as_str(),
                        ".venv" | "venv" | ".dsh" | ".claude" | ".codex" | ".config" | ".cursor"
                    )
                {
                    continue;
                }
                stack.push((p, depth + 1));
            } else if name.eq_ignore_ascii_case(target_name) {
                found.push(p);
            }
        }
    }
    found
}

pub fn file_mtime_iso(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Some(dt.format("%Y-%m-%d %H:%M").to_string())
}

pub fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// 人类可读的字节数（与前端 formatBytes 保持一致的口径）
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut idx = 0usize;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, UNITS[idx])
    } else {
        format!("{:.1} {}", value, UNITS[idx])
    }
}

/* ----------------------------------------------------------------- 脱敏 */

pub fn mask_secret(value: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        return "（空）".to_string();
    }
    let chars: Vec<char> = v.chars().collect();
    if chars.len() <= 10 {
        return "•".repeat(chars.len().min(8));
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{}••••{} （{} 字符）", head, tail, chars.len())
}

/* ------------------------------------------------------------ glob 匹配 */

/// 单段内的通配匹配：`*` 任意字符序列（不跨分隔符）、`?` 单个字符。
/// 经典双指针回溯实现，无需正则引擎。
fn segment_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = text.chars().collect();
    let (mut si, mut pi) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            si += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = si;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// 按 `/` 分段的 glob 匹配：`**` 匹配任意层（含零层）。
pub fn glob_match(pattern: &str, text: &str) -> bool {
    fn match_segments(p: &[&str], t: &[&str]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        if p[0] == "**" {
            for skip in 0..=t.len() {
                if match_segments(&p[1..], &t[skip..]) {
                    return true;
                }
            }
            return false;
        }
        if t.is_empty() {
            return false;
        }
        if !segment_match(p[0], t[0]) {
            return false;
        }
        match_segments(&p[1..], &t[1..])
    }
    let p: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = text.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&p, &t)
}

/// 收集 `root` 下与 `pattern` 匹配的相对路径（`/` 分隔）。
/// 深度与条目上限保护，防止巨型目录树把扫描卡死。
pub fn glob_collect(
    root: &Path,
    pattern: &str,
    out: &mut Vec<String>,
    max_entries: usize,
    max_depth: usize,
) {
    fn walk(
        dir: &Path,
        prefix: &str,
        pattern: &str,
        out: &mut Vec<String>,
        max_entries: usize,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth || out.len() >= max_entries {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= max_entries {
                return;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if glob_match(pattern, &rel) {
                out.push(rel.clone());
            }
            // 目录名本身也可能匹配（如 `**/skills`）；继续下钻收集子路径
            if is_dir {
                walk(&entry.path(), &rel, pattern, out, max_entries, depth + 1, max_depth);
            }
        }
    }
    walk(root, "", pattern, out, max_entries, 0, max_depth);
}

/* ------------------------------------------------------------- 主机信息 */

pub fn host_info(app_version: &str, data_dir: &str, db_path: &str) -> HostInfo {
    let os_version = if cfg!(windows) {
        std::env::var("OS").unwrap_or_else(|_| "Windows".to_string())
    } else {
        String::new()
    };
    HostInfo {
        os: std::env::consts::OS.to_string(),
        os_version,
        arch: std::env::consts::ARCH.to_string(),
        hostname: std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".to_string()),
        username: std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "unknown".to_string()),
        home_dir: home_dir().to_string_lossy().to_string(),
        app_version: app_version.to_string(),
        data_dir: data_dir.to_string(),
        db_path: db_path.to_string(),
    }
}