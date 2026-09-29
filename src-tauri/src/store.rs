//! SQLite 存储层。
//!
//! M0 实际使用：`settings`（应用设置）与 `scan_snapshot`（扫描历史）。
//! 其余表为 M1/M2 的资源与同步模型预留，schema 已就位，届时无需迁移。

use crate::model::{AppSettings, ScanSnapshot, SnapshotMeta};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scan_snapshot (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    scanned_at    TEXT    NOT NULL,
    duration_ms   INTEGER NOT NULL,
    agents        INTEGER NOT NULL DEFAULT 0,
    skills        INTEGER NOT NULL DEFAULT 0,
    mcp_servers   INTEGER NOT NULL DEFAULT 0,
    python_envs   INTEGER NOT NULL DEFAULT 0,
    npm_packages  INTEGER NOT NULL DEFAULT 0,
    json          TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scan_snapshot_time ON scan_snapshot(scanned_at DESC);

-- ===== 以下为 M1/M2 资源模型预留 =====

CREATE TABLE IF NOT EXISTS provider (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL,
    base_url     TEXT,
    key_ref      TEXT,
    models_json  TEXT NOT NULL DEFAULT '[]',
    health       TEXT NOT NULL DEFAULT 'unknown',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    source_type  TEXT NOT NULL DEFAULT 'local',
    path         TEXT,
    repo_url     TEXT,
    version      TEXT,
    enabled      INTEGER NOT NULL DEFAULT 1,
    meta_json    TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS mcp_server (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    transport    TEXT NOT NULL DEFAULT 'stdio',
    command      TEXT,
    args_json    TEXT NOT NULL DEFAULT '[]',
    env_json     TEXT NOT NULL DEFAULT '[]',
    url          TEXT,
    package_ref  TEXT,
    health       TEXT NOT NULL DEFAULT 'unknown',
    enabled      INTEGER NOT NULL DEFAULT 1,
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL DEFAULT '',
    updated_at   TEXT NOT NULL DEFAULT ''
);

-- 每个 (Agent, 文件, 节点) 上次由 AgentHub 写入的键，用于清理已移除项
CREATE TABLE IF NOT EXISTS sync_state (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id    TEXT NOT NULL,
    file        TEXT NOT NULL,
    root        TEXT NOT NULL,
    keys_json   TEXT NOT NULL DEFAULT '[]',
    applied_at  TEXT NOT NULL,
    UNIQUE(agent_id, file, root)
);

-- 写入前的文件备份索引
CREATE TABLE IF NOT EXISTS backup (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    target       TEXT NOT NULL,
    backup_path  TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    bytes        INTEGER NOT NULL DEFAULT 0,
    note         TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS npm_package (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    version_spec TEXT,
    installed    TEXT,
    scope        TEXT NOT NULL DEFAULT 'global',
    manager      TEXT NOT NULL DEFAULT 'npm'
);

CREATE TABLE IF NOT EXISTS python_env (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    manager      TEXT NOT NULL,
    path         TEXT NOT NULL,
    py_version   TEXT,
    deps_lock    TEXT NOT NULL DEFAULT '{}',
    managed      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS profile (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL UNIQUE,
    description  TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS profile_item (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id     INTEGER NOT NULL REFERENCES profile(id) ON DELETE CASCADE,
    resource_type  TEXT NOT NULL,
    resource_ref   TEXT NOT NULL,
    override_json  TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS agent_target (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id        TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    config_root     TEXT,
    bound_profile   INTEGER REFERENCES profile(id) ON DELETE SET NULL,
    last_sync_hash  TEXT,
    last_synced_at  TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS sync_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    target_id    INTEGER NOT NULL,
    profile_id   INTEGER,
    diff_json    TEXT NOT NULL DEFAULT '{}',
    backup_path  TEXT,
    status       TEXT NOT NULL,
    actor        TEXT NOT NULL DEFAULT 'gui',
    created_at   TEXT NOT NULL
);
"#;

/* ------------------------------------------------------------- 迁移 */

/// 幂等补列：老库升级时 CREATE TABLE IF NOT EXISTS 不会补新列，这里按需 ALTER
fn migrate(conn: &Connection) {
    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        let mut stmt = match conn.prepare(&format!("PRAGMA table_info({})", table)) {
            Ok(s) => s,
            Err(_) => return true, // 表不存在时不折腾
        };
        let found = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map(|rows| rows.flatten().any(|name| name == column))
            .unwrap_or(true);
        found
    }
    let additions: &[(&str, &str, &str)] = &[
        ("provider", "enabled", "enabled INTEGER NOT NULL DEFAULT 1"),
        ("provider", "notes", "notes TEXT NOT NULL DEFAULT ''"),
        ("provider", "balance", "balance TEXT NOT NULL DEFAULT ''"),
        // M0 建的 mcp_server 表缺这四列，而 mcp_upsert 会写它们 ——
        // 漏补会导致「从扫描导入」静默失败（INSERT 报 no such column）
        ("mcp_server", "enabled", "enabled INTEGER NOT NULL DEFAULT 1"),
        ("mcp_server", "notes", "notes TEXT NOT NULL DEFAULT ''"),
        ("mcp_server", "created_at", "created_at TEXT NOT NULL DEFAULT ''"),
        ("mcp_server", "updated_at", "updated_at TEXT NOT NULL DEFAULT ''"),
        ("mcp_server", "headers_json", "headers_json TEXT NOT NULL DEFAULT '[]'"),
    ];
    for (table, column, ddl) in additions {
        if !has_column(conn, table, column) {
            let _ = conn.execute_batch(&format!("ALTER TABLE {} ADD COLUMN {}", table, ddl));
        }
    }
}

pub struct Store {
    conn: Mutex<Connection>,
    data_dir: std::path::PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn);
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            data_dir: path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
        })
    }

    pub fn data_dir(&self) -> std::path::PathBuf {
        self.data_dir.clone()
    }

    /// 诊断用：列出某张表的列名
    pub fn table_columns(&self, table: &str) -> Vec<String> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(&format!("PRAGMA table_info({})", table)) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| row.get::<_, String>(1))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn count_of(&self, table: &str) -> i64 {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| row.get(0))
            .unwrap_or(0)
    }

    /* --------------------------------------------------- MCP 资源库 */

    pub fn mcp_list(&self) -> Vec<crate::model::McpResource> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, name, transport, command, args_json, env_json, url, enabled, notes, health, headers_json
             FROM mcp_server ORDER BY name COLLATE NOCASE",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| {
            let args_json: String = row.get(4)?;
            let env_json: String = row.get(5)?;
            let health_raw: String = row
                .get::<_, Option<String>>(9)?
                .unwrap_or_else(|| "unknown".to_string());
            let headers_raw: String = row
                .get::<_, Option<String>>(10)?
                .unwrap_or_else(|| "[]".to_string());
            Ok(crate::model::McpResource {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                command: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                args: serde_json::from_str(&args_json).unwrap_or_default(),
                env: serde_json::from_str(&env_json).unwrap_or_default(),
                headers: serde_json::from_str(&headers_raw).unwrap_or_default(),
                url: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                enabled: row.get::<_, i64>(7)? != 0,
                notes: row.get(8)?,
                health: serde_json::from_str(&health_raw).unwrap_or_default(),
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 新增或更新（id = 0 视为新增，同名冲突时按 id 更新）
    pub fn mcp_upsert(&self, res: &crate::model::McpResource) -> Result<i64> {
        let conn = self.conn()?;
        let args_json = serde_json::to_string(&res.args)?;
        let env_json = serde_json::to_string(&res.env)?;
        let headers_json = serde_json::to_string(&res.headers)?;
        let now = crate::util::now_human();
        if res.id > 0 {
            conn.execute(
                "UPDATE mcp_server SET name=?1, transport=?2, command=?3, args_json=?4,
                   env_json=?5, url=?6, enabled=?7, notes=?8, updated_at=?9, headers_json=?10 WHERE id=?11",
                params![
                    res.name, res.transport, res.command, args_json, env_json, res.url,
                    res.enabled as i64, res.notes, now, headers_json, res.id
                ],
            )?;
            return Ok(res.id);
        }
        if let Some(existing) = conn
            .query_row(
                "SELECT id FROM mcp_server WHERE name = ?1 COLLATE NOCASE",
                params![res.name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE mcp_server SET transport=?1, command=?2, args_json=?3, env_json=?4,
                   url=?5, enabled=?6, notes=?7, updated_at=?8, headers_json=?9 WHERE id=?10",
                params![
                    res.transport, res.command, args_json, env_json, res.url,
                    res.enabled as i64, res.notes, now, headers_json, existing
                ],
            )?;
            return Ok(existing);
        }
        conn.execute(
            "INSERT INTO mcp_server
               (name, transport, command, args_json, env_json, url, enabled, notes, created_at, updated_at, headers_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10)",
            params![
                res.name, res.transport, res.command, args_json, env_json, res.url,
                res.enabled as i64, res.notes, now, headers_json
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 更新 MCP 健康状态（只由握手检查写入，普通保存不重置）
    pub fn mcp_set_health(&self, id: i64, health_json: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE mcp_server SET health = ?1, updated_at = ?2 WHERE id = ?3",
            params![health_json, crate::util::now_human(), id],
        )?;
        Ok(())
    }

    pub fn mcp_delete(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM mcp_server WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* --------------------------------------------------- Provider 资源 */

    pub fn provider_list(&self) -> Vec<crate::model::ProviderResource> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, name, kind, base_url, key_ref, models_json, enabled, notes, health, balance
             FROM provider ORDER BY name COLLATE NOCASE",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| {
            let models_json: String = row.get(5)?;
            let health_raw: String = row
                .get::<_, Option<String>>(8)?
                .unwrap_or_else(|| "unknown".to_string());
            let balance_raw: String = row
                .get::<_, Option<String>>(9)?
                .unwrap_or_default();
            Ok(crate::model::ProviderResource {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                base_url: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                key_ref: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                models: serde_json::from_str(&models_json).unwrap_or_default(),
                enabled: row.get::<_, i64>(6)? != 0,
                notes: row.get(7)?,
                has_key: false,
                masked_key: None,
                // 老值 'unknown' 解析失败 → 默认（未测试）
                health: serde_json::from_str(&health_raw).unwrap_or_default(),
                balance: serde_json::from_str(&balance_raw).unwrap_or_default(),
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn provider_upsert(&self, res: &crate::model::ProviderResource) -> Result<i64> {
        let conn = self.conn()?;
        let models_json = serde_json::to_string(&res.models)?;
        let now = crate::util::now_human();
        if res.id > 0 {
            conn.execute(
                "UPDATE provider SET name=?1, kind=?2, base_url=?3, key_ref=?4, models_json=?5,
                   enabled=?6, notes=?7, updated_at=?8 WHERE id=?9",
                params![
                    res.name, res.kind, res.base_url, res.key_ref, models_json,
                    res.enabled as i64, res.notes, now, res.id
                ],
            )?;
            return Ok(res.id);
        }
        if let Some(existing) = conn
            .query_row(
                "SELECT id FROM provider WHERE name = ?1 COLLATE NOCASE",
                params![res.name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE provider SET kind=?1, base_url=?2, key_ref=?3, models_json=?4,
                   enabled=?5, notes=?6, updated_at=?7 WHERE id=?8",
                params![
                    res.kind, res.base_url, res.key_ref, models_json,
                    res.enabled as i64, res.notes, now, existing
                ],
            )?;
            return Ok(existing);
        }
        conn.execute(
            "INSERT INTO provider
               (name, kind, base_url, key_ref, models_json, health, created_at, updated_at, enabled, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, 'unknown', ?6, ?6, ?7, ?8)",
            params![
                res.name, res.kind, res.base_url, res.key_ref, models_json, now,
                res.enabled as i64, res.notes
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 更新供应商健康状态（只由连通性测试写入，普通保存不重置）
    pub fn provider_set_health(&self, id: i64, health_json: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE provider SET health = ?1, updated_at = ?2 WHERE id = ?3",
            params![health_json, crate::util::now_human(), id],
        )?;
        Ok(())
    }

    /// 更新供应商余额状态（只由余额查询写入，普通保存不重置）
    pub fn provider_set_balance(&self, id: i64, balance_json: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE provider SET balance = ?1, updated_at = ?2 WHERE id = ?3",
            params![balance_json, crate::util::now_human(), id],
        )?;
        Ok(())
    }

    pub fn provider_delete(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM provider WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------------ 同步状态 */

    pub fn sync_state_get(&self, agent_id: &str, file: &str, root: &str) -> Vec<String> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        conn.query_row(
            "SELECT keys_json FROM sync_state WHERE agent_id=?1 AND file=?2 AND root=?3",
            params![agent_id, file, root],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
    }

    pub fn sync_state_set(
        &self,
        agent_id: &str,
        file: &str,
        root: &str,
        keys: &[String],
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO sync_state (agent_id, file, root, keys_json, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(agent_id, file, root)
             DO UPDATE SET keys_json = excluded.keys_json, applied_at = excluded.applied_at",
            params![
                agent_id,
                file,
                root,
                serde_json::to_string(keys)?,
                crate::util::now_human()
            ],
        )?;
        Ok(())
    }

    /* -------------------------------------------------------- Profile 档案 */

    fn profile_items(&self, conn: &Connection, profile_id: i64) -> Vec<crate::model::ProfileItem> {
        let mut stmt = match conn.prepare(
            "SELECT id, resource_type, resource_ref FROM profile_item
             WHERE profile_id = ?1 ORDER BY resource_type, resource_ref COLLATE NOCASE",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![profile_id], |row| {
            let resource_type: String = row.get(1)?;
            let resource_ref: String = row.get(2)?;
            // Skill 用路径存储，展示时取末段目录名
            let display = if resource_type == "skill" {
                std::path::Path::new(&resource_ref)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| resource_ref.clone())
            } else {
                resource_ref.clone()
            };
            Ok(crate::model::ProfileItem {
                id: row.get(0)?,
                resource_type,
                resource_ref,
                display,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    fn profile_agents(&self, conn: &Connection, profile_id: i64) -> Vec<String> {
        let mut stmt = match conn.prepare(
            "SELECT agent_id FROM agent_target WHERE bound_profile = ?1 ORDER BY agent_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![profile_id], |row| row.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn profile_list(&self) -> Vec<crate::model::ProfileResource> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, name, description, updated_at FROM profile ORDER BY name COLLATE NOCASE",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows: Vec<(i64, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map(|it| it.flatten().collect())
            .unwrap_or_default();

        rows.into_iter()
            .map(|(id, name, description, updated_at)| {
                let items = self.profile_items(&conn, id);
                let counts = crate::model::ProfileCounts {
                    mcp: items.iter().filter(|i| i.resource_type == "mcp").count(),
                    provider: items.iter().filter(|i| i.resource_type == "provider").count(),
                    skill: items.iter().filter(|i| i.resource_type == "skill").count(),
                };
                crate::model::ProfileResource {
                    id,
                    name,
                    description,
                    agents: self.profile_agents(&conn, id),
                    counts,
                    updated_at,
                }
            })
            .collect()
    }

    pub fn profile_detail(&self, id: i64) -> Option<crate::model::ProfileDetail> {
        let profile = self.profile_list().into_iter().find(|p| p.id == id)?;
        let conn = self.conn.lock().ok()?;
        Some(crate::model::ProfileDetail {
            items: self.profile_items(&conn, id),
            profile,
        })
    }

    /// 保存档案（含资源项与 Agent 绑定）。返回档案 id。
    pub fn profile_save(
        &self,
        profile: &crate::model::ProfileResource,
        items: &[crate::model::ProfileItem],
    ) -> Result<i64> {
        let conn = self.conn()?;
        let now = crate::util::now_human();
        let id = if profile.id > 0 {
            conn.execute(
                "UPDATE profile SET name=?1, description=?2, updated_at=?3 WHERE id=?4",
                params![profile.name, profile.description, now, profile.id],
            )?;
            profile.id
        } else if let Some(existing) = conn
            .query_row(
                "SELECT id FROM profile WHERE name = ?1 COLLATE NOCASE",
                params![profile.name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE profile SET description=?1, updated_at=?2 WHERE id=?3",
                params![profile.description, now, existing],
            )?;
            existing
        } else {
            conn.execute(
                "INSERT INTO profile (name, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)",
                params![profile.name, profile.description, now],
            )?;
            conn.last_insert_rowid()
        };

        // 资源项整体替换（简单可靠，避免增量同步的边界问题）
        conn.execute(
            "DELETE FROM profile_item WHERE profile_id = ?1",
            params![id],
        )?;
        for item in items {
            conn.execute(
                "INSERT INTO profile_item (profile_id, resource_type, resource_ref, override_json)
                 VALUES (?1, ?2, ?3, '{}')",
                params![id, item.resource_type, item.resource_ref],
            )?;
        }

        // Agent 绑定：先清掉本档案的所有绑定，再按传入列表重建
        conn.execute(
            "UPDATE agent_target SET bound_profile = NULL WHERE bound_profile = ?1",
            params![id],
        )?;
        for agent_id in &profile.agents {
            conn.execute(
                "INSERT INTO agent_target (agent_id, name, bound_profile, enabled)
                 VALUES (?1, ?1, ?2, 1)
                 ON CONFLICT(agent_id) DO UPDATE SET bound_profile = excluded.bound_profile",
                params![agent_id, id],
            )?;
        }
        Ok(id)
    }

    pub fn profile_delete(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE agent_target SET bound_profile = NULL WHERE bound_profile = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM profile_item WHERE profile_id = ?1", params![id])?;
        conn.execute("DELETE FROM profile WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* -------------------------------------------------------- 备份 */

    pub fn backup_add(
        &self,
        target: &str,
        backup_path: &str,
        bytes: u64,
        note: &str,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO backup (target, backup_path, created_at, bytes, note)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![target, backup_path, crate::util::now_human(), bytes as i64, note],
        )?;
        Ok(())
    }

    pub fn backup_list(&self, limit: usize) -> Vec<crate::model::BackupInfo> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, target, backup_path, created_at, bytes, note
             FROM backup ORDER BY id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(crate::model::BackupInfo {
                id: row.get(0)?,
                target: row.get(1)?,
                backup_path: row.get(2)?,
                created_at: row.get(3)?,
                bytes: row.get::<_, i64>(4)? as u64,
                note: row.get(5)?,
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 每个文件保留最近 N 份备份的路径（更早的清理由调用方处理）
    pub fn backup_paths_for(&self, target: &str, keep: usize) -> Vec<String> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT backup_path FROM backup WHERE target = ?1 ORDER BY id DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![target, keep as i64], |row| row.get::<_, String>(0))
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn backup_delete(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM backup WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 同步历史留痕
    pub fn sync_history_add(
        &self,
        target: &str,
        summary: &str,
        backup_path: Option<&str>,
        status: &str,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO sync_history (target_id, diff_json, backup_path, status, actor, created_at)
             VALUES (0, ?1, ?2, ?3, 'gui', ?4)",
            params![
                serde_json::json!({ "target": target, "summary": summary }).to_string(),
                backup_path,
                status,
                crate::util::now_human()
            ],
        )?;
        Ok(())
    }

    /// 同步审计时间线（最新在前）
    pub fn sync_history_list(&self, limit: usize) -> Vec<crate::model::SyncHistoryEntry> {        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, diff_json, backup_path, status, actor, created_at
             FROM sync_history ORDER BY id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![limit as i64], |row| {
            let diff_json: String = row.get(1)?;
            let parsed: serde_json::Value = serde_json::from_str(&diff_json).unwrap_or_default();
            Ok(crate::model::SyncHistoryEntry {
                id: row.get(0)?,
                target: parsed
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                summary: parsed
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                backup_path: row.get(2)?,
                status: row.get(3)?,
                actor: row.get(4)?,
                created_at: row.get(5)?,
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /* -------------------------------------------------- 受管 Python 环境 */

    /// 记录一个受管（由本软件创建）的 Python 环境；已存在则更新版本
    pub fn python_env_upsert(
        &self,
        name: &str,
        manager: &str,
        path: &str,
        py_version: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn()?;
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM python_env WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            conn.execute(
                "UPDATE python_env SET name = ?1, manager = ?2, py_version = ?3, managed = 1 WHERE id = ?4",
                params![name, manager, py_version, id],
            )?;
        } else {
            conn.execute(
                "INSERT INTO python_env (name, manager, path, py_version, managed, deps_lock)
                 VALUES (?1, ?2, ?3, ?4, 1, '{}')",
                params![name, manager, path, py_version],
            )?;
        }
        Ok(())
    }

    /// 受管环境清单（界面与扫描结果合并展示）
    pub fn python_env_managed(&self) -> Vec<crate::model::PythonEnv> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT name, manager, path, py_version FROM python_env
             WHERE managed = 1 ORDER BY path",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| {
            let path: String = row.get(2)?;
            Ok(crate::model::PythonEnv {
                id: format!("managed::{}", path),
                name: row.get(0)?,
                manager: row.get(1)?,
                python_version: row.get(3)?,
                package_count: None,
                active: std::env::var("VIRTUAL_ENV")
                    .map(|v| v.eq_ignore_ascii_case(&path))
                    .unwrap_or(false),
                detail: Some("AgentHub 受管（创建于本软件）".to_string()),
                path,
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 清除受管记录（环境删除后调用）
    pub fn python_env_delete(&self, path: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM python_env WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// 取连接锁。PoisonError 不能直接 `?`（MutexGuard 非 Send），这里显式转换。
    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("数据库连接锁不可用"))
    }

    /* ------------------------------------------------------- settings */

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> AppSettings {
        self.get_setting("app")
            .and_then(|raw| serde_json::from_str::<AppSettings>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        self.set_setting("app", &serde_json::to_string(settings)?)
    }

    /* ------------------------------------------------------ snapshots */

    pub fn save_snapshot(&self, snap: &ScanSnapshot) -> Result<i64> {
        let json = serde_json::to_string(snap)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO scan_snapshot
               (scanned_at, duration_ms, agents, skills, mcp_servers, python_envs, npm_packages, json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snap.scanned_at,
                snap.duration_ms as i64,
                snap.agents.iter().filter(|a| a.installed).count() as i64,
                snap.skills.len() as i64,
                snap.mcp_servers.len() as i64,
                snap.python_envs.len() as i64,
                snap.npm_packages.len() as i64,
                json
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn latest_snapshot(&self) -> Option<ScanSnapshot> {
        let conn = self.conn.lock().ok()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT json FROM scan_snapshot ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        raw.and_then(|j| serde_json::from_str::<ScanSnapshot>(&j).ok())
    }

    pub fn snapshot_history(&self, limit: usize) -> Vec<SnapshotMeta> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, scanned_at, duration_ms, agents, skills, mcp_servers, python_envs, npm_packages
             FROM scan_snapshot ORDER BY id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SnapshotMeta {
                id: row.get(0)?,
                scanned_at: row.get(1)?,
                duration_ms: row.get::<_, i64>(2)? as u64,
                agents: row.get::<_, i64>(3)? as usize,
                skills: row.get::<_, i64>(4)? as usize,
                mcp_servers: row.get::<_, i64>(5)? as usize,
                python_envs: row.get::<_, i64>(6)? as usize,
                npm_packages: row.get::<_, i64>(7)? as usize,
            })
        });
        match rows {
            Ok(iter) => iter.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 按编号取一份完整快照（快照对比用）
    pub fn snapshot_by_id(&self, id: i64) -> Option<ScanSnapshot> {
        let conn = self.conn.lock().ok()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT json FROM scan_snapshot WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        raw.and_then(|j| serde_json::from_str::<ScanSnapshot>(&j).ok())
    }

    /// 保留最近 `keep` 份快照，避免数据库无限增长。
    pub fn prune_snapshots(&self, keep: usize) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM scan_snapshot WHERE id NOT IN (
                 SELECT id FROM scan_snapshot ORDER BY id DESC LIMIT ?1
             )",
            params![keep as i64],
        )?;
        Ok(())
    }
}