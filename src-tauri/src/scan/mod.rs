//! 环境侦察引擎：一次扫描产出整机 Agent 环境的只读快照。
//!
//! 阶段顺序有依赖：npm 全局包要在 Agent 判定之前拿到（全局包是重要的安装证据）。

pub mod agents;
pub mod executables;
pub mod mcp;
pub mod npm_pkgs;
pub mod providers;
pub mod python_envs;
pub mod skills;

use crate::agentdef;
use crate::capability::EvalContext;
use crate::model::{AppSettings, HostInfo, Progress, ScanSnapshot};
use std::time::Instant;

/// 进度上报器：把阶段与日志实时推给 GUI（任务控制台）。
pub struct Reporter<'a> {
    sink: &'a dyn Fn(Progress),
}

impl<'a> Reporter<'a> {
    pub fn new(sink: &'a dyn Fn(Progress)) -> Self {
        Self { sink }
    }

    fn emit(&self, phase: &str, level: &str, current: usize, total: usize, message: String) {
        (self.sink)(Progress {
            phase: phase.to_string(),
            message,
            current,
            total,
            level: level.to_string(),
            ts: crate::util::now_human(),
        });
    }

    pub fn info(&self, phase: &str, message: impl Into<String>) {
        self.emit(phase, "info", 0, 0, message.into());
    }

    pub fn warn(&self, phase: &str, message: impl Into<String>) {
        self.emit(phase, "warn", 0, 0, message.into());
    }

    pub fn error(&self, phase: &str, message: impl Into<String>) {
        self.emit(phase, "error", 0, 0, message.into());
    }

    pub fn step(&self, phase: &str, current: usize, total: usize, message: impl Into<String>) {
        self.emit(phase, "info", current, total, message.into());
    }

    pub fn done(&self, phase: &str, message: impl Into<String>) {
        self.emit(phase, "done", 1, 1, message.into());
    }
}

const TOTAL_STEPS: usize = 7;

/// 执行完整扫描。`reporter` 会在每个阶段被调用，用于 GUI 实时可视化。
pub fn scan(settings: &AppSettings, host: &HostInfo, reporter: &Reporter) -> ScanSnapshot {
    let started = Instant::now();
    let mut warnings: Vec<String> = Vec::new();

    reporter.step("环境探测", 1, TOTAL_STEPS, "探测可执行文件与运行时…");
    let executables = executables::scan(settings);
    let found = executables.iter().filter(|e| e.found).count();
    reporter.info(
        "环境探测",
        format!("已定位 {}/{} 个工具链组件", found, executables.len()),
    );

    reporter.step("npm 全局包", 2, TOTAL_STEPS, "读取全局包清单…");
    let npm_list = npm_pkgs::scan_npm_packages(&executables, &mut warnings);
    reporter.info("npm 全局包", format!("读取到 {} 个全局包", npm_list.len()));

    reporter.step("Agent 发现", 3, TOTAL_STEPS, "加载 Agent 定义并求值证据规则…");
    let loaded = agentdef::load(settings);
    reporter.info(
        "Agent 发现",
        format!(
            "已加载 {} 个 Agent 定义（{} 个来自用户目录）",
            loaded.defs.len(),
            loaded.defs.iter().filter(|d| d.from_user_dir).count()
        ),
    );
    warnings.extend(loaded.warnings.iter().cloned());

    let ctx = EvalContext::new(&npm_list, settings);
    let mut agent_list = agents::evaluate(&loaded.defs, &ctx);
    let agents: Vec<_> = agent_list
        .iter()
        .filter(|a| agents::is_agent_kind(&a.kind))
        .collect();
    let installed = agents.iter().filter(|a| a.installed).count();
    let configured = agents.iter().filter(|a| a.status == "configured").count();
    let leftover = agents.iter().filter(|a| a.status == "leftover").count();
    reporter.info(
        "Agent 发现",
        format!(
            "已安装 {} 个 Agent；另发现 {} 个仅配置、{} 个残留，{} 个宿主应用",
            installed,
            configured,
            leftover,
            agent_list.iter().filter(|a| a.kind == "host").count()
        ),
    );

    // 状态异常但仍有痕迹的 Agent 需要用户注意
    for agent in agent_list
        .iter()
        .filter(|a| agents::is_agent_kind(&a.kind) && a.status == "configured")
        .take(4)
    {
        warnings.push(format!(
            "「{}」仅发现配置文件，未找到命令行程序 —— 可能已卸载或使用便携版；相关配置不会被同步",
            agent.name
        ));
    }
    for agent in agent_list
        .iter()
        .filter(|a| agents::is_agent_kind(&a.kind) && a.status == "leftover")
        .take(4)
    {
        warnings.push(format!(
            "「{}」存在残留目录但程序本体缺失（状态：残留），资源不会被纳入同步目标",
            agent.name
        ));
    }

    reporter.step("MCP 配置", 4, TOTAL_STEPS, "按定义声明的位置解析 MCP 配置…");
    let mcp_servers = mcp::scan_mcp(&loaded.defs, &agent_list, &mut warnings);
    reporter.info("MCP 配置", format!("解析到 {} 个 MCP 服务器条目", mcp_servers.len()));

    reporter.step("Skills", 5, TOTAL_STEPS, "扫描定义中 role = skills 的目录…");
    let skill_list = skills::scan_skills(settings, &loaded.defs, &agent_list, &mut warnings);
    reporter.info("Skills", format!("发现 {} 个 Skill", skill_list.len()));

    reporter.step("Python 环境", 6, TOTAL_STEPS, "枚举 conda / uv / venv 环境…");
    let py_envs = python_envs::scan_python_envs(settings, &executables, &mut warnings);
    reporter.info("Python 环境", format!("发现 {} 个 Python 环境", py_envs.len()));

    reporter.step("供应商线索", 7, TOTAL_STEPS, "汇总供应商配置线索…");
    let provider_hints = providers::scan_providers(&loaded.defs, &agent_list, &mut warnings);
    reporter.info(
        "供应商线索",
        format!("发现 {} 条供应商配置线索", provider_hints.len()),
    );

    // 回填每个 Agent 的 MCP / Skill 计数
    for agent in agent_list.iter_mut() {
        agent.mcp_count = mcp_servers
            .iter()
            .filter(|m| m.source_agent_id == agent.id)
            .count();
        agent.skill_count = skill_list
            .iter()
            .filter(|s| s.source_agent_id == agent.id)
            .count();
    }

    let duration_ms = started.elapsed().as_millis() as u64;

    // 汇总阶段的告警统一在任务控制台里可视化
    for w in &warnings {
        reporter.warn("告警", w.clone());
    }

    reporter.done(
        "完成",
        format!(
            "扫描完成：{} 个 Agent 已安装、{} 个 Skill、{} 个 MCP、{} 个 Python 环境、{} 个全局包（{:.1}s）",
            installed,
            skill_list.len(),
            mcp_servers.len(),
            py_envs.len(),
            npm_list.len(),
            duration_ms as f64 / 1000.0
        ),
    );

    ScanSnapshot {
        scanned_at: crate::util::now_rfc3339(),
        duration_ms,
        host: host.clone(),
        executables,
        agents: agent_list,
        python_envs: py_envs,
        npm_packages: npm_list,
        mcp_servers,
        skills: skill_list,
        provider_hints,
        warnings,
    }
}