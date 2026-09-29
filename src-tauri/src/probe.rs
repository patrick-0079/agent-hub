//! 供应商连通性探测与余额查询：T3 能力 `net.provider.probe` / `net.provider.balance` 的实现。
//!
//! 只发起最小只读请求（GET /models、GET /user/balance 等）来验证 Base URL / Key / 余额：
//! - Key 明文只在内存中存活于这一次请求，不写日志、不落库；
//! - 响应体若意外回显 Key，进入 message 前先打码；
//! - 尊重设置里的网络代理。
//!
//! 结果分级：
//! - `ok`          —— 端点可用（可附带模型数量 / 余额）
//! - `no_key`      —— 端点可达但需要认证，而本地还没有保存 Key（HTTP 401/403）
//! - `error`       —— 连接失败 / 超时 / TLS 失败 / Key 无效 / 端点不存在
//! - `unsupported` ——（仅余额查询）该供应商类型还没有余额端点实现

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::{Duration, Instant};

/// 一次探测的结果（落库到 provider.health 的就是它的 JSON）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub provider_id: i64,
    pub provider_name: String,
    /// ok | no_key | error
    pub status: String,
    pub http_status: Option<u16>,
    pub latency_ms: u64,
    /// 响应中解析到的模型数量（响应不是可识别结构时为 None）
    pub models: Option<usize>,
    pub message: String,
    pub tested_at: String,
    /// 实际请求的端点（含兜底重试后的最终端点）
    pub endpoint: String,
}

const OVERALL_TIMEOUT: Duration = Duration::from_secs(12);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn failed(message: impl Into<String>) -> ProviderTestResult {
    ProviderTestResult {
        status: "error".into(),
        message: message.into(),
        tested_at: now_human(),
        ..Default::default()
    }
}

/// 归一化 Base URL：补默认协议（ollama 本地服务默认 http，其余 https）
fn normalize_base(base: &str, kind: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.starts_with("http://") || b.starts_with("https://") {
        return b.to_string();
    }
    if kind.to_lowercase().contains("ollama") {
        format!("http://{}", b)
    } else {
        format!("https://{}", b)
    }
}

fn build_agent(proxy: &str) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(OVERALL_TIMEOUT);
    let p = proxy.trim();
    if !p.is_empty() {
        let proxy = ureq::Proxy::new(p).map_err(|e| format!("代理配置无效（{}）：{}", p, e))?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build())
}

/// 供其它网络探测（如 MCP HTTP 握手）复用的带超时/代理 Agent
pub fn network_agent(proxy: &str) -> Result<ureq::Agent, String> {
    build_agent(proxy)
}

/// 从响应体里数模型个数（OpenAI/Anthropic 的 data 数组，或 Ollama 的 models 数组）
fn count_models(body: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        return Some(arr.len());
    }
    if let Some(arr) = v.get("models").and_then(|x| x.as_array()) {
        return Some(arr.len());
    }
    if let Some(arr) = v.as_array() {
        return Some(arr.len());
    }
    None
}

/// 响应体片段进入 message 前的净化：打码 Key、去换行、截断
fn sanitize_snippet(body: &str, key: Option<&str>) -> String {
    let mut text = body.trim().replace(['\n', '\r'], " ");
    if let Some(k) = key.filter(|k| k.len() >= 8) {
        if text.contains(k) {
            text = text.replace(k, "***");
        }
    }
    if text.chars().count() > 140 {
        text.chars().take(140).collect::<String>() + "…"
    } else {
        text
    }
}

/// 发起一次连通性测试。`api_key` 为 None 或空串表示未保存 Key。
pub fn test_provider(
    base_url: &str,
    kind: &str,
    api_key: Option<&str>,
    proxy: &str,
) -> ProviderTestResult {
    if base_url.trim().is_empty() {
        return failed("缺少 Base URL");
    }
    let kind_l = kind.to_lowercase();
    let is_anthropic = kind_l.contains("anthropic");
    let is_ollama = kind_l.contains("ollama");
    let base = normalize_base(base_url, &kind_l);
    let key = api_key.map(str::trim).filter(|k| !k.is_empty());

    // 端点候选：主端点失败（404）时按约定补一次兜底
    let mut candidates: Vec<String> = Vec::new();
    if is_ollama {
        candidates.push(format!("{}/api/tags", base));
    } else if is_anthropic {
        if base.ends_with("/v1") {
            candidates.push(format!("{}/models", base));
        } else {
            candidates.push(format!("{}/v1/models", base));
        }
    } else {
        // OpenAI 兼容约定 base 含 /v1；不含时再试 /v1/models
        candidates.push(format!("{}/models", base));
        if !base.ends_with("/v1") && !base.ends_with("/api") {
            candidates.push(format!("{}/v1/models", base));
        }
    }

    let agent = match build_agent(proxy) {
        Ok(a) => a,
        Err(e) => return failed(e),
    };

    let mut last: Option<ProviderTestResult> = None;
    for endpoint in &candidates {
        let mut req = agent.get(endpoint);
        if is_anthropic {
            req = req.set("anthropic-version", "2023-06-01");
            if let Some(k) = key {
                req = req.set("x-api-key", k);
            }
        } else if let Some(k) = key {
            req = req.set("Authorization", &format!("Bearer {}", k));
        }

        let start = Instant::now();
        let response = req.call();
        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let http_status = resp.status();
                let mut body = String::new();
                let _ = resp
                    .into_reader()
                    .take(128 * 1024)
                    .read_to_string(&mut body);
                let result = classify_response(
                    http_status,
                    &body,
                    key,
                    latency_ms,
                    endpoint,
                    is_last_candidate(endpoint, &candidates),
                );
                // 404 且还有兜底端点 → 继续尝试
                if result.status == "retry" {
                    last = Some(ProviderTestResult {
                        status: "error".into(),
                        message: format!("HTTP 404：{}", sanitize_snippet(&body, key)),
                        tested_at: now_human(),
                        endpoint: endpoint.clone(),
                        ..Default::default()
                    });
                    continue;
                }
                let mut result = result;
                result.provider_id = 0;
                result.provider_name = String::new();
                result.tested_at = now_human();
                result.endpoint = endpoint.clone();
                return result;
            }
            Err(ureq::Error::Status(code, resp)) => {
                // ureq 对非 2xx 默认返回 Err(Status)；读取体后统一走分类
                let mut body = String::new();
                let _ = resp
                    .into_reader()
                    .take(128 * 1024)
                    .read_to_string(&mut body);
                let result = classify_response(
                    code,
                    &body,
                    key,
                    latency_ms,
                    endpoint,
                    is_last_candidate(endpoint, &candidates),
                );
                if result.status == "retry" {
                    last = Some(ProviderTestResult {
                        status: "error".into(),
                        message: format!("HTTP {}: {}", code, sanitize_snippet(&body, key)),
                        tested_at: now_human(),
                        endpoint: endpoint.clone(),
                        ..Default::default()
                    });
                    continue;
                }
                let mut result = result;
                result.tested_at = now_human();
                result.endpoint = endpoint.clone();
                return result;
            }
            Err(ureq::Error::Transport(t)) => {
                let message = describe_transport_error(&t);
                return ProviderTestResult {
                    status: "error".into(),
                    message,
                    tested_at: now_human(),
                    endpoint: endpoint.clone(),
                    latency_ms,
                    ..Default::default()
                };
            }
        }
    }
    last.unwrap_or_else(|| failed("没有可用的端点"))
}

fn is_last_candidate(endpoint: &str, candidates: &[String]) -> bool {
    candidates.last().map(|c| c == endpoint).unwrap_or(true)
}

fn describe_transport_error(t: &ureq::Transport) -> String {
    let detail = t.message().unwrap_or_default();
    match t.kind() {
        ureq::ErrorKind::Dns => "域名解析失败：检查 Base URL 与网络/DNS".to_string(),
        ureq::ErrorKind::ConnectionFailed => {
            if detail.contains("timed out") {
                "连接超时：服务无响应".to_string()
            } else {
                "连接失败：服务未启动或地址/端口错误".to_string()
            }
        }
        ureq::ErrorKind::InvalidUrl => "URL 无效：检查 Base URL 格式".to_string(),
        ureq::ErrorKind::InvalidProxyUrl => "代理地址无效：到「设置」检查网络代理".to_string(),
        ureq::ErrorKind::Io => {
            if detail.contains("timed out") || detail.contains("timeout") {
                format!("请求超时（{} 秒）：网络不通或服务响应过慢", OVERALL_TIMEOUT.as_secs())
            } else {
                format!("网络错误：{}", detail)
            }
        }
        other => format!("网络错误（{:?}）：{}", other, detail),
    }
}

/// 把 HTTP 响应归到 ok / no_key / error / retry（retry 表示 404 可换端点再试）
fn classify_response(
    http_status: u16,
    body: &str,
    key: Option<&str>,
    latency_ms: u64,
    _endpoint: &str,
    last_candidate: bool,
) -> ProviderTestResult {
    let mut base = ProviderTestResult {
        http_status: Some(http_status),
        latency_ms,
        ..Default::default()
    };
    if (200..300).contains(&http_status) {
        base.status = "ok".into();
        base.models = count_models(body);
        let models_note = base
            .models
            .map(|n| format!("，返回 {} 个模型", n))
            .unwrap_or_default();
        base.message = format!("连通正常{}（{} ms）", models_note, latency_ms);
        return base;
    }
    if http_status == 404 && !last_candidate {
        base.status = "retry".into();
        return base;
    }
    if http_status == 401 || http_status == 403 {
        if key.is_none() {
            base.status = "no_key".into();
            base.message =
                format!("端点可达，但需要认证（HTTP {}）——尚未保存 API Key", http_status);
        } else {
            base.status = "error".into();
            base.message = format!(
                "认证失败（HTTP {}）：Key 无效或无权限 {}",
                http_status,
                sanitize_snippet(body, key)
            );
        }
        return base;
    }
    base.status = "error".into();
    base.message = format!(
        "HTTP {} {}",
        http_status,
        sanitize_snippet(body, key)
    );
    base
}

/* ------------------------------------------------------------- 余额查询 */

/// 一次余额查询的返回（落库到 provider.balance 的就是它的 JSON）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBalance {
    pub provider_id: i64,
    pub provider_name: String,
    /// ok | no_key | error | unsupported
    pub status: String,
    pub http_status: Option<u16>,
    /// 账户是否可用（DeepSeek 的 is_available）
    pub is_available: Option<bool>,
    /// 币种（如 CNY）
    pub currency: String,
    /// 总余额（API 原样字符串，保留精度）
    pub total_balance: String,
    /// 赠送余额
    pub granted_balance: String,
    /// 充值余额
    pub topped_up_balance: String,
    pub message: String,
    pub tested_at: String,
    /// 实际请求的端点
    pub endpoint: String,
    /// 请求耗时
    pub latency_ms: u64,
}

/// DeepSeek /user/balance 响应体（字段为 API 原生 snake_case）
#[derive(Debug, Deserialize)]
struct DeepSeekBalanceResponse {
    #[serde(default)]
    is_available: Option<bool>,
    #[serde(default)]
    balance_infos: Vec<DeepSeekBalanceInfo>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekBalanceInfo {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    total_balance: String,
    #[serde(default)]
    granted_balance: String,
    #[serde(default)]
    topped_up_balance: String,
}

fn balance_failed(message: impl Into<String>) -> ProviderBalance {
    ProviderBalance {
        status: "error".into(),
        message: message.into(),
        tested_at: now_human(),
        ..Default::default()
    }
}

/// 查询供应商余额。`api_key` 为 None 或空串表示未保存 Key。
/// 尚未实现余额端点的类型返回 `unsupported`（界面据此隐藏/禁用入口）。
pub fn query_balance(
    base_url: &str,
    kind: &str,
    api_key: Option<&str>,
    proxy: &str,
) -> ProviderBalance {
    let kind_l = kind.to_lowercase();
    if !kind_l.contains("deepseek") {
        return ProviderBalance {
            status: "unsupported".into(),
            message: "该供应商类型暂不支持余额查询（当前支持：DeepSeek）".into(),
            tested_at: now_human(),
            ..Default::default()
        };
    }
    if base_url.trim().is_empty() {
        return balance_failed("缺少 Base URL");
    }

    // DeepSeek 余额端点在根路径：base 带 /v1 时先剥掉（OpenAI 兼容习惯）
    let mut base = normalize_base(base_url, &kind_l);
    if base.ends_with("/v1") {
        base.truncate(base.len() - 3);
    }
    let endpoint = format!("{}/user/balance", base);
    let key = api_key.map(str::trim).filter(|k| !k.is_empty());

    let agent = match build_agent(proxy) {
        Ok(a) => a,
        Err(e) => return balance_failed(e),
    };

    let mut req = agent.get(&endpoint);
    if let Some(k) = key {
        req = req.set("Authorization", &format!("Bearer {}", k));
    }
    let start = Instant::now();
    let response = req.call();
    let latency_ms = start.elapsed().as_millis() as u64;

    let mut result = match response {
        Ok(resp) => {
            let http_status = resp.status();
            let mut body = String::new();
            let _ = resp.into_reader().take(64 * 1024).read_to_string(&mut body);
            classify_balance_response(http_status, &body, key, &endpoint)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let mut body = String::new();
            let _ = resp
                .into_reader()
                .take(64 * 1024)
                .read_to_string(&mut body);
            classify_balance_response(code, &body, key, &endpoint)
        }
        Err(ureq::Error::Transport(t)) => {
            let mut r = balance_failed(describe_transport_error(&t));
            r.endpoint = endpoint;
            r
        }
    };
    result.latency_ms = latency_ms;
    result
}

fn classify_balance_response(
    http_status: u16,
    body: &str,
    key: Option<&str>,
    endpoint: &str,
) -> ProviderBalance {
    let mut result = ProviderBalance {
        http_status: Some(http_status),
        endpoint: endpoint.to_string(),
        tested_at: now_human(),
        ..Default::default()
    };
    if !(200..300).contains(&http_status) {
        if (http_status == 401 || http_status == 403) && key.is_none() {
            result.status = "no_key".into();
            result.message =
                format!("端点可达，但需要认证（HTTP {}）——尚未保存 API Key", http_status);
        } else {
            result.status = "error".into();
            result.message = format!(
                "HTTP {} {}",
                http_status,
                sanitize_snippet(body, key)
            );
        }
        return result;
    }
    match serde_json::from_str::<DeepSeekBalanceResponse>(body) {
        Ok(parsed) => {
            // 既没有 is_available 也没有 balance_infos —— 根本不像余额响应
            if parsed.is_available.is_none() && parsed.balance_infos.is_empty() {
                result.status = "error".into();
                result.message = format!(
                    "响应不是可识别的余额结构：{}",
                    sanitize_snippet(body, key)
                );
                return result;
            }
            result.status = "ok".into();
            result.is_available = parsed.is_available;
            match parsed.balance_infos.first() {
                Some(info) => {
                    result.currency = info.currency.clone();
                    result.total_balance = info.total_balance.clone();
                    result.granted_balance = info.granted_balance.clone();
                    result.topped_up_balance = info.topped_up_balance.clone();
                    result.message = format!(
                        "余额 {} {}（赠送 {} + 充值 {}）{}",
                        info.total_balance,
                        info.currency,
                        info.granted_balance,
                        info.topped_up_balance,
                        if parsed.is_available == Some(false) {
                            " · 账户不可用"
                        } else {
                            ""
                        }
                    );
                }
                None => {
                    result.message = "响应正常，但 balance_infos 为空（账户可能未开通计费）".into();
                }
            }
        }
        Err(_) => {
            result.status = "error".into();
            result.message = format!(
                "响应不是可识别的余额结构：{}",
                sanitize_snippet(body, key)
            );
        }
    }
    result
}
