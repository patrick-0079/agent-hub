// Windows 发布版不弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 无界面自检 / CLI 镜像：agenthub --scan-json
    if std::env::args().any(|a| a == "--scan-json") {
        agenthub_lib::cli_scan();
        return;
    }
    // T2/T3 沙箱自检：agenthub --self-test
    if std::env::args().any(|a| a == "--self-test") {
        agenthub_lib::cli_self_test();
        return;
    }
    // 回收站检查：agenthub --trash-json
    if std::env::args().any(|a| a == "--trash-json") {
        agenthub_lib::cli_trash();
        return;
    }
    // 数据库结构与写入路径自检：agenthub --db-check
    if std::env::args().any(|a| a == "--db-check") {
        agenthub_lib::cli_db_check();
        return;
    }
    // 供应商体检（连通性 + DeepSeek 余额）：agenthub --providers-check
    if std::env::args().any(|a| a == "--providers-check") {
        agenthub_lib::cli_providers_check();
        return;
    }
    // MCP 握手体检：agenthub --mcp-check
    if std::env::args().any(|a| a == "--mcp-check") {
        agenthub_lib::cli_mcp_check();
        return;
    }
    agenthub_lib::run()
}