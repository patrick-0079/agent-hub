# AgentHub 方案（v3）

> 多 Agent 运行环境统一管理器。本文为完整设计文档，M0 已按此实现。

## 1. 要解决的核心问题

一台电脑上往往同时跑着多个 Agent（Claude Code、Codex、DSH、Cursor、Cline…），各自有独立的配置格式与位置：

| 痛点 | 现状 |
|---|---|
| 模型供应商配置分散 | API Key / Base URL 散落在 `~/.claude/settings.json`、`~/.codex/config.toml`、各 IDE 设置里，换一个 Key 要改 N 处 |
| MCP 重复配置 | 同一个 MCP Server 要在每个 Agent 里各写一遍，格式还略有差异 |
| Skill 无法共享 | 写好的 skill 目录要手动拷贝/软链到各 Agent 的 skills 目录，链接目标一挪就静默失效 |
| Python 环境混乱 | conda / uv / venv 各管各的，Agent 不知道用哪个环境 |
| npm 工具不可追溯 | 全局装了哪些 CLI 工具（含 MCP 常用的 npx 包）没有清单、没有版本锁定 |

**核心思路：单一事实源（Single Source of Truth）+ 适配器同步（Adapter Sync）。**
所有资源在本软件中统一管理，通过「档案（Profile）」组合成一套环境，再由各 Agent 的适配器生成对应配置，同步前可 diff、可回滚。

## 2. 管理的五类资源

### 2.1 模型供应商（Provider）
- 字段：名称、类型（OpenAI 兼容 / Anthropic / OpenRouter / Ollama / Azure…）、Base URL、API Key（加密存储）、模型列表、默认参数
- 能力：**连通性测试**（最小请求验证 Key/URL，展示延迟与错误原因）、多 Key 轮换标记、按 Profile 决定哪个 Agent 用哪个 Provider

### 2.2 Skill
- 以目录为单位识别 `SKILL.md`（frontmatter：name / description / when_to_use）
- 支持符号链接 / junction 部署形态，并**检测失效链接**（目标被删除时 Agent 会静默失效）
- 能力：本地库管理、Git 仓库导入与更新检查、启用/停用、同步方式可选拷贝或链接

### 2.3 MCP Server
- 字段：名称、启动方式（stdio: command/args/env；http/sse: url/headers）、依赖运行时（node/uvx/python/二进制）
- 内置常见 MCP 注册表（filesystem、fetch、playwright、context7…）一键添加
- 能力：配置校验、**握手健康检查**（spawn → initialize → 响应逐步打勾）、依赖预检

### 2.4 npm 包
- 管理全局工具清单（package + 版本约束），支持 npm / pnpm
- 能力：扫描现有全局包导入、按 Profile 声明式安装/卸载、版本锁定与更新提示（与 npx 型 MCP 联动）

### 2.5 Python 环境（uv + conda）
- 自动探测：conda 安装位置及全部 env、uv 受管解释器、现有 venv（`pyvenv.cfg`）
- 能力：创建/删除/克隆环境（uv 优先）、记录 Python 版本与关键依赖、锁定状态可视化、绑定到 Profile

## 3. 关键概念

### 3.1 Profile（环境档案）

```
Profile "日常开发"                 Profile "轻量问答"
├─ Provider: OpenRouter           ├─ Provider: Ollama(本地)
├─ MCP: filesystem, context7      ├─ MCP: fetch
├─ Skills: office-*, code-review  ├─ Skills: (无)
├─ Python: uv env "py312-ai"      └─ npm: (无)
└─ npm: pnpm, tsx

        │  sync (diff → backup → write)
        ▼
┌──────────────────────────────────────────────┐
│ Adapter: Claude Code  → ~/.claude/*.json      │
│ Adapter: Codex        → ~/.codex/config.toml  │
│ Adapter: DSH          → ~/.dsh/…              │
│ Adapter: 自定义 Agent  → 任意路径与格式        │
└──────────────────────────────────────────────┘
```

一个 Agent 目标可绑定一个 Profile；同步是声明式的：本软件生成「期望状态」，与目标文件当前内容 diff，用户确认后写入。

**安全机制**：写入前自动备份（保留 N 份）、检测外部修改（hash/mtime）防止覆盖手改内容、一键回滚。

### 3.2 声明式 Agent 定义 + 能力分级（M0 已实现）

**内核不含任何 Agent 专属逻辑**，只提供带分级的基础能力原语；Agent 的一切 —— 名字、去哪找、怎么判定、路径角色、允许操作到哪一级 —— 都是**数据**，写在 `config/agents/<id>.toml` 里。

| 级别 | 名称 | 副作用 | 授权方式 |
|---|---|---|---|
| **T0** | 观察 Observe | 无 | 扫描时自动执行 |
| **T1** | 解析 Parse | 无（只读内容） | 扫描时自动执行 |
| **T2** | 部署 Deploy | 写磁盘（可备份回滚） | 用户显式确认 |
| **T3** | 变更 Mutate | 调外部程序改系统状态 | 显式确认 + 二次警告 |

- 定义加载：内置定义编译进二进制（21 个）→ 首次启动导出到 `%APPDATA%/dev.agenthub.desktop/agents/` → 同名用户文件覆盖内置
- 未改动的导出副本不算自定义（不告警）；写坏的定义报错并**回退内置**；未知键**直接报错**（不做静默忽略）
- 状态由证据强度汇总：强（程序本体）→ 已安装；中（配置文件）→ 仅发现配置；弱（数据/技能目录）→ 残留
- `kind` 区分 `cli` / `ide` / `extension` / `host`，其中 `host`（VS Code 等）是宿主应用，不计入 Agent

M0 实现全部 T0/T1 能力（探测所需），T2/T3 已登记级别与参数、在界面上可见但标为「待实现」。

**目标形态（M2/M3.5）**：定义将进一步支持模版语法（Tera）与四种写入模式，形如：

```yaml
# adapters/my-agent.yaml
name: My Custom Agent
detect:                          # 自动探测该 Agent 是否存在
  paths: ["{{HOME}}/.myagent/config.json"]

resources:
  mcp:
    # 方式A：结构化写入 —— 定位文件内节点，只管理自己的键
    file: "{{HOME}}/.myagent/config.json"
    format: json                 # json | toml | yaml | env | text
    root: "/mcpServers"          # JSON Pointer / TOML path
    merge: managed-keys
    template: |
      {
        "command": "{{mcp.command}}",
        "args": {{mcp.args | json}},
        "env": {{mcp.env_with_secrets | json}}
      }

  provider:
    # 方式B：环境变量映射
    file: "{{HOME}}/.myagent/.env"
    format: env
    mapping:
      OPENAI_BASE_URL: "{{provider.base_url}}"
      OPENAI_API_KEY:  "{{provider.api_key}}"

  skills:
    # 方式C：目录部署
    dest: "{{HOME}}/.myagent/skills/{{skill.name}}"
    method: copy                 # copy | symlink | junction
    include: ["SKILL.md", "**/*"]

  python_env:
    # 方式D：文本文件托管块
    file: "{{HOME}}/.myagent/setup.sh"
    format: text
    managed_block: "AgentHub:python"
    template: |
      export PATH="{{env.path}}/Scripts:$PATH"
      export VIRTUAL_ENV="{{env.path}}"
```

**模版语法**：Rust 侧 Tera（Jinja2 兼容），前端 diff 预览复用同一渲染结果。
- 上下文：`profile`、`provider`、`mcp`、`skill`、`env`、`npm`、`secrets`（渲染瞬间解密注入，磁盘上的模版永不含明文 Key）
- 过滤器：`| json`、`| toml_value`、`| path`、`| quote`
- 四种写入模式：`merge: managed-keys`（最安全，首选）、`managed_block`（文本托管块）、整文件模版渲染、目录部署
- **安全不变式**（对所有 Adapter 一致生效）：写前备份、外部修改检测、一键回滚、dry-run diff 必过

## 4. 全功能可视化（交付标准）

每个功能必须同时交付：**后端能力 + 对应 GUI + 操作反馈可视化**。

| 标准线 | 说明 |
|---|---|
| 无 CLI-only 功能 | CLI 仅作为自动化的补充镜像（M4），不是任何功能的唯一入口 |
| 长任务过程可视化 | 装包、建环境、同步、健康检查 → 实时进度 + 流式输出控制台（stdout/stderr 逐行，错误高亮） |
| 破坏性操作先看 diff | 同步 / 回滚 / 删除一律先展示「将发生什么」 |
| 状态可见 | 所有资源带健康状态灯，悬停显示详情 |
| 命令面板 | **✅ `Ctrl+K` 全局搜索直达（M4 提前落地）** |

### 界面结构

```
┌──────┬────────────────────────────────────────────────┐
│ 侧边栏 │  ① 仪表盘   ② 供应商   ③ Skills   ④ MCP        │
│      │  ⑤ npm 包   ⑥ Python环境  ⑦ Profiles          │
│      │  ⑧ Agents与同步  ⑨ Adapter编辑器  ⑩ 历史审计     │
│      │  ⑪ 密钥保险库  ⑫ 设置                            │
├──────┴────────────────────────────────────────────────┤
│  底部：全局任务控制台（可折叠，显示进行中/最近完成的长任务）  │
└───────────────────────────────────────────────────────┘
```

**① 仪表盘**：概览卡片；**拓扑图**（Profile ↔ Agent ↔ 资源关系，节点颜色表状态，点击直达）；待办提醒（外部修改冲突、可用更新、Key 连通失败）。
**② 模型供应商**：表格 + 详情抽屉；「测试连接」动画等待 → 延迟 / 可用模型数 / 错误原因；类型切换时表单字段自适应。
**③ Skills**：卡片网格；SKILL.md 渲染预览；Git 来源显示分支/落后提交数；链接与失效状态显式标注。
**④ MCP**：状态灯列表；握手测试可视化日志；应用商店式注册表浏览；stdio/http 切换时字段联动。
**⑤ npm 包**：表格（当前/声明版本、来源、可更新徽章）；安装卸载 → 底部控制台流式输出。
**⑥ Python 环境**：按管理器分组卡片；创建向导（分步可视化，执行页实时输出）；依赖表格与锁定状态。
**⑦ Profiles**：可视化组合编辑器（左资源库 / 右档案内容，拖拽添加，per-profile 覆盖）；多档案对比。
**⑧ Agents 与同步**：目标列表（同步状态徽章、外部已修改警示）；同步三屏（diff 预览 → 确认 → 执行）；备份时间线可 diff 与回滚。
**⑨ Adapter 编辑器**：表单 ⇄ YAML 源码 ⇄ 实时渲染预览三视图联动；反向生成向导（指向配置文件 → 树形结构 → 勾选节点 → 生成草稿）。
**⑩ 历史审计**：全局时间线，每次同步/回滚/变更一条，展开看 diff、操作者、备份路径。
**⑪ 密钥保险库**：Key 列表（归属、验证状态），默认永不显示明文，导出自动剔除。
**⑫ 设置**：可执行文件探测面板（找到/未找到、路径、版本、手动指定）、扫描范围、备份保留数、主题与语言。
**首次启动引导**：扫描本机 → 可视化展示发现 → 勾选导入建议 → 创建第一个 Profile → 完成。

## 5. 架构

```
┌────────────────────────────────────────────┐
│  GUI（Tauri 2 + React/TypeScript）          │
├────────────────────────────────────────────┤
│  CLI（同一核心，供脚本与 Agent 自己调用）        │
├────────────────────────────────────────────┤
│  Core（Rust）                               │
│  ├─ Inventory Scanner                       │
│  ├─ Store：SQLite（资源、Profile、同步历史）    │
│  ├─ Secret Vault：Windows DPAPI 加密 API Key  │
│  ├─ Sync Engine：期望状态 → diff → backup → 原子写入 │
│  ├─ Adapters：内置定义 + 用户自定义（同一加载器）  │
│  ├─ Template Engine（Tera）：渲染 + diff 预览共用 │
│  ├─ Runners：uv、conda、npm/pnpm、MCP 握手探测  │
│  └─ ProviderPort (trait)：★ 网关预留            │
└────────────────────────────────────────────┘
```

**选型理由**：Tauri 2 + Rust → 安装包小（~10MB，Electron 100MB+）、原生 spawn 子进程性能好、DPAPI/keyring 集成方便，前端仍是 React 生态。SQLite 本地单用户场景足够，迁移即一个文件。CLI 与 GUI 同核，Agent 自己也能通过 CLI 管理环境。

## 6. 数据模型

```
provider(id, name, kind, base_url, key_ref, models_json, health)
skill(id, name, source_type[local|git], path/repo_url, version, enabled)
mcp_server(id, name, transport, command, args_json, env_json, url, package_ref, health)
npm_package(id, name, version_spec, installed, scope, manager)
python_env(id, name, manager[uv|conda|venv], path, py_version, deps_lock, managed)
profile(id, name, description)
profile_item(profile_id, resource_type, resource_ref, override_json)
agent_target(id, agent_id, config_root, bound_profile, last_sync_hash, last_synced_at)
sync_history(id, target_id, profile_id, diff_json, backup_path, status, actor, created_at)
scan_snapshot(id, scanned_at, duration_ms, agents, skills, mcp_servers, python_envs, npm_packages, json)
```

M0 已建全部表结构，实际写入 `settings` 与 `scan_snapshot`。

## 7. 路线图

| 阶段 | 内容 |
|---|---|
| **M0** ✅ | 工程骨架（Tauri+Rust+React+SQLite）、**能力分级内核 + 配置驱动的 Agent 定义（21 个）**、Inventory Scanner、仪表盘与拓扑图、Onboarding、设置探测面板、只读资源页、任务控制台、`--scan-json` 自检 |
| **M1** ✅ | 五类资源 CRUD（MCP 与供应商资源库）；密钥保险库 **DPAPI**；供应商分发（claude-code 目标）；**环境档案 Profile + 一键应用**；**Provider 连通性测试（GET models，延迟/模型数/错误三级结果）**；**快照对比（任意两次扫描的资源级差异）**；**档案导出/导入（自包含 JSON，密钥永不落文件）**；余下：MCP 应用商店（内置注册表） |
| **M2 · Skill 部分** ✅ | **T2/T3 首批落地**：`skill.copy` / `skill.link` / `skill.relink` / `path.delete` / `git.clone` / `backup.*`；快捷导入（本地目录 + Git 仓库 + 代理）、一键清理失效链接、重建链接、**删除统一入回收站 + 独立回收站管理页（统计/详情/部分恢复/按期清理/永久删除）**、可恢复操作清单、路径别名去重、`--self-test` 沙箱自检与 `--trash-json` |
| M2（其余） | **配置写入类 T2 已落地 MCP 部分**：`file.merge_keys`（JSON 结构合并）+ `file.write_block`（TOML 托管块）、MCP 资源库 CRUD 与从扫描导入、三屏分发向导（逐文件 diff）、写前备份与回滚、受管键状态跟踪与清理；余下：Provider 分发到更多 Agent（opencode）、diff 三屏推广到其它资源、Agent 配置同步审计时间线 |
| M3 | **T3 其余**：~~MCP 握手健康检查~~ **✅ 已落地**（stdio 真实起进程 / http initialize，`mcp_server.health` 常驻展示）；余下：uv / conda 环境创建向导；npm 声明式安装与版本锁定 |
| M3.5 | 定义模版语法（Tera）+ 可视化编辑器三视图；反向生成向导；定义导入导出分享 |
| M4 | ~~命令面板~~ **✅ 已落地**（Ctrl+K 全局搜索直达页面与操作）；更多内置 Adapter（Cursor/Cline/Windsurf 同步）；换机迁移导入导出（**档案级已落地**）；CLI 完整化；浅色主题与英文界面 |
| M5 | 本地统一网关（OpenAI 兼容 proxy）：用量统计仪表盘、Key 集中轮换、故障切换 |

## 8. 风险与对策

| 风险 | 对策 |
|---|---|
| 各 Agent 配置格式频繁变动 | Adapter 声明式 + 写前备份 + 外部修改检测，坏了可回滚 |
| Windows 符号链接权限 | Skill 同步默认拷贝，链接作为可选项；已有链接形态显式识别 |
| conda/uv 不在 PATH 或非标准安装 | 多路径兜底探测 + 设置页手动指定 |
| 密钥安全 | 只读键名、DPAPI 加密、导出剔除、日志永不打印明文 |
| 链接目标被删除导致 Skill 静默失效 | 扫描阶段检测失效链接并在仪表盘告警（M0 已实现） |