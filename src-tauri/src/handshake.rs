//! MCP 握手健康检查：T3 能力 `proc.spawn.probe` 的实现。
//!
//! 真正把 MCP 服务器跑起来完成一次 JSON-RPC `initialize` 握手：
//! - stdio：spawn 进程 → 写 initialize → 读响应 → 再问一次 tools/list → 关进程
//! - http/sse：POST initialize（Streamable HTTP），响应可能是 JSON 也可能是 SSE 帧
//!
//! 环境变量值支持 `%VAR%` / `$VAR` 引用（界面里不落明文，握手前瞬间展开）。
//! 只读探测：不写入任何配置，进程结束即恢复原状。

use crate::model::McpResource;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

const INIT_TIMEOUT: Duration = Duration::from_secs(15);
const TOOLS_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpHandshakeResult {
    pub server_id: i64,
    pub name: String,
    /// stdio | http | sse
    pub transport: String,
    /// ok | timeout | error
    pub status: String,
    pub latency_ms: u64,
    /// initialize 响应里的 protocolVersion
    pub protocol_version: Option<String>,
    /// initialize 响应里的 serverInfo.name
    pub server_name: Option<String>,
    /// tools/list 返回的工具数（服务器不支持或未响应时为 None）
    pub tools: Option<usize>,
    pub message: String,
    pub tested_at: String,
}

fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn failed(res: &McpResource, message: impl Into<String>) -> McpHandshakeResult {
    McpHandshakeResult {
        server_id: res.id,
        name: res.name.clone(),
        transport: res.transport.clone(),
        status: "error".into(),
        message: message.into(),
        tested_at: now_human(),
        ..Default::default()
    }
}

fn init_request() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agenthub", "version": env!("CARGO_PKG_VERSION") }
        }
    })
}

fn tools_request() -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })
}

/// 入口：按传输方式分发
pub fn handshake(res: &McpResource, proxy: &str) -> McpHandshakeResult {
    match res.transport.as_str() {
        "http" | "sse" => handshake_http(res, proxy),
        _ => handshake_stdio(res),
    }
}

/* ---------------------------------------------------------------- stdio */

fn handshake_stdio(res: &McpResource) -> McpHandshakeResult {
    let started = Instant::now();
    if res.command.trim().is_empty() {
        return failed(res, "缺少启动命令（stdio 传输需要 command）");
    }
    let program = match crate::util::resolve_program(res.command.trim(), &[]) {
        Some(p) => p,
        None => {
            return failed(
                res,
                format!(
                    "找不到命令「{}」（PATH 与 PATHEXT 均未命中；npx 型服务器依赖 Node）",
                    res.command.trim()
                ),
            )
        }
    };

    let mut cmd = Command::new(&program);
    cmd.args(&res.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for pair in &res.env {
        if !pair.key.trim().is_empty() {
            let value = crate::util::expand_value(&pair.value);
            cmd.env(pair.key.trim(), value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return failed(res, format!("无法启动 {}: {}", program.display(), e)),
    };

    let result = drive_stdio(&mut child, started.elapsed().as_millis() as u64);
    // 无论成败都收尾：杀进程并收割，不留孤儿
    let _ = child.kill();
    let _ = child.wait();
    let mut result = result;
    result.server_id = res.id;
    result.name = res.name.clone();
    result.transport = res.transport.clone();
    result.latency_ms = started.elapsed().as_millis() as u64;
    result.tested_at = now_human();
    result
}

/// 与已启动的子进程完成握手（stdin/stdout JSON-RPC 行协议）
fn drive_stdio(child: &mut Child, _prewait_ms: u64) -> McpHandshakeResult {
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take();

    // 读线程：逐行转发到 channel，避免主线程阻塞读
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(out) = stdout {
        std::thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // 1) initialize
    if let Some(pipe) = stdin.as_mut() {
        let _ = writeln!(pipe, "{}", init_request());
        let _ = pipe.flush();
    } else {
        return McpHandshakeResult {
            status: "error".into(),
            message: "无法写入子进程 stdin".into(),
            ..Default::default()
        };
    }

    let mut init_result: Option<serde_json::Value> = None;
    let deadline = Instant::now() + INIT_TIMEOUT;
    while init_result.is_none() {
        let now = Instant::now();
        if now >= deadline {
            return McpHandshakeResult {
                status: "timeout".into(),
                message: format!(
                    "initialize 握手超时（{} 秒）：进程启动了，但没有在 stdout 上返回 JSON-RPC 响应",
                    INIT_TIMEOUT.as_secs()
                ),
                ..Default::default()
            };
        }
        match rx.recv_timeout(deadline - now) {
            Ok(line) => {
                if let Some(v) = parse_rpc_line(&line) {
                    if rpc_id(&v) == Some(1) {
                        if let Some(err) = v.get("error") {
                            return McpHandshakeResult {
                                status: "error".into(),
                                message: format!(
                                    "服务器拒绝了 initialize：{}",
                                    err.get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("未知错误")
                                ),
                                ..Default::default()
                            };
                        }
                        init_result = v.get("result").cloned();
                        if init_result.is_none() {
                            // id=1 但既无 result 也无 error 的畸形响应
                            return McpHandshakeResult {
                                status: "error".into(),
                                message: "initialize 响应缺少 result 字段".into(),
                                ..Default::default()
                            };
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                return McpHandshakeResult {
                    status: "timeout".into(),
                    message: format!(
                        "initialize 握手超时（{} 秒）",
                        INIT_TIMEOUT.as_secs()
                    ),
                    ..Default::default()
                };
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let Some(init) = init_result else {
        return McpHandshakeResult {
            status: "error".into(),
            message: "进程提前退出（stdout 关闭），没有完成 initialize".into(),
            ..Default::default()
        };
    };

    let protocol_version = init
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let server_name = init
        .get("serverInfo")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // 2) notifications/initialized + tools/list（尽力而为：失败不影响握手结论）
    let mut tools: Option<usize> = None;
    if let Some(pipe) = stdin.as_mut() {
        let _ = writeln!(
            pipe,
            "{}",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        );
        let _ = writeln!(pipe, "{}", tools_request());
        let _ = pipe.flush();
    }
    let tools_deadline = Instant::now() + TOOLS_TIMEOUT;
    loop {
        let now = Instant::now();
        if now >= tools_deadline {
            break;
        }
        // 非阻塞收尾：把缓冲里剩下的行消化掉
        match rx.try_recv() {
            Ok(line) => {
                if let Some(v) = parse_rpc_line(&line) {
                    if rpc_id(&v) == Some(2) {
                        if let Some(list) = v.get("result").and_then(|r| r.get("tools")) {
                            if let Some(arr) = list.as_array() {
                                tools = Some(arr.len());
                            }
                        }
                        break;
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(TryRecvError::Disconnected) => break,
        }
    }

    let mut message = format!(
        "握手成功（协议 {}）",
        protocol_version.as_deref().unwrap_or("未知版本")
    );
    if let Some(name) = server_name.as_deref() {
        message.push_str(&format!(" · {}", name));
    }
    if let Some(n) = tools {
        message.push_str(&format!(" · {} 个工具", n));
    }

    McpHandshakeResult {
        status: "ok".into(),
        latency_ms: 0,
        protocol_version,
        server_name,
        tools,
        message,
        ..Default::default()
    }
}

fn parse_rpc_line(line: &str) -> Option<serde_json::Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None; // 进程横幅/日志行，忽略
    }
    serde_json::from_str(trimmed).ok()
}

fn rpc_id(v: &serde_json::Value) -> Option<i64> {
    v.get("id").and_then(|i| i.as_i64())
}

/* ---------------------------------------------------------------- http */

fn handshake_http(res: &McpResource, proxy: &str) -> McpHandshakeResult {
    let started = Instant::now();
    if res.url.trim().is_empty() {
        return failed(res, "缺少 URL（http/sse 传输需要 url）");
    }
    let agent = match crate::probe::network_agent(proxy) {
        Ok(a) => a,
        Err(e) => return failed(res, e),
    };
    let url = res.url.trim().to_string();
    let body = init_request().to_string();

    let response = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream")
        .send_string(&body);

    let latency_ms = started.elapsed().as_millis() as u64;
    match response {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp
                .header("Content-Type")
                .unwrap_or("")
                .to_string();
            let mut body = String::new();
            let _ = resp
                .into_reader()
                .take(256 * 1024)
                .read_to_string(&mut body);
            let mut result = classify_http(res, status, &content_type, &body);
            result.latency_ms = latency_ms;
            result.tested_at = now_human();
            result
        }
        Err(ureq::Error::Status(code, resp)) => {
            let mut body = String::new();
            let _ = resp
                .into_reader()
                .take(8 * 1024)
                .read_to_string(&mut body);
            let mut result = failed(
                res,
                format!(
                    "HTTP {}：{}",
                    code,
                    body.trim().replace(['\n', '\r'], " ").chars().take(160).collect::<String>()
                ),
            );
            result.latency_ms = latency_ms;
            result
        }
        Err(ureq::Error::Transport(t)) => {
            let mut result = failed(res, format!("网络错误：{}", t.message().unwrap_or("未知")));
            result.latency_ms = latency_ms;
            result
        }
    }
}

fn classify_http(
    res: &McpResource,
    status: u16,
    content_type: &str,
    body: &str,
) -> McpHandshakeResult {
    // 从 JSON 或 SSE 帧里提取 JSON-RPC 响应
    let parsed: Option<serde_json::Value> = if content_type.contains("text/event-stream") {
        body.lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .find_map(|data| serde_json::from_str(data).ok())
    } else {
        serde_json::from_str(body).ok()
    };

    let mut result = McpHandshakeResult {
        server_id: res.id,
        name: res.name.clone(),
        transport: res.transport.clone(),
        ..Default::default()
    };

    if !(200..300).contains(&status) {
        result.status = "error".into();
        result.message = format!("HTTP {}（握手端点应为 MCP Streamable HTTP 入口）", status);
        return result;
    }

    let Some(v) = parsed else {
        result.status = "error".into();
        result.message =
            "端点可达，但响应不是可识别的 JSON-RPC（URL 可能不是 MCP 入口）".into();
        return result;
    };

    // Streamable HTTP 在 initialize 阶段可能直接返回 result（无 id 包裹）或完整响应
    let payload = v.get("result").cloned().unwrap_or(v.clone());
    let protocol_version = payload
        .get("protocolVersion")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let server_name = payload
        .get("serverInfo")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(str::to_string);
    if protocol_version.is_none() && server_name.is_none() {
        result.status = "error".into();
        result.message = "响应 JSON 里没有 initialize 结果字段（protocolVersion / serverInfo 缺失）".into();
        return result;
    }
    result.status = "ok".into();
    result.protocol_version = protocol_version;
    result.server_name = server_name.clone();
    result.message = format!(
        "握手成功（HTTP {} · 协议 {}）{}",
        status,
        result.protocol_version.as_deref().unwrap_or("未知"),
        server_name.map(|n| format!(" · {}", n)).unwrap_or_default()
    );
    result
}
