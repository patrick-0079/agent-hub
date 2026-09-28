# Agent 定义文件 schema 速查

> 面向「要接入一个新 Agent」或「想改现有 Agent 行为」的人。定义文件是**数据**，内核只按声明求值。

## 文件位置与加载顺序

```
src-tauri/config/agents/<id>.toml          内置（编译进二进制，21 个）
        ↓ 首次启动自动导出（已存在的文件不覆盖）
%APPDATA%\dev.agenthub.desktop\agents\*.toml   用户目录，可直接编辑
        ↓ 同名文件覆盖内置
运行时判定结果
```

- 定义目录可在 **设置 → Agent 定义** 里改（`definitionsDir`）
- 与内置逐字相同的副本视为「内置」（不告警）；内容改过才算自定义并提示覆盖
- 解析失败的定义会被**跳过并回退内置**，界面上报出具体错误行
- **未知键直接报错**（`deny_unknown_fields`）—— 不做静默忽略

## 字段总览

键名一律 **camelCase**（schema 用 `rename_all = "camelCase"`）。

### `[agent]`

| 键 | 必填 | 说明 |
|---|---|---|
| `id` | ✅ | 唯一标识，也是文件名（`<id>.toml`） |
| `name` / `vendor` | | 显示名与厂商 |
| `kind` | | `cli` \| `ide` \| `extension` \| `host`（`host` 是宿主应用，不计入 Agent） |
| `accent` | | 界面强调色，如 `"#8b8bff"` |
| `note` | | 展示在详情里的说明 |
| `adapter` | | 内置同步适配器标识（M2 起可用） |

### `[[evidence]]` — 显式证据规则

只在**无法用路径表达**时使用（大多数情况由 `[[paths]]` 的 role 自动派生）。

| 键 | 说明 |
|---|---|
| `capability` | 内核能力 id，必须是能力目录里已登记的（写错直接报错） |
| `strength` | `strong` \| `medium` \| `weak` |
| `label` | 界面上显示的证据名称 |
| `args` | 能力参数，如程序名 `["opencode"]`、包名 `["opencode-ai"]` |
| `paths` | 能力参数，如额外搜索目录 |
| `root` / `key` / `format` | 读取文件类能力的参数 |

常用：

```toml
[[evidence]]
capability = "which"            # 解析命令行程序（命中时自动探测版本）
args = ["opencode"]
paths = ["%APPDATA%/npm", "%PROGRAMDATA%/npm/npm"]
strength = "strong"
label = "命令行程序 opencode"

[[evidence]]
capability = "npm.global.has"   # 在已扫描的全局包清单里查找
args = ["opencode-ai", "@opencode-ai/cli-node-windows-x64"]
strength = "strong"
label = "全局 npm 包"
```

### `[[paths]]` — 路径与角色（**会自动派生证据**）

| `role` | 派生证据强度 | 内核如何使用 |
|---|---|---|
| `install` | **强** | 程序本体安装痕迹；作为 Agent 根目录 |
| `config` | 中 | 配置文件；展示 + 供 `[[mcp]]` / `[[provider]]` 引用 |
| `data` | 弱 | 数据 / 会话 / 缓存目录 |
| `skills` | 弱 | **Skill 扫描目录**，配合 `deploy` 声明期望部署方式 |
| `mcp` | 不派生 | 仅表示这是 MCP 目录 |
| `provider` | 不派生 | 仅表示这是供应商配置 |

| 键 | 说明 |
|---|---|
| `label` | 显示名 |
| `path` | 支持 `~`、`%APPDATA%`、`$HOME`，以及相对当前目录的 `./` |
| `format` | `json` \| `toml` \| `yaml` \| `conf` \| `md` \| `dir` \| `db`（留空按目录处理） |
| `deploy` | `link` \| `copy`（仅 `role = "skills"` 有意义） |

### `[[mcp]]` — MCP 配置来源

明确声明节点位置，内核**不做格式猜测**。

```toml
[[mcp]]
file = "~/.config/opencode/opencode.json"
root = "mcp"          # 点号分隔的节点路径，如 "mcp"、"mcpServers"、"mcp.servers"
format = "json"
```

内核会把每个条目规范化：`command` 为数组时首元素当程序、其余当参数；`type = local/remote` 归一为 `stdio`/`http`；`environment` 与 `env` 都识别。

### `[[provider]]` — 供应商线索来源

```toml
[[provider]]
file = "~/.config/opencode/opencode.json"
root = "provider"
format = "json"
kind = "base-url"            # base-url | model | credential | api-key
baseUrlKey = "options.baseURL"   # 条目内取值的点号路径
modelsKey = "models"             # 用于统计模型数量的键
label = "opencode"               # 线索标签前缀

# 凭据类来源：只解析键名，值永不进入内存快照
[[provider]]
file = "~/.dsh/.credentials.yaml"
root = "refs"
format = "yaml"
kind = "api-key"
label = "DSH 密钥"
keysOnly = true
```

**内核安全不变式**：无论声明什么类型，键名命中 `key` / `token` / `secret` / `password` 的条目一律脱敏并归为 `api-key`。

### `[capabilities]` — 能力分级授权

| 键 | 说明 |
|---|---|
| `maxTier` | `observe` \| `parse` \| `deploy` \| `mutate` —— 允许 AgentHub 对该 Agent 操作到哪一级 |
| `skillMethods` | 允许的 Skill 部署方式：`link` / `copy` |
| `configWrite` | 允许的配置写入方式：`merge-keys` / `managed-block` |
| `notes` | 说明文字 |

## 判定逻辑

```
强证据命中  → installed（已安装）
中证据命中  → configured（仅发现配置：可能已卸载或为便携版）
弱证据命中  → leftover（残留：只有数据/技能目录）
都没有      → absent（未安装）
```

已安装的 Agent 只保留「命中」的证据用于展示，避免「没找到 X」的噪音。

## 常见改法

| 想做什么 | 怎么做 |
|---|---|
| 程序不在 PATH | 用设置页的「指定路径」覆盖，或在 `[[evidence]]` 的 `paths` 里加目录 |
| 换 Skill 目录 | 改对应 `[[paths]] role = "skills"` 的 `path` |
| 新增一个 Agent | 复制一份内置定义，改 `id` / `name` / `[[paths]]` / `[[evidence]]` |
| 某 Agent 不希望被写入 | 把 `[capabilities] maxTier` 设为 `"observe"` |
| 只想让它被发现、不做任何提取 | 只留 `[[paths]] role = "install"` 与 `[[evidence]] which` |

## 校验

保存时（界面的编辑器或直接改文件后重新加载）会依次校验：`agent.id` 非空、`kind` 合法、`evidence.capability` 在能力目录中已登记、`strength` 合法、`paths.role` 合法、`maxTier` 合法。任一项不通过即报错并回退内置定义。