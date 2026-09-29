//! AgentHub 后端入口：装配状态、注册命令、启动窗口。

mod commands;
pub mod actions;
pub mod agentdef;
pub mod capability;
pub mod handshake;
pub mod merge;
pub mod model;
pub mod probe;
pub mod profile;
pub mod scan;
pub mod share;
pub mod snapdiff;
pub mod store;
pub mod sync;
pub mod util;
pub mod vault;
pub mod yaml;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// AgentHub 数据目录（与 Tauri 的 app_data_dir 保持一致）。
///
/// 可用 `AGENTHUB_DATA_DIR` 覆盖 —— 便携模式与自检沙箱都依赖这一点。
pub fn default_data_dir() -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("AGENTHUB_DATA_DIR") {
        if !custom.trim().is_empty() {
            return std::path::PathBuf::from(custom);
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        std::path::PathBuf::from(appdata).join("dev.agenthub.desktop")
    } else {
        util::home_dir().join(".agenthub")
    }
}

/// 无界面回收站检查：`agenthub --trash-json`
pub fn cli_trash() {
    use actions::*;
    let stats = trash_stats();
    println!("回收站目录: {}", stats.dir);
    println!(
        "条目 {} 个 · 对象 {} 个 · 占用 {}",
        stats.entries,
        stats.items,
        util::format_bytes(stats.bytes)
    );
    for entry in list_trash() {
        println!(
            "\n[{}] {} / {} 个对象 / {} / {}",
            entry.kind,
            entry.name,
            entry.item_count,
            util::format_bytes(entry.size),
            entry.summary
        );
        match trash_detail(&entry.name) {
            Ok(detail) => {
                let restorable = detail.items.iter().filter(|i| i.restorable).count();
                println!("  可恢复 {} / {}", restorable, detail.items.len());
                for item in detail.items.iter().take(3) {
                    println!(
                        "    - {} [{}] → {}",
                        item.original,
                        item.kind,
                        item.link_target.clone().unwrap_or_else(|| "（无链接目标）".into())
                    );
                }
                if detail.items.len() > 3 {
                    println!("    … 其余 {} 项", detail.items.len() - 3);
                }
            }
            Err(e) => println!("  读取详情失败: {}", e),
        }
    }
}

/// 无界面数据库自检：`agenthub --db-check`
///
/// 会在真实数据库上执行一次迁移与「写入 → 读回 → 删除」的往返验证（用临时条目，
/// 结束后清理）。老库缺列导致的写入失败就是靠这个路径暴露的。
pub fn cli_db_check() {
    let db = default_data_dir().join("agenthub.db");
    println!("数据库: {}", db.display());
    let store = match store::Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("打开失败：{}", e);
            std::process::exit(1);
        }
    };

    let required: &[(&str, &[&str])] = &[
        ("mcp_server", &["enabled", "notes", "created_at", "updated_at"]),
        ("provider", &["enabled", "notes"]),
        ("sync_state", &["agent_id", "file", "root", "keys_json"]),
        ("backup", &["target", "backup_path", "bytes"]),
    ];
    let mut missing_total = 0usize;
    println!("\n=== 表结构 ===");
    for (table, needed) in required {
        let cols = store.table_columns(table);
        let missing: Vec<&str> = needed
            .iter()
            .filter(|c| !cols.iter().any(|x| x == *c))
            .copied()
            .collect();
        missing_total += missing.len();
        println!(
            "  {:<12} {:<3} 列   {} 行{}",
            table,
            cols.len(),
            store.count_of(table),
            if missing.is_empty() {
                "   ✅ 关键列齐全".to_string()
            } else {
                format!("   ❌ 缺列：{}", missing.join("、"))
            }
        );
    }

    println!("\n=== 写入往返验证（临时条目，结束后删除） ===");
    let probe = model::McpResource {
        id: 0,
        name: "__agenthub_selfcheck__".to_string(),
        transport: "stdio".to_string(),
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "selfcheck".to_string()],
        env: vec![model::EnvPair {
            key: "PROBE".to_string(),
            value: "%PROBE%".to_string(),
        }],
        url: String::new(),
        enabled: true,
        notes: "自检临时条目".to_string(),
        health: Default::default(),
    };
    match store.mcp_upsert(&probe) {
        Ok(id) => {
            let read_back = store
                .mcp_list()
                .into_iter()
                .find(|r| r.name == probe.name);
            match read_back {
                Some(item) => println!(
                    "  ✅ MCP 写入/读回成功（id={}，{} 个参数，{} 个环境变量）",
                    id,
                    item.args.len(),
                    item.env.len()
                ),
                None => println!("  ❌ 写入成功但读不回来"),
            }
            let _ = store.mcp_delete(id);
            println!("  ✅ 已清理临时条目（当前 {} 行）", store.count_of("mcp_server"));
        }
        Err(e) => println!("  ❌ MCP 写入失败：{}", e),
    }

    if missing_total > 0 {
        println!("\n结论：仍有 {} 个关键列缺失，请把上面的信息反馈给开发者", missing_total);
        std::process::exit(1);
    }
    println!("\n结论：表结构与写入路径均正常");
}

/// 沙箱自检：在临时目录里完整跑一遍 T2/T3 链路，不触碰真实技能库。
///
/// 用法：`agenthub --self-test`
/// 极简 HTTP mock（自检用）：读取请求行，按路径路由到 (状态码, 响应体)。
/// mock 线程可能仍在 accept() 等待额外连接：各自都有连接数上限，随进程退出回收。
fn spawn_mock_server(
    router: fn(&str) -> (u16, String),
    max_connections: usize,
) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..max_connections {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(600)));
            let mut req = Vec::new();
            let mut chunk = [0u8; 2048];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        req.extend_from_slice(&chunk[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let first_line = String::from_utf8_lossy(&req);
            let path = first_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            let (status, body) = router(&path);
            let reason = match status {
                200 => "OK",
                401 => "Unauthorized",
                404 => "Not Found",
                _ => "Error",
            };
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                reason,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}", port)
}

pub fn cli_self_test() {
    use actions::BrokenRef;
    use std::path::Path;

    let mut pass = 0usize;
    let mut fail = 0usize;
    macro_rules! check {
        ($cond:expr, $($arg:tt)*) => {{
            let ok = $cond;
            if ok { pass += 1; println!("  ✅ {}", format!($($arg)*)); }
            else { fail += 1; println!("  ❌ {}", format!($($arg)*)); }
        }};
    }

    let base = std::env::temp_dir().join("agenthub-selftest");
    let _ = std::fs::remove_dir_all(&base);
    let source = base.join("source");
    let library = base.join("library");
    // 各节共用的 selftest-*.db 开跑前清一次：多个小节有意共享状态，
    // 但跨轮残留会让「备份数 == 1」这类计数断言随机失败（CI 必踩）
    let _ = std::fs::remove_file(default_data_dir().join("selftest-sync.db"));
    let _ = std::fs::remove_file(default_data_dir().join("selftest-profile.db"));
    println!("\n=== AgentHub T2/T3 沙箱自检 ===");
    println!("沙箱: {}", base.display());
    println!("数据目录: {}\n", default_data_dir().display());

    // 造两个 Skill
    for name in ["alpha-skill", "beta-skill"] {
        let dir = source.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: 沙箱测试用 Skill\n---\n\n# {}\n",
                name, name
            ),
        )
        .unwrap();
        std::fs::write(dir.join("extra.md"), "content").unwrap();
    }

    println!("[1] 发现来源目录中的 Skill");
    match actions::discover_skills(&source, Some(&library)) {
        Ok(list) => {
            check!(list.len() == 2, "发现 {} 个 Skill（期望 2）", list.len());
            check!(
                list.iter().all(|s| s.has_manifest),
                "全部含 SKILL.md，且已解析 description"
            );
        }
        Err(e) => {
            fail += 1;
            println!("  ❌ 发现失败: {}", e);
        }
    }

    println!("[2] 导入计划（T2 · link 模式）");
    match actions::plan_import(&source, &library, "link", &[]) {
        Ok(plan) => {
            check!(plan.items.len() == 2, "计划 {} 项", plan.items.len());
            check!(
                plan.items.iter().all(|i| i.action == "create-link"),
                "动作均为 create-link，级别 {} / {}", plan.tier_code, plan.tier
            );
        }
        Err(e) => {
            fail += 1;
            println!("  ❌ 计划失败: {}", e);
        }
    }

    println!("[3] 执行导入并校验链接可解析");
    let imported = actions::apply_import(&source, &library, "link", &[]);
    check!(imported.ok, "导入成功: {}", imported.summary);
    check!(imported.manifest.is_some(), "已写入可恢复清单");
    for name in ["alpha-skill", "beta-skill"] {
        let p = library.join(name);
        check!(
            actions::is_link(&p) && p.join("SKILL.md").is_file(),
            "{}：是链接={} 可解析={}",
            name,
            actions::is_link(&p),
            p.join("SKILL.md").is_file()
        );
    }
    // 重复导入不应覆盖
    let again = actions::plan_import(&source, &library, "link", &[]).unwrap();
    check!(
        again.items.iter().all(|i| i.action == "skip"),
        "重复导入全部识别为 skip（不覆盖已有内容）"
    );

    println!("[4] 模拟「删除内容但没清链接」：删掉来源里的 beta-skill");
    std::fs::remove_dir_all(source.join("beta-skill")).unwrap();
    let beta_link = library.join("beta-skill");
    check!(!beta_link.exists() && actions::is_link(&beta_link), "beta-skill 已成为失效链接");

    let broken = vec![BrokenRef {
        path: beta_link.to_string_lossy().to_string(),
        target: source.join("beta-skill").to_string_lossy().to_string(),
        kind: "junction".into(),
        owner: "沙箱".into(),
        name: "beta-skill".into(),
    }];

    println!("[5] 一键清理失效链接计划");
    let cplan = actions::plan_cleanup_broken(&broken);
    check!(
        cplan.items.len() == 1 && cplan.items[0].risk == "destructive",
        "计划 {} 项，级别 {} / {}", cplan.items.len(), cplan.tier_code, cplan.tier
    );

    println!("[5b] 重建链接：把失效链接指向另一个技能库");
    let library2 = base.join("library2");
    let revived = library2.join("beta-skill");
    std::fs::create_dir_all(&revived).unwrap();
    std::fs::write(
        revived.join("SKILL.md"),
        "---\nname: beta-skill\ndescription: 从另一个库恢复\n---\n",
    )
    .unwrap();
    let rplan = actions::plan_relink(&broken, &library2);
    check!(
        rplan.items.iter().any(|i| i.action == "create-link"),
        "计划中识别出可在新库中找到: {}", rplan.summary
    );
    let relinked = actions::apply_relink(&broken, &library2);
    check!(relinked.ok, "重建完成: {}", relinked.summary);
    check!(
        beta_link.join("SKILL.md").is_file(),
        "链接重新可用（指向新库并解析到内容）"
    );
    check!(
        std::fs::read_link(&beta_link)
            .map(|t| t.to_string_lossy().contains("library2"))
            .unwrap_or(false),
        "链接目标已切换到 library2"
    );

    println!("[5c] 再次制造失效后清理（回到「删了内容没清链接」的场景）");
    std::fs::remove_dir_all(&revived).unwrap();
    check!(!beta_link.exists() && actions::is_link(&beta_link), "链接又变成失效状态");

    println!("[6] 执行清理（失效链接应整体移入回收站）");
    let cleaned = actions::apply_cleanup_broken(&broken);
    check!(cleaned.ok, "清理完成: {}", cleaned.summary);
    check!(
        !actions::is_link(&beta_link) && !beta_link.exists(),
        "失效链接已从原位移除"
    );
    check!(source.join("alpha-skill").is_dir(), "来源目录内容完全未受影响");

    let trash_after_cleanup = actions::list_trash();
    let batch = trash_after_cleanup.iter().find(|t| t.kind == "link");
    check!(
        batch.is_some(),
        "回收站里出现了清理条目（当前 {} 条）",
        trash_after_cleanup.len()
    );

    println!("[7] 从回收站恢复这一批链接");
    if let Some(entry) = batch {
        check!(
            entry.item_count == 1 && entry.summary.contains("清理"),
            "条目含 {} 个对象，摘要：{}",
            entry.item_count,
            entry.summary
        );
        let restored = actions::restore_trash(&entry.name);
        check!(restored.ok, "恢复完成: {}", restored.summary);
        check!(
            actions::is_link(&beta_link) && !beta_link.exists(),
            "链接回到原位，且仍指向已不存在的目标（指向关系原样保留）"
        );
    }

    println!("[7b] 再清理一次，改用「依清单恢复」路径");
    let broken_again = vec![BrokenRef {
        path: beta_link.to_string_lossy().to_string(),
        target: source.join("beta-skill").to_string_lossy().to_string(),
        kind: "junction".into(),
        owner: "沙箱".into(),
        name: "beta-skill".into(),
    }];
    let cleaned2 = actions::apply_cleanup_broken(&broken_again);
    if let Some(manifest) = &cleaned2.manifest {
        let r = actions::restore_manifest(Path::new(manifest));
        check!(r.ok, "依清单恢复（清单在回收站内）: {}", r.summary);
        check!(actions::is_link(&beta_link), "链接再次回到原位");
    }

    println!("[7c] 单独删除链接也必须进回收站");
    let del_link = actions::apply_delete_skill(&beta_link);
    check!(del_link.ok && !actions::is_link(&beta_link), "链接已删除: {}", del_link.summary);
    let link_entry = actions::list_trash()
        .into_iter()
        .find(|t| t.kind == "link" && t.item_count == 1);
    check!(link_entry.is_some(), "回收站中出现了链接条目");
    if let Some(entry) = link_entry {
        let r = actions::restore_trash(&entry.name);
        check!(
            r.ok && actions::is_link(&beta_link),
            "链接从回收站恢复成功: {}",
            r.summary
        );
    }

    println!("[7d] 路径别名去重（junction 指向同一处时不应重复处理）");
    // 先恢复一个失效链接出来，再通过别名路径引用它两次
    if let Some(entry) = actions::list_trash().into_iter().find(|t| t.kind == "link") {
        actions::restore_trash(&entry.name);
    }
    let alias_dir = base.join("library-alias");
    let _ = std::fs::remove_dir_all(&alias_dir);
    match actions::create_dir_link(&alias_dir, &library) {
        Ok(_) => {
            let direct = library.join("beta-skill");
            let via_alias = alias_dir.join("beta-skill");
            check!(
                actions::is_link(&via_alias) || via_alias.exists(),
                "已通过 junction 造出别名路径 {}",
                via_alias.to_string_lossy()
            );
            let dupes = vec![
                BrokenRef {
                    path: direct.to_string_lossy().to_string(),
                    target: "缺失".into(),
                    kind: "junction".into(),
                    owner: "沙箱".into(),
                    name: "beta-skill".into(),
                },
                BrokenRef {
                    path: via_alias.to_string_lossy().to_string(),
                    target: "缺失".into(),
                    kind: "junction".into(),
                    owner: "沙箱".into(),
                    name: "beta-skill".into(),
                },
            ];
            let alias_plan = actions::plan_cleanup_broken(&dupes);
            check!(
                alias_plan.items.len() == 1 && alias_plan.warnings.iter().any(|w| w.contains("别名")),
                "计划已合并为 {} 项并给出别名提示",
                alias_plan.items.len()
            );
            let alias_result = actions::apply_cleanup_broken(&dupes);
            check!(
                alias_result.ok
                    && alias_result.steps.iter().all(|s| s.ok)
                    && alias_result.summary.contains("重复别名"),
                "执行未把重复项误报为失败: {}",
                alias_result.summary
            );
            let _ = actions::remove_link(&alias_dir);
        }
        Err(e) => {
            fail += 1;
            println!("  ❌ 无法创建别名 junction: {}", e);
        }
    }

    println!("[8] 删除范围白名单");
    match actions::plan_delete_skill(Path::new("C:\\Windows"), &[library.clone()], 0) {
        Ok(_) => {
            fail += 1;
            println!("  ❌ 库外路径竟然被允许删除");
        }
        Err(e) => {
            pass += 1;
            println!("  ✅ 库外路径被拒绝: {}", e);
        }
    }

    println!("[9] 删除真实目录 → 回收站 → 恢复");
    let manual = library.join("manual-skill");
    std::fs::create_dir_all(&manual).unwrap();
    std::fs::write(manual.join("SKILL.md"), "---\nname: manual-skill\n---\n").unwrap();
    match actions::plan_delete_skill(&manual, &[library.clone()], 3) {
        Ok(p) => check!(
            p.warnings.iter().any(|w| w.contains("3 个链接")),
            "计划中提示了链接影响面: {}",
            p.warnings.join("；")
        ),
        Err(e) => {
            fail += 1;
            println!("  ❌ 计划失败: {}", e);
        }
    }
    let del = actions::apply_delete_skill(&manual);
    check!(del.ok && !manual.exists(), "已移出原位: {}", del.summary);
    let trash = actions::list_trash();
    let dir_entry = trash.iter().find(|t| t.kind == "dir");
    check!(dir_entry.is_some(), "回收站记录 {} 条，其中有目录条目", trash.len());
    if let Some(entry) = dir_entry {
        let restored = actions::restore_trash(&entry.name);
        check!(
            restored.ok && manual.join("SKILL.md").is_file(),
            "已从回收站恢复: {}",
            restored.summary
        );
    }

    println!("[10] 越权清理临时目录");
    let evil = actions::cleanup_tmp(Path::new("C:\\Windows"));
    check!(!evil.ok, "非 AgentHub 临时目录被拒绝清理");

    println!("[11] 回收站管理：统计 / 详情 / 部分恢复 / 永久删除");
    let manual2 = library.join("manual-skill-2");
    std::fs::create_dir_all(&manual2).unwrap();
    std::fs::write(manual2.join("SKILL.md"), "---\nname: manual-skill-2\n---\n").unwrap();
    let _ = actions::apply_delete_skill(&manual2);

    let stats = actions::trash_stats();
    check!(
        stats.entries >= 1 && stats.items >= 1 && stats.bytes > 0,
        "统计：{} 个条目 / {} 个对象 / {}",
        stats.entries,
        stats.items,
        crate::util::format_bytes(stats.bytes)
    );

    let target_entry = actions::list_trash()
        .into_iter()
        .find(|t| t.original.as_deref() == Some(manual2.to_string_lossy().as_ref()));
    check!(target_entry.is_some(), "能在回收站中定位到刚删除的条目");
    if let Some(entry) = target_entry {
        match actions::trash_detail(&entry.name) {
            Ok(detail) => {
                check!(
                    detail.items.len() == 1
                        && detail.items[0].restorable
                        && detail.items[0].kind == "dir",
                    "详情：{} 个对象，类型 {}，可恢复 {}",
                    detail.items.len(),
                    detail.items[0].kind,
                    detail.items[0].restorable
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ❌ 读取详情失败: {}", e);
            }
        }

        // 部分恢复（这里只有 1 个对象，等价于整条恢复）
        let stored: Vec<String> = actions::trash_detail(&entry.name)
            .map(|d| d.items.iter().map(|i| i.stored.clone()).collect())
            .unwrap_or_default();
        let partial = actions::restore_trash_items(&entry.name, &stored);
        check!(
            partial.ok && manual2.join("SKILL.md").is_file(),
            "按对象恢复成功: {}",
            partial.summary
        );

        // 再删一次，然后永久删除该条目
        let _ = actions::apply_delete_skill(&manual2);
        let again = actions::list_trash()
            .into_iter()
            .find(|t| t.original.as_deref() == Some(manual2.to_string_lossy().as_ref()))
            .map(|t| t.name);
        if let Some(name) = again {
            match actions::plan_purge_trash(&[name.clone()]) {
                Ok(plan) => check!(
                    plan.summary.contains("永久删除") && !plan.warnings.is_empty(),
                    "永久删除计划已标红并给出不可恢复警告"
                ),
                Err(e) => {
                    fail += 1;
                    println!("  ❌ 永久删除计划失败: {}", e);
                }
            }
            let purged = actions::apply_purge_trash(&[name.clone()]);
            check!(
                purged.ok && !actions::list_trash().iter().any(|t| t.name == name),
                "条目已永久删除: {}",
                purged.summary
            );
        }
    }

    println!("[12] 清空回收站与按时间清理");
    let leftover = library.join("manual-skill-3");
    std::fs::create_dir_all(&leftover).unwrap();
    std::fs::write(leftover.join("SKILL.md"), "---\nname: manual-skill-3\n---\n").unwrap();
    let _ = actions::apply_delete_skill(&leftover);
    let before = actions::trash_stats().entries;
    let empty = actions::apply_purge_older_than(0);
    check!(
        empty.ok && actions::trash_stats().entries == 0,
        "清空回收站：{} 个条目 → 0（{}）",
        before,
        empty.summary
    );
    check!(
        actions::plan_purge_older_than(0).is_err(),
        "空回收站再清空会如实报错而不是假装成功"
    );

    println!("[13] 配置写入（T2）：JSON 结构合并 / 备份 / 回滚 / 移除清理");
    {
        use crate::agentdef::{
            AgentFile, AgentMeta, CapabilityPolicy, LoadedDef, McpSource,
        };
        use crate::model::{AgentTarget, McpResource};
        use crate::sync::SyncRequest;

        let sync_dir = base.join("sync");
        std::fs::create_dir_all(&sync_dir).unwrap();
        let target_file = sync_dir.join("agent-config.json");
        // 用户手写的配置：必须被完整保留
        std::fs::write(
            &target_file,
            "{\n  \"userKey\": \"must-keep\",\n  \"nested\": {\n    \"a\": 1\n  },\n  \"mcpServers\": {\n    \"hand-written\": {\n      \"command\": \"mine\"\n    }\n  }\n}\n",
        )
        .unwrap();

        let store = crate::store::Store::open(&default_data_dir().join("selftest-sync.db")).unwrap();
        let def = LoadedDef {
            file: AgentFile {
                agent: AgentMeta {
                    id: "sandbox-agent".into(),
                    name: "沙箱 Agent".into(),
                    kind: "cli".into(),
                    ..Default::default()
                },
                mcp: vec![McpSource {
                    file: target_file.to_string_lossy().to_string(),
                    root: "mcpServers".into(),
                    format: "json".into(),
                    strategy: String::new(),
                    marker: String::new(),
                    entry_style: String::new(),
                }],
                capabilities: CapabilityPolicy {
                    max_tier: "deploy".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            source: "沙箱".into(),
            from_user_dir: false,
            used_capabilities: vec![],
        };
        let agents = vec![AgentTarget {
            id: "sandbox-agent".into(),
            name: "沙箱 Agent".into(),
            status: "installed".into(),
            installed: true,
            ..Default::default()
        }];

        let res_a = McpResource {
            id: 0,
            name: "managed-a".into(),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "server-a".into()],
            env: vec![],
            url: String::new(),
            enabled: true,
            notes: String::new(),
            health: Default::default(),
        };
        let resources = vec![res_a.clone()];
        let defs = vec![def];
        let no_agents: Vec<String> = vec![];

        let plan = crate::sync::plan_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &resources,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        })
        .expect("应能生成计划");
        check!(
            plan.targets.len() == 1 && plan.targets[0].supported && plan.targets[0].added == 1,
            "计划：{} 个目标，新增 {} 项，未改动 {} 项",
            plan.targets.len(),
            plan.targets[0].added,
            plan.targets[0].unchanged
        );
        check!(
            plan.targets[0].diff.contains("+") && plan.targets[0].diff.contains("managed-a"),
            "计划里带了 diff（{} 行）",
            plan.targets[0].diff.lines().count()
        );

        let applied = crate::sync::apply_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &resources,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        check!(applied.ok, "写入完成: {}", applied.summary);

        let after = std::fs::read_to_string(&target_file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
        check!(
            parsed["userKey"] == "must-keep" && parsed["nested"]["a"] == 1,
            "用户手写内容完整保留（userKey / nested）"
        );
        check!(
            parsed["mcpServers"]["hand-written"]["command"] == "mine",
            "用户自己写的 mcpServers 条目未被删除"
        );
        check!(
            parsed["mcpServers"]["managed-a"]["command"] == "npx",
            "受管条目已写入"
        );
        check!(
            parsed["mcpServers"]["managed-a"]["args"][1] == "server-a",
            "args 数组正确落盘"
        );

        let backups = store.backup_list(10);
        check!(
            backups.len() == 1 && std::path::Path::new(&backups[0].backup_path).is_file(),
            "写入前已备份（{} 份，{}）",
            backups.len(),
            crate::util::format_bytes(backups[0].bytes)
        );

        // 再跑一次：内容一致 → 应全部 unchanged、不重复写入
        let again = crate::sync::plan_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &resources,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        })
        .unwrap();
        check!(
            again.targets[0].unchanged == 1 && again.targets[0].added == 0 && again.targets[0].diff.is_empty(),
            "幂等：重复同步识别为 {} 项未变、diff 为空",
            again.targets[0].unchanged
        );

        // 换成另一个资源：managed-a 应被清理，managed-b 写入
        let res_b = McpResource {
            name: "managed-b".into(),
            transport: "http".into(),
            url: "https://example.com/mcp".into(),
            enabled: true,
            ..Default::default()
        };
        let swapped = vec![res_b.clone()];
        let plan2 = crate::sync::plan_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &swapped,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        })
        .unwrap();
        check!(
            plan2.targets[0].added == 1 && plan2.targets[0].removed == 1,
            "替换资源：新增 {} / 清理 {}",
            plan2.targets[0].added,
            plan2.targets[0].removed
        );
        crate::sync::apply_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &swapped,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        let after2 = std::fs::read_to_string(&target_file).unwrap();
        let parsed2: serde_json::Value = serde_json::from_str(&after2).unwrap();
        check!(
            parsed2["mcpServers"]["managed-a"].is_null()
                && parsed2["mcpServers"]["managed-b"]["url"] == "https://example.com/mcp",
            "上次受管的键已清理，新键已写入"
        );
        check!(
            parsed2["mcpServers"]["hand-written"]["command"] == "mine",
            "清理只针对受管键，用户手写条目仍在"
        );

        // 回滚到第一次备份
        let rollback = crate::sync::restore_backup(&store, backups[0].id);
        check!(rollback.ok, "回滚: {}", rollback.summary);
        let restored = std::fs::read_to_string(&target_file).unwrap();
        let parsed3: serde_json::Value = serde_json::from_str(&restored).unwrap();
        check!(
            parsed3["mcpServers"]["managed-a"].is_null()
                && parsed3["mcpServers"]["managed-b"].is_null()
                && parsed3["mcpServers"]["hand-written"]["command"] == "mine",
            "回滚后回到原始内容（仅剩用户手写条目）"
        );

        println!("[13b] 非受管同名条目：默认跳过，显式允许时才覆盖");
        let collide = McpResource {
            id: 0,
            name: "hand-written".into(),
            transport: "stdio".into(),
            command: "should-not-overwrite".into(),
            args: vec![],
            env: vec![],
            url: String::new(),
            enabled: true,
            notes: String::new(),
            health: Default::default(),
        };
        let with_collide = vec![collide];
        let plan3 = crate::sync::plan_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &with_collide,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        })
        .unwrap();
        check!(
            plan3.targets[0].skipped == 1
                && plan3.targets[0].added == 0
                && plan3.warnings.iter().any(|w| w.contains("已跳过")),
            "默认跳过非受管同名条目（skipped={}）并给出提示",
            plan3.targets[0].skipped
        );
        crate::sync::apply_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &with_collide,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        let kept = std::fs::read_to_string(&target_file).unwrap();
        let kept_json: serde_json::Value = serde_json::from_str(&kept).unwrap();
        check!(
            kept_json["mcpServers"]["hand-written"]["command"] == "mine",
            "用户手写条目内容未被改写"
        );

        let plan4 = crate::sync::plan_sync(&SyncRequest {
            defs: &defs,
            agents: &agents,
            resources: &with_collide,
            agent_ids: &no_agents,
            overwrite_unmanaged: true,
            store: &store,
        })
        .unwrap();
        check!(
            plan4.targets[0].updated == 1 && plan4.targets[0].skipped == 0,
            "显式允许覆盖时按更新处理（updated={}）",
            plan4.targets[0].updated
        );

        println!("[13c] 目标文件不存在时应能新建");
        let fresh = sync_dir.join("brand-new.json");
        let _ = std::fs::remove_file(&fresh);
        let mut def2 = defs[0].clone();
        def2.file.mcp[0].file = fresh.to_string_lossy().to_string();
        let one = vec![McpResource {
            id: 0,
            name: "brand-new".into(),
            transport: "stdio".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-fetch".into()],
            env: vec![],
            url: String::new(),
            enabled: true,
            notes: String::new(),
            health: Default::default(),
        }];
        let applied3 = crate::sync::apply_sync(&SyncRequest {
            defs: &[def2],
            agents: &agents,
            resources: &one,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        let created = std::fs::read_to_string(&fresh).unwrap_or_default();
        let created_json: serde_json::Value =
            serde_json::from_str(&created).unwrap_or(serde_json::Value::Null);
        check!(
            applied3.ok && created_json["mcpServers"]["brand-new"]["command"] == "uvx",
            "目标文件不存在时正常新建: {}",
            applied3.summary
        );
    }

    println!("[14] 配置写入（T2）：TOML 托管块");
    {
        use crate::agentdef::{AgentFile, AgentMeta, CapabilityPolicy, LoadedDef, McpSource};
        use crate::model::{AgentTarget, McpResource};
        use crate::sync::SyncRequest;

        let sync_dir = base.join("sync-toml");
        std::fs::create_dir_all(&sync_dir).unwrap();
        let toml_file = sync_dir.join("config.toml");
        std::fs::write(
            &toml_file,
            "# 用户自己的配置\nmodel = \"gpt-5\"\n\n[user_section]\nkeep = \"yes\"\n",
        )
        .unwrap();

        let store = crate::store::Store::open(&default_data_dir().join("selftest-sync.db")).unwrap();
        let def = LoadedDef {
            file: AgentFile {
                agent: AgentMeta {
                    id: "sandbox-codex".into(),
                    name: "沙箱 Codex".into(),
                    kind: "cli".into(),
                    ..Default::default()
                },
                mcp: vec![McpSource {
                    file: toml_file.to_string_lossy().to_string(),
                    root: "mcp_servers".into(),
                    format: "toml".into(),
                    strategy: "managed-block".into(),
                    marker: "mcp".into(),
                    entry_style: String::new(),
                }],
                capabilities: CapabilityPolicy {
                    max_tier: "deploy".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            source: "沙箱".into(),
            from_user_dir: false,
            used_capabilities: vec![],
        };
        let agents = vec![AgentTarget {
            id: "sandbox-codex".into(),
            name: "沙箱 Codex".into(),
            status: "installed".into(),
            installed: true,
            ..Default::default()
        }];
        let resources = vec![McpResource {
            name: "fs-tools".into(),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
            env: vec![],
            enabled: true,
            ..Default::default()
        }];
        let no_agents: Vec<String> = vec![];

        let applied = crate::sync::apply_sync(&SyncRequest {
            defs: &[def.clone()],
            agents: &agents,
            resources: &resources,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        check!(applied.ok, "托管块写入: {}", applied.summary);

        let text = std::fs::read_to_string(&toml_file).unwrap();
        check!(
            text.contains("model = \"gpt-5\"") && text.contains("[user_section]"),
            "原有内容一字未动"
        );
        check!(
            text.contains("# BEGIN AgentHub managed:mcp") && text.contains("[mcp_servers.fs-tools]"),
            "托管块已生成且包含 TOML 表格"
        );

        // 再写一次：托管块应就地替换而不是重复堆叠
        let mut res2 = resources[0].clone();
        res2.args.push("--read-only".into());
        let applied2 = crate::sync::apply_sync(&SyncRequest {
            defs: &[def],
            agents: &agents,
            resources: &[res2],
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        let text2 = std::fs::read_to_string(&toml_file).unwrap();
        check!(
            applied2.ok
                && text2.matches("# BEGIN AgentHub managed:mcp").count() == 1
                && text2.contains("--read-only"),
            "重复写入就地替换（标记出现 {} 次）",
            text2.matches("# BEGIN AgentHub managed:mcp").count()
        );
        check!(
            text2.contains("[user_section]"),
            "用户段落依然存在"
        );
    }

    println!("[15] 密钥保险库（DPAPI）");
    {
        let vault_path = default_data_dir().join("selftest-vault.json");
        let _ = std::fs::remove_file(&vault_path);
        let vault = crate::vault::Vault::open(&vault_path);

        let secret = "sk-test-0123456789abcdefghijklmnopqrstuvwxyz";
        match vault.set("provider:selftest:key", secret) {
            Ok(_) => {
                check!(true, "写入密钥成功");
                check!(
                    vault.get("provider:selftest:key").as_deref() == Some(secret),
                    "解密后与原文一致（往返正确）"
                );
                let masked = vault.masked("provider:selftest:key").unwrap_or_default();
                check!(
                    !masked.contains("0123456789abcdef") && masked.contains('•'),
                    "掩码展示不泄露明文：{}",
                    masked
                );
                // 落盘内容必须不是明文
                let raw = std::fs::read_to_string(&vault_path).unwrap_or_default();
                check!(
                    !raw.contains(secret) && !raw.contains("sk-test"),
                    "落盘文件不含明文（{} 字节）",
                    raw.len()
                );
                // 用同一用户不可解密被篡改的密文
                let tampered = crate::vault::Vault::open(&vault_path);
                let _ = tampered.set("provider:selftest:key2", "another-secret");
                let ids = tampered.ids();
                check!(ids.len() == 2, "保险库可存放多个密钥（{} 个）", ids.len());
                check!(
                    tampered.verify().is_ok(),
                    "自检：全部密钥均可解密"
                );
                let _ = tampered.remove("provider:selftest:key2");
                check!(
                    tampered.ids().len() == 1 && !tampered.has("provider:selftest:key2"),
                    "删除密钥生效"
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ❌ 写入密钥失败: {}", e);
            }
        }
        let _ = std::fs::remove_file(&vault_path);
    }

    println!("[16] 供应商分发：密钥从保险库注入 + diff 掩码");
    {
        use crate::agentdef::{AgentFile, AgentMeta, CapabilityPolicy, LoadedDef};
        use crate::model::{AgentTarget, ProviderEntry, ProviderResource, ProviderWrite};
        use crate::sync::ProviderSyncRequest;

        let dir = base.join("provider");
        std::fs::create_dir_all(&dir).unwrap();
        let settings_file = dir.join("settings.json");
        std::fs::write(
            &settings_file,
            "{\n  \"model\": \"claude-sonnet\",\n  \"env\": {\n    \"MY_OWN_VAR\": \"keep-me\"\n  },\n  \"permissions\": {\n    \"allow\": [\"Read\"]\n  }\n}\n",
        )
        .unwrap();

        let store = crate::store::Store::open(&default_data_dir().join("selftest-sync.db")).unwrap();
        let def = LoadedDef {
            file: AgentFile {
                agent: AgentMeta {
                    id: "sandbox-claude".into(),
                    name: "沙箱 Claude".into(),
                    kind: "cli".into(),
                    ..Default::default()
                },
                provider_write: vec![ProviderWrite {
                    file: settings_file.to_string_lossy().to_string(),
                    root: "env".into(),
                    format: "json".into(),
                    strategy: String::new(),
                    object_per_provider: false,
                    entries: vec![
                        ProviderEntry { key: "ANTHROPIC_BASE_URL".into(), from: "baseUrl".into() },
                        ProviderEntry { key: "ANTHROPIC_API_KEY".into(), from: "apiKey".into() },
                    ],
                }],
                capabilities: CapabilityPolicy {
                    max_tier: "deploy".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            source: "沙箱".into(),
            from_user_dir: false,
            used_capabilities: vec![],
        };
        let agents = vec![AgentTarget {
            id: "sandbox-claude".into(),
            name: "沙箱 Claude".into(),
            status: "installed".into(),
            installed: true,
            ..Default::default()
        }];
        let secret = "sk-ant-sandbox-SECRET-abcdefghijklmnop";
        let providers = vec![ProviderResource {
            id: 0,
            name: "sandbox-provider".into(),
            kind: "anthropic".into(),
            base_url: "https://api.example.com".into(),
            models: vec!["claude-sonnet".into()],
            key_ref: "provider:sandbox-provider".into(),
            has_key: true,
            masked_key: None,
            enabled: true,
            notes: String::new(),
            health: Default::default(),
        }];
        let no_agents: Vec<String> = vec![];
        let resolve = |key_ref: &str| {
            if key_ref == "provider:sandbox-provider" {
                Some(secret.to_string())
            } else {
                None
            }
        };

        let plan = crate::sync::plan_provider_sync(&ProviderSyncRequest {
            defs: &[def.clone()],
            agents: &agents,
            providers: &providers,
            resolve_key: &resolve,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        })
        .expect("应能生成供应商分发计划");
        check!(
            plan.targets[0].supported && plan.targets[0].added == 2,
            "计划：{} 个目标，新增 {} 项",
            plan.targets.len(),
            plan.targets[0].added
        );
        check!(
            !plan.targets[0].diff.contains(secret)
                && plan.targets[0]
                    .changes
                    .iter()
                    .all(|c| !c.detail.contains(secret)),
            "计划中的 diff 与变更详情均已掩码（不含明文密钥）"
        );
        check!(
            plan.warnings.iter().any(|w| w.contains("密钥明文")),
            "计划中明确提示会写入密钥明文"
        );

        let applied = crate::sync::apply_provider_sync(&ProviderSyncRequest {
            defs: &[def],
            agents: &agents,
            providers: &providers,
            resolve_key: &resolve,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        });
        check!(applied.ok, "写入完成: {}", applied.summary);
        let text = std::fs::read_to_string(&settings_file).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        check!(
            json["env"]["ANTHROPIC_BASE_URL"] == "https://api.example.com"
                && json["env"]["ANTHROPIC_API_KEY"] == secret,
            "密钥与 Base URL 已写入 env 节点"
        );
        check!(
            json["env"]["MY_OWN_VAR"] == "keep-me" && json["permissions"]["allow"][0] == "Read",
            "用户原有的 env 变量与其它段落完整保留"
        );
        check!(
            store.backup_list(5).iter().any(|b| b.target == settings_file.to_string_lossy()),
            "写入前已备份目标文件"
        );
    }

    println!("[17] 数据库迁移：M0 形状的老库缺列时仍能写入");
    {
        // 复现线上问题：老库的 mcp_server 没有 enabled / notes / created_at / updated_at，
        // 而写入语句会用到它们 —— 必须靠迁移补齐，否则「从扫描导入」会静默失败
        let legacy = default_data_dir().join("selftest-legacy.db");
        let _ = std::fs::remove_file(&legacy);
        {
            let conn = rusqlite::Connection::open(&legacy).unwrap();
            conn.execute_batch(
                "CREATE TABLE mcp_server (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                    transport TEXT NOT NULL DEFAULT 'stdio', command TEXT,
                    args_json TEXT NOT NULL DEFAULT '[]', env_json TEXT NOT NULL DEFAULT '{}',
                    url TEXT, package_ref TEXT, health TEXT NOT NULL DEFAULT 'unknown');
                 CREATE TABLE provider (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, kind TEXT NOT NULL,
                    base_url TEXT, key_ref TEXT, models_json TEXT NOT NULL DEFAULT '[]',
                    health TEXT NOT NULL DEFAULT 'unknown',
                    created_at TEXT NOT NULL, updated_at TEXT NOT NULL);",
            )
            .unwrap();
        }
        let store = crate::store::Store::open(&legacy).unwrap();
        let mcp = crate::model::McpResource {
            id: 0,
            name: "legacy-import".into(),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "server".into()],
            env: vec![crate::model::EnvPair {
                key: "TOKEN".into(),
                value: "%MY_TOKEN%".into(),
            }],
            url: String::new(),
            enabled: true,
            notes: "从扫描导入".into(),
            health: Default::default(),
        };
        match store.mcp_upsert(&mcp) {
            Ok(id) => {
                let list = store.mcp_list();
                check!(
                    id > 0 && list.len() == 1 && list[0].name == "legacy-import",
                    "老库迁移后可写入并读回 MCP（{} 项，含 {} 个环境变量）",
                    list.len(),
                    list[0].env.len()
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ❌ 老库写入 MCP 失败: {}", e);
            }
        }
        let provider = crate::model::ProviderResource {
            id: 0,
            name: "legacy-provider".into(),
            kind: "openai-compatible".into(),
            base_url: "https://example.com/v1".into(),
            models: vec!["m1".into()],
            key_ref: "provider:legacy-provider".into(),
            has_key: false,
            masked_key: None,
            enabled: true,
            notes: String::new(),
            health: Default::default(),
        };
        match store.provider_upsert(&provider) {
            Ok(_) => check!(
                store.provider_list().len() == 1,
                "老库迁移后可写入并读回 Provider"
            ),
            Err(e) => {
                fail += 1;
                println!("  ❌ 老库写入 Provider 失败: {}", e);
            }
        }
        let _ = std::fs::remove_file(&legacy);
    }

    println!("[18] 环境档案应用：Skill 部署（链接方式）");
    {
        use crate::agentdef::{AgentFile, AgentMeta, CapabilityPolicy, LoadedDef, PathRule};
        use crate::model::{AgentTarget, ProfileCounts, ProfileItem, ProfileResource};
        use crate::profile::{ProfileApplyRequest, ProfileSkill};

        let dir = base.join("profile");
        let source_root = dir.join("library");
        let agent_skills = dir.join("agent-skills");
        std::fs::create_dir_all(&source_root).unwrap();

        // 技能库里两个真实 Skill
        let mut skills: Vec<ProfileSkill> = Vec::new();
        for name in ["alpha", "beta"] {
            let p = source_root.join(name);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(
                p.join("SKILL.md"),
                format!("---\nname: {}\ndescription: 档案测试\n---\n", name),
            )
            .unwrap();
            skills.push(ProfileSkill {
                path: p.to_string_lossy().to_string(),
                name: name.to_string(),
            });
        }

        let def = LoadedDef {
            file: AgentFile {
                agent: AgentMeta {
                    id: "sandbox-profile".into(),
                    name: "沙箱档案目标".into(),
                    kind: "cli".into(),
                    ..Default::default()
                },
                paths: vec![PathRule {
                    label: "Skills 目录".into(),
                    path: agent_skills.to_string_lossy().to_string(),
                    role: "skills".into(),
                    format: String::new(),
                    deploy: "link".into(),
                }],
                capabilities: CapabilityPolicy {
                    max_tier: "deploy".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            source: "沙箱".into(),
            from_user_dir: false,
            used_capabilities: vec![],
        };
        let agents = vec![AgentTarget {
            id: "sandbox-profile".into(),
            name: "沙箱档案目标".into(),
            status: "installed".into(),
            installed: true,
            ..Default::default()
        }];

        // 档案数据结构本身也要能落库
        let store = crate::store::Store::open(&default_data_dir().join("selftest-profile.db")).unwrap();
        let profile = ProfileResource {
            id: 0,
            name: "沙箱档案".into(),
            description: "自检用".into(),
            agents: vec!["sandbox-profile".into()],
            counts: ProfileCounts::default(),
            updated_at: String::new(),
        };
        let items = vec![
            ProfileItem {
                id: 0,
                resource_type: "skill".into(),
                resource_ref: skills[0].path.clone(),
                display: "alpha".into(),
            },
            ProfileItem {
                id: 0,
                resource_type: "skill".into(),
                resource_ref: skills[1].path.clone(),
                display: "beta".into(),
            },
        ];
        match store.profile_save(&profile, &items) {
            Ok(id) => {
                let list = store.profile_list();
                let detail = store.profile_detail(id);
                check!(
                    list.len() == 1
                        && list[0].counts.skill == 2
                        && list[0].agents == vec!["sandbox-profile".to_string()]
                        && detail.map(|d| d.items.len()).unwrap_or(0) == 2,
                    "档案落库：{} 个档案 / {} 个 Skill 项 / 绑定 {} 个 Agent",
                    list.len(),
                    list[0].counts.skill,
                    list[0].agents.len()
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ❌ 档案保存失败: {}", e);
            }
        }

        let no_agents: Vec<String> = vec![];
        let resolve = |_: &str| None;
        let request = |skills: &'static [ProfileSkill]| ProfileApplyRequest {
            defs: std::slice::from_ref(&def),
            agents: &agents,
            mcp_resources: &[],
            providers: &[],
            skills,
            resolve_key: &resolve,
            agent_ids: &no_agents,
            overwrite_unmanaged: false,
            store: &store,
        };

        // 计划
        let leaked: &'static [ProfileSkill] = Box::leak(skills.clone().into_boxed_slice());
        match crate::profile::plan_profile_apply(&request(leaked)) {
            Ok(plan) => check!(
                plan.targets.len() == 1
                    && plan.targets[0].kind == "skill"
                    && plan.targets[0].added == 2,
                "计划：{} 个目标（类型 {}），新增 {} 项 Skill",
                plan.targets.len(),
                plan.targets[0].kind,
                plan.targets[0].added
            ),
            Err(e) => {
                fail += 1;
                println!("  ❌ 生成计划失败: {}", e);
            }
        }

        // 执行
        let applied = crate::profile::apply_profile_apply(&request(leaked));
        check!(applied.ok, "应用完成: {}", applied.summary);
        check!(
            agent_skills.join("alpha").join("SKILL.md").is_file()
                && agent_skills.join("beta").join("SKILL.md").is_file(),
            "两个 Skill 已通过链接部署并可读"
        );
        check!(
            crate::actions::is_link(&agent_skills.join("alpha")),
            "部署方式是链接（junction），不占额外空间"
        );
        check!(
            applied.manifest.is_some(),
            "已记录可撤销清单"
        );

        // 幂等
        let again = crate::profile::plan_profile_apply(&request(leaked)).unwrap();
        check!(
            again.targets[0].unchanged == 2 && again.targets[0].added == 0,
            "幂等：重复应用识别为 {} 项未变、0 项新增",
            again.targets[0].unchanged
        );

        // 已存在真实目录时跳过
        let manual = agent_skills.join("manual-skill");
        std::fs::create_dir_all(&manual).unwrap();
        std::fs::write(manual.join("SKILL.md"), "---\nname: manual-skill\n---\n").unwrap();
        let manual_skill: &'static [ProfileSkill] = Box::leak(
            vec![ProfileSkill {
                path: manual.to_string_lossy().to_string(),
                name: "manual-skill".to_string(),
            }]
            .into_boxed_slice(),
        );
        let plan3 = crate::profile::plan_profile_apply(&request(manual_skill)).unwrap();
        check!(
            plan3.targets[0].skipped == 1 && plan3.targets[0].added == 0,
            "目标已有真实目录时默认跳过（skipped={}）",
            plan3.targets[0].skipped
        );
        let _ = std::fs::remove_file(default_data_dir().join("selftest-profile.db"));
    }

    println!("[19] 供应商连通性测试 net.provider.probe（本地 mock 服务器）");
    {
        use crate::probe::test_provider;
        use std::net::TcpListener;

        // 1) OpenAI 兼容 + 端点兜底：/models 404 → /v1/models 200
        fn openai_router(path: &str) -> (u16, String) {
            if path == "/v1/models" {
                (200, r#"{"data":[{"id":"m1"},{"id":"m2"},{"id":"m3"}]}"#.into())
            } else {
                (404, r#"{"error":"not found"}"#.into())
            }
        }
        let base1 = spawn_mock_server(openai_router, 3);
        let r = test_provider(&base1, "openai-compatible", None, "");
        check!(
            r.status == "ok" && r.http_status == Some(200) && r.models == Some(3),
            "OpenAI 兼容：404 后兜底 /v1/models → ok（HTTP {:?}，{} 个模型，{} ms）",
            r.http_status,
            r.models.unwrap_or(0),
            r.latency_ms
        );
        check!(
            r.endpoint.ends_with("/v1/models"),
            "实际端点为兜底地址：{}",
            r.endpoint
        );

        // 2) 未保存 Key：401 → no_key（这是可用的信号，不是失败）
        fn unauthorized(_path: &str) -> (u16, String) {
            (401, r#"{"error":"missing api key"}"#.into())
        }
        let base2 = spawn_mock_server(unauthorized, 3);
        let r = test_provider(&base2, "openai-compatible", None, "");
        check!(
            r.status == "no_key" && r.http_status == Some(401),
            "端点可达但无 Key → no_key（提示：{}）",
            r.message
        );

        // 3) Key 无效：401 → error，且响应体回显的 Key 会被打码
        fn echo_key(_path: &str) -> (u16, String) {
            (401, r#"{"error":"invalid key sk-sandbox-echo-0123456789"}"#.into())
        }
        let base3 = spawn_mock_server(echo_key, 2);
        let r = test_provider(&base3, "openai-compatible", Some("sk-sandbox-echo-0123456789"), "");
        check!(
            r.status == "error" && r.message.contains("***") && !r.message.contains("sk-sandbox-echo-0123456789"),
            "Key 无效 → error，且回显内容已打码（{}）",
            r.message
        );

        // 4) 连接失败（端口已关闭）
        let dead_port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };
        let r = test_provider(&format!("http://127.0.0.1:{}", dead_port), "openai-compatible", None, "");
        check!(
            r.status == "error" && r.message.contains("连接"),
            "端口关闭 → error（{}）",
            r.message
        );

        // 5) Anthropic：base 无 /v1 时端点补 /v1/models
        fn anthropic_router(path: &str) -> (u16, String) {
            if path == "/v1/models" {
                (200, r#"{"data":[]}"#.into())
            } else {
                (404, "{}".into())
            }
        }
        let base5 = spawn_mock_server(anthropic_router, 2);
        let r = test_provider(&base5, "anthropic", None, "");
        check!(
            r.status == "ok" && r.endpoint.ends_with("/v1/models") && r.models == Some(0),
            "Anthropic：端点补 /v1/models → ok（{} 个模型）",
            r.models.unwrap_or(0)
        );

        // 6) Ollama：无协议前缀自动补 http，端点 /api/tags
        fn ollama_router(path: &str) -> (u16, String) {
            if path == "/api/tags" {
                (200, r#"{"models":[{},{}]}"#.into())
            } else {
                (404, "{}".into())
            }
        }
        let base6 = spawn_mock_server(ollama_router, 2);
        let raw_base = base6.trim_start_matches("http://").to_string();
        let r = test_provider(&raw_base, "ollama", None, "");
        check!(
            r.status == "ok" && r.endpoint.ends_with("/api/tags") && r.models == Some(2),
            "Ollama：补协议 + /api/tags → ok（{} 个模型）",
            r.models.unwrap_or(0)
        );

        // 7) 空 Base URL
        let r = test_provider("", "openai-compatible", None, "");
        check!(r.status == "error", "空 Base URL → error（{}）", r.message);

        // mock 线程可能仍在 accept() 等待额外连接：不 join，
        // 让它们随进程退出自然回收（各自都有连接数上限，不会泄漏累积）
    }

    println!("[20] 快照对比：两次扫描的资源级差异");
    {
        use crate::model::{ScanSnapshot, SnapshotMeta};
        use crate::snapdiff::diff_snapshots;

        let mk_agent = |id: &str, status: &str, skills: usize, mcp: usize| crate::model::AgentTarget {
            id: id.into(),
            name: format!("Agent-{}", id),
            status: status.into(),
            installed: status == "installed",
            skill_count: skills,
            mcp_count: mcp,
            ..Default::default()
        };
        let mk_skill = |path: &str, name: &str, bytes: u64, broken: bool| crate::model::SkillFound {
            path: path.into(),
            name: name.into(),
            bytes,
            broken,
            ..Default::default()
        };
        let mk_mcp = |name: &str, agent: &str, agent_id: &str, transport: &str| {
            crate::model::McpServerFound {
                id: format!("{}/{}", agent_id, name),
                name: name.into(),
                source_agent: agent.into(),
                source_agent_id: agent_id.into(),
                transport: transport.into(),
                ..Default::default()
            }
        };

        let mut a = ScanSnapshot {
            scanned_at: "2025-01-01 10:00:00".into(),
            ..Default::default()
        };
        a.agents = vec![mk_agent("ag-a", "installed", 5, 2), mk_agent("ag-b", "configured", 0, 0)];
        a.skills = vec![
            mk_skill("C:/lib/s1", "s1", 100, false),
            mk_skill("C:/lib/s2", "s2", 200, false),
        ];
        a.mcp_servers = vec![mk_mcp("old-mcp", "Agent-ag-a", "ag-a", "stdio")];
        a.python_envs = vec![crate::model::PythonEnv {
            path: "C:/envs/py312".into(),
            name: "py312".into(),
            manager: "uv".into(),
            python_version: Some("3.12.1".into()),
            package_count: Some(10),
            ..Default::default()
        }];
        a.npm_packages = vec![crate::model::NpmPackage {
            name: "pkg-a".into(),
            version: "1.0.0".into(),
            manager: "npm".into(),
            ..Default::default()
        }];
        a.provider_hints = vec![crate::model::ProviderHint {
            id: "h1".into(),
            label: "旧线索".into(),
            ..Default::default()
        }];

        let mut b = ScanSnapshot {
            scanned_at: "2025-01-02 10:00:00".into(),
            ..Default::default()
        };
        b.agents = vec![mk_agent("ag-a", "leftover", 5, 2), mk_agent("ag-b", "configured", 0, 0), mk_agent("ag-c", "installed", 1, 1)];
        b.skills = vec![
            mk_skill("C:/lib/s2", "s2", 500, true),      // 变化：大小 + 失效
            mk_skill("C:/lib/s3", "s3", 300, false),      // 新增
        ]; // s1 移除
        b.mcp_servers = vec![mk_mcp("new-mcp", "Agent-ag-a", "ag-a", "http")]; // old-mcp 移除
        b.python_envs = vec![crate::model::PythonEnv {
            path: "C:/envs/py312".into(),
            name: "py312".into(),
            manager: "uv".into(),
            python_version: Some("3.12.4".into()),
            package_count: Some(12),
            ..Default::default()
        }];
        b.npm_packages = vec![
            crate::model::NpmPackage {
                name: "pkg-a".into(),
                version: "1.1.0".into(),
                manager: "npm".into(),
                ..Default::default()
            },
            crate::model::NpmPackage {
                name: "pkg-b".into(),
                version: "0.9.0".into(),
                manager: "pnpm".into(),
                ..Default::default()
            },
        ];
        b.provider_hints = vec![]; // h1 移除

        let diff = diff_snapshots(&a, &b);
        let sec = |name: &str| diff.sections.iter().find(|s| s.resource == name);
        check!(
            sec("agents").map(|s| s.changed.len() == 1 && s.added.len() == 1).unwrap_or(false),
            "Agent：ag-a 状态变化 ×1、ag-c 新增 ×1"
        );
        let skills = sec("skills").expect("skills section");
        check!(
            skills.added.len() == 1 && skills.removed.len() == 1 && skills.changed.len() == 1,
            "Skill：新增 {} / 移除 {} / 变化 {}",
            skills.added.len(),
            skills.removed.len(),
            skills.changed.len()
        );
        check!(
            skills
                .changed
                .iter()
                .any(|c| c.detail.contains("链接已失效")),
            "Skill 变化细节标注失效：{}",
            skills.changed[0].detail
        );
        let mcp = sec("mcp").expect("mcp section");
        check!(
            mcp.added.len() == 1 && mcp.removed.len() == 1,
            "MCP：新增 {} / 移除 {}",
            mcp.added.len(),
            mcp.removed.len()
        );
        let npm = sec("npm").expect("npm section");
        check!(
            npm.changed
                .iter()
                .any(|c| c.detail.contains("1.0.0 → 1.1.0")),
            "npm 版本变化：{}",
            npm.changed[0].detail
        );
        check!(
            sec("providers").map(|s| s.removed.len() == 1).unwrap_or(false)
                && sec("python").map(|s| s.changed.len() == 1).unwrap_or(false),
            "供应商线索移除 ×1；Python 版本变化 ×1"
        );
        check!(
            diff.summary.contains("新增 4"),
            "汇总：{}",
            diff.summary
        );

        // 相同快照 → 无差异
        let same = diff_snapshots(&a, &a);
        check!(
            same.sections.is_empty() && same.summary.contains("没有"),
            "相同快照 → 无差异（{}）",
            same.summary
        );

        // store 往返：save → snapshot_by_id → diff
        let store =
            crate::store::Store::open(&base.join("selftest-snapdiff.db")).unwrap();
        let id_a = store.save_snapshot(&a).unwrap();
        let id_b = store.save_snapshot(&b).unwrap();
        let loaded_a = store.snapshot_by_id(id_a).unwrap();
        let loaded_b = store.snapshot_by_id(id_b).unwrap();
        check!(
            diff_snapshots(&loaded_a, &loaded_b).summary == diff.summary,
            "快照落库 → 按编号读回 → 差异一致"
        );
        let metas: Vec<SnapshotMeta> = store.snapshot_history(10);
        check!(
            metas.len() == 2 && metas[0].id == id_b,
            "快照历史按时间倒序（最新在前）"
        );
        let _ = std::fs::remove_file(base.join("selftest-snapdiff.db"));
    }

    println!("[21] 档案导出 / 导入（自包含 JSON，不含任何密钥）");
    {
        use crate::model::{ProfileCounts, ProfileItem, ProfileResource};
        use crate::share;

        let store = crate::store::Store::open(&base.join("selftest-share.db")).unwrap();
        let profile = ProfileResource {
            id: 0,
            name: "分享档案".into(),
            description: "导出导入自检".into(),
            agents: vec![],
            counts: ProfileCounts::default(),
            updated_at: String::new(),
        };
        let items = vec![
            ProfileItem {
                id: 0,
                resource_type: "skill".into(),
                resource_ref: "C:/lib/alpha".into(),
                display: "alpha".into(),
            },
            ProfileItem {
                id: 0,
                resource_type: "mcp".into(),
                resource_ref: "filesystem".into(),
                display: "filesystem".into(),
            },
        ];
        let pid = store.profile_save(&profile, &items).unwrap();

        let outcome = share::export_profile(&store, pid).unwrap();
        check!(
            std::path::Path::new(&outcome.path).is_file() && outcome.items == 2,
            "导出成功：{}（{} 项资源）",
            outcome.path,
            outcome.items
        );
        let text = std::fs::read_to_string(&outcome.path).unwrap();
        check!(
            text.contains("\"format\": \"agenthub-profile\"") && !text.contains("keyRef"),
            "导出文件带 format 标记，且不含任何 keyRef/密钥字段"
        );

        // 同库导入：同名 → 自动后缀，不覆盖
        let imported = share::import_profile(&store, std::path::Path::new(&outcome.path)).unwrap();
        check!(
            imported.name == "分享档案（导入）",
            "同名档案导入自动加后缀：{}",
            imported.name
        );
        let detail = store.profile_detail(imported.id).unwrap();
        check!(
            detail.items.len() == 2
                && detail.items.iter().any(|i| i.resource_type == "mcp")
                && detail.items.iter().any(|i| i.resource_type == "skill"),
            "导入项完整（{} 项，skill 与 mcp 各就位）",
            detail.items.len()
        );

        // 全新库导入：保留原名
        let fresh = crate::store::Store::open(&base.join("selftest-share-fresh.db")).unwrap();
        let r2 = share::import_profile(&fresh, std::path::Path::new(&outcome.path)).unwrap();
        check!(
            r2.name == "分享档案" && fresh.profile_list().len() == 1,
            "全新库导入保留原名"
        );

        // 非法文件 / 不存在文件（坏文件放在 exports 目录里，列表应跳过它）
        let bad = share::exports_dir(&store).join("bad.agenthub-profile.json");
        std::fs::write(&bad, "{}").unwrap();
        let err = share::import_profile(&store, &bad).unwrap_err();
        check!(err.contains("format"), "非法文件被拒绝：{}", err);
        let err2 = share::import_profile(&store, std::path::Path::new("Z:/不存在.json"))
            .unwrap_err();
        check!(!err2.is_empty(), "不存在的文件报错：{}", err2);

        // 导出目录列表
        let (metas, failed) = share::list_exports(&store);
        check!(
            metas.len() == 1 && metas[0].items == 2 && failed == 1,
            "导出目录列表：{} 个有效文件（含 skill/mcp 分项计数），{} 个无效文件被忽略",
            metas.len(),
            failed
        );
        let _ = std::fs::remove_file(base.join("selftest-share.db"));
        let _ = std::fs::remove_file(base.join("selftest-share-fresh.db"));
    }

    println!("[22] path.glob 能力与目录表登记");
    {
        use crate::capability::{catalog, invoke, EvalContext, RuleParams};
        use crate::model::AppSettings;

        let root = base.join("glob");
        for rel in ["a/SKILL.md", "b/SKILL.md", "c/d/SKILL.md", "c/notes.txt"] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "x").unwrap();
        }
        let settings = AppSettings::default();
        let ctx = EvalContext::new(&[], &settings);
        let params = RuleParams {
            paths: vec![root.to_string_lossy().to_string()],
            args: vec!["**/SKILL.md".into()],
            ..Default::default()
        };
        let out = invoke("path.glob", &params, &ctx);
        check!(
            out.hit && out.detail.as_deref().unwrap_or("").contains("3 个条目"),
            "**/SKILL.md 命中 3 个（{}）",
            out.detail.as_deref().unwrap_or("—")
        );
        let flat = invoke(
            "path.glob",
            &RuleParams {
                paths: vec![root.to_string_lossy().to_string()],
                args: vec!["*.md".into()],
                ..Default::default()
            },
            &ctx,
        );
        check!(!flat.hit, "*.md 不跨层匹配（顶层没有 md 文件）");
        let sub = invoke(
            "path.glob",
            &RuleParams {
                paths: vec![root.to_string_lossy().to_string()],
                args: vec!["c/*.txt".into()],
                ..Default::default()
            },
            &ctx,
        );
        check!(sub.hit, "c/*.txt 命中 notes.txt");

        // glob 匹配器单元行为
        check!(
            crate::util::glob_match("a/*/c", "a/b/c") && !crate::util::glob_match("*.md", "b/c.md"),
            "glob 匹配器：段内通配不跨层"
        );
        check!(
            crate::util::glob_match("a/**/*.md", "a/b/c/d.md")
                && crate::util::glob_match("**", "任意/路径"),
            "glob 匹配器：** 跨任意层"
        );

        // 能力目录登记
        let caps = catalog();
        let probe = caps.iter().find(|c| c.id == "net.provider.probe");
        check!(
            probe.map(|c| c.implemented && c.tier_code == "T3").unwrap_or(false),
            "net.provider.probe 已登记为 T3 且已实现"
        );
        check!(
            caps.iter()
                .find(|c| c.id == "path.glob")
                .map(|c| c.implemented)
                .unwrap_or(false),
            "path.glob 已实现"
        );
        check!(
            caps.len() == 30,
            "能力目录共 {} 项（期望 30）",
            caps.len()
        );
    }

    println!("[23] MCP 握手健康检查 proc.spawn.probe（stdio + http）");
    {
        use crate::handshake::handshake;
        use crate::model::{EnvPair, McpResource};

        let mk = |transport: &str, command: &str, args: Vec<String>, url: &str, env: Vec<EnvPair>| {
            McpResource {
                id: 0,
                name: "sandbox-mcp".into(),
                transport: transport.into(),
                command: command.into(),
                args,
                env,
                url: url.into(),
                enabled: true,
                notes: String::new(),
                health: Default::default(),
            }
        };

        // 1) stdio：node 模拟一个真正会说 JSON-RPC 的 MCP 服务器
        let node = crate::util::resolve_program("node", &[]);
        let mock_script = r#"const rl=require('readline').createInterface({input:process.stdin});
rl.on('line',l=>{let m;try{m=JSON.parse(l)}catch(e){return}
if(m.id===1){process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:1,result:{protocolVersion:'2024-11-05',capabilities:{},serverInfo:{name:'mock-'+(process.env.SELFTEST_TOKEN||'node'),version:'1.0'}}})+'\n')}
else if(m.id===2){process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:2,result:{tools:[{name:'echo'},{name:'ping'}]}})+'\n')}})"#;
        match &node {
            Some(node) => {
                std::env::set_var("AGENTHUB_SELFTEST_TOKEN", "expanded-ok");
                let res = mk(
                    "stdio",
                    &node.to_string_lossy(),
                    vec!["-e".into(), mock_script.into()],
                    "",
                    vec![EnvPair {
                        key: "SELFTEST_TOKEN".into(),
                        value: "%AGENTHUB_SELFTEST_TOKEN%".into(),
                    }],
                );
                let r = handshake(&res, "");
                check!(
                    r.status == "ok"
                        && r.protocol_version.as_deref() == Some("2024-11-05")
                        && r.tools == Some(2),
                    "stdio 握手：initialize + tools/list（{}，{} 个工具）",
                    r.message,
                    r.tools.unwrap_or(0)
                );
                check!(
                    r.server_name.as_deref() == Some("mock-expanded-ok"),
                    "环境变量 %VAR% 引用在传给进程前展开（serverInfo.name = {:?}）",
                    r.server_name
                );
                check!(r.latency_ms > 0, "记录了启动到握手的耗时（{} ms）", r.latency_ms);

                // 2) 进程活着但不说 JSON-RPC → timeout（而不是把进程挂死）
                let silent = mk(
                    "stdio",
                    &node.to_string_lossy(),
                    vec!["-e".into(), "setInterval(function(){},1000)".into()],
                    "",
                    vec![],
                );
                let r = handshake(&silent, "");
                check!(r.status == "timeout", "静默进程 → timeout（{}）", r.message);
            }
            None => {
                check!(false, "未找到 node，跳过 stdio 握手（CI 环境应预装 node）");
            }
        }

        // 3) 命令不存在 → 明确报「找不到命令」
        let r = handshake(&mk("stdio", "agenthub-no-such-cmd-xyz", vec![], "", vec![]), "");
        check!(
            r.status == "error" && r.message.contains("找不到命令"),
            "命令不存在 → error（{}）",
            r.message
        );

        // 4) http：Streamable HTTP 风格的 initialize 响应
        fn mcp_http_router(_path: &str) -> (u16, String) {
            (
                200,
                r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","serverInfo":{"name":"mock-http","version":"0.1"}}}"#.into(),
            )
        }
        let base = spawn_mock_server(mcp_http_router, 2);
        let r = handshake(&mk("http", "", vec![], &base, vec![]), "");
        check!(
            r.status == "ok"
                && r.protocol_version.as_deref() == Some("2025-03-26")
                && r.server_name.as_deref() == Some("mock-http"),
            "http 握手：{}",
            r.message
        );

        // 5) http 端口关闭 → error
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };
        let r = handshake(
            &mk("http", "", vec![], &format!("http://127.0.0.1:{}", dead_port), vec![]),
            "",
        );
        check!(r.status == "error", "http 端口关闭 → error（{}）", r.message);

        // 6) 能力目录：proc.spawn.probe 已实现
        check!(
            crate::capability::catalog()
                .iter()
                .find(|c| c.id == "proc.spawn.probe")
                .map(|c| c.implemented)
                .unwrap_or(false),
            "proc.spawn.probe 在能力目录中标记为已实现"
        );
    }

    println!("\n=== 结果：{} 项通过，{} 项失败 ===", pass, fail);
    println!("沙箱残留（可手动删除）: {}", base.display());
    if fail > 0 {
        std::process::exit(1);
    }
}

/// 无界面自检 / CLI 镜像：执行一次完整扫描并把快照 JSON 打到 stdout，进度走 stderr。
///
/// 用法：`agenthub --scan-json > snapshot.json`
pub fn cli_scan() {
    let settings = model::AppSettings::default();
    let data_dir = default_data_dir();
    let host = util::host_info(
        env!("CARGO_PKG_VERSION"),
        &data_dir.to_string_lossy(),
        &data_dir.join("agenthub.db").to_string_lossy(),
    );
    let sink = |p: model::Progress| eprintln!("[{:>6}] {:<14} {}", p.level, p.phase, p.message);
    let reporter = scan::Reporter::new(&sink);
    let snapshot = scan::scan(&settings, &host, &reporter);
    match serde_json::to_string_pretty(&snapshot) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("序列化失败：{}", e),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("agenthub.db");

            let store = store::Store::open(&db_path)?;
            let settings = store.load_settings();
            // 密钥保险库：DPAPI 加密，明文永不落库
            let vault = vault::Vault::open(&data_dir.join("vault.json"));
            if let Err(e) = vault.verify() {
                eprintln!("保险库自检告警：{}", e);
            }

            // 首次运行把内置 Agent 定义导出到数据目录，用户可直接编辑；
            // 已存在的同名文件不会被覆盖。
            let definitions_dir = agentdef::user_dir(&settings);
            if let Err(e) = agentdef::seed_user_dir(&definitions_dir) {
                eprintln!("导出内置 Agent 定义失败：{}", e);
            }

            app.manage(commands::AppState {
                store: Arc::new(store),
                settings: Mutex::new(settings),
                scanning: Arc::new(AtomicBool::new(false)),
                vault: Arc::new(vault),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::reset_onboarding,
            commands::detect_executables,
            commands::run_scan,
            commands::last_snapshot,
            commands::snapshot_history,
            commands::read_text_preview,
            commands::reveal_path,
            commands::list_dir,
            commands::frontend_ready,
            commands::capability_catalog,
            commands::agent_definitions,
            commands::save_agent_definition,
            commands::reset_agent_definition,
            commands::seed_agent_definitions,
            commands::skill_environment,
            commands::skill_discover,
            commands::skill_import_plan,
            commands::skill_import_apply,
            commands::git_clone_repo,
            commands::tmp_cleanup,
            commands::skill_cleanup_plan,
            commands::skill_cleanup_apply,
            commands::skill_relink_plan,
            commands::skill_relink_apply,
            commands::skill_delete_plan,
            commands::skill_delete_apply,
            commands::trash_list,
            commands::trash_restore,
            commands::trash_stats,
            commands::trash_detail,
            commands::trash_restore_items,
            commands::trash_purge_plan,
            commands::trash_purge_apply,
            commands::trash_purge_older_plan,
            commands::trash_purge_older_apply,
            commands::mcp_resources,
            commands::mcp_save,
            commands::mcp_remove,
            commands::mcp_import,
            commands::mcp_test,
            commands::mcp_sync_plan,
            commands::mcp_sync_apply,
            commands::backups_list,
            commands::backup_restore,
            commands::provider_resources,
            commands::provider_save,
            commands::provider_remove,
            commands::provider_import,
            commands::provider_reveal_key,
            commands::vault_status,
            commands::provider_test,
            commands::snapshot_diff,
            commands::profile_export,
            commands::profile_export_list,
            commands::profile_import,
            commands::provider_sync_plan,
            commands::provider_sync_apply,
            commands::profile_list,
            commands::profile_detail,
            commands::profile_save,
            commands::profile_delete,
            commands::profile_apply_plan,
            commands::profile_apply_run,
            commands::manifests_list,
            commands::manifest_restore,
        ])
        .run(tauri::generate_context!())
        .expect("AgentHub 启动失败");
}