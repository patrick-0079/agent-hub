# AgentHub

> 一台电脑上多个 AI Agent 的**统一环境管理器**：模型供应商、Skills、MCP 服务器、npm 包、Python 环境（uv / conda / venv）集中管理，再按需分发到各个 Agent。

当前进度：**M1 全部完成 + M3 的 MCP 握手检查落地**（可运行）；界面已全面扁平化重设计。

---

## 核心设计：内核只提供基础能力，Agent 的一切都是配置

### 1. 能力分级（内核原语）

内核**不含任何 Agent 专属逻辑**，只暴露 31 个带分级的基础能力（T0 观察 8 · T1 解析 6 · T2 部署 8 · T3 变更 9），**31 个全部实现**：

| 级别 | 名称 | 副作用 | 授权方式 | 示例能力 |
|---|---|---|---|---|
| **T0** | 观察 Observe | 无 | 扫描时自动执行 | `path.exists`、`path.glob`、`which`、`run.version`、`npm.global.has`、`link.read`、`dir.count` |
| **T1** | 解析 Parse | 无（只读内容） | 扫描时自动执行 | `file.json.get`、`file.toml.keys`、`file.text.head`、`skill.frontmatter` |
| **T2** | 部署 Deploy | 写磁盘（可备份回滚） | 用户显式确认 | `skill.copy`、`skill.link`、`skill.relink`、`file.merge_keys`、`file.write_block`、`backup.restore` |
| **T3** | 变更 Mutate | 调外部程序改系统状态 | 显式确认 + 二次警告 | `pkg.npm.install`、`py.env.create`、`net.provider.probe`、`net.provider.balance`、`proc.spawn.probe`、`path.delete`、`git.clone` |

级别决定「谁能做决定」：T0/T1 无副作用，扫描时自动跑；T2 起是写操作，必须在界面上先看到 diff 并确认。定义文件里每个 Agent 还要声明自己的 `maxTier`（能力上限），越权的操作直接不可用。

已实现情况：

- **T0 观察 8/8** 全部就绪（`path.glob` 支持段内 `*` / `?` 与跨层 `**`，深度与条目上限保护）
- **T1 解析 6/6** 全部就绪
- **T2 部署 7/8**：`skill.copy`、`skill.link`、`skill.relink`、`file.merge_keys`、`file.write_block`、`backup.create`、`backup.restore` 已落地（`file.render` 待 M3.5 模板语法）
- **T3 变更 5/9**：`path.delete`、`git.clone`、`net.provider.probe`（供应商连通性测试）、`net.provider.balance`（余额查询）、`proc.spawn.probe`（MCP 握手）已落地（npm 装包 / Python 建环境待 M3）

## Skill 操作（T2/T3 首批落地）

| 功能 | 说明 |
|---|---|
| **快捷导入** | 从**本地目录**或 **Git 仓库**导入到技能库。支持三种来源形态：单个 Skill、多个 Skill 子目录、`skills/` 容器（如 anthropics/skills 仓库）；可选**链接**（省空间）或**拷贝**（独立副本）；已存在同名项自动跳过不覆盖 |
| **一键清理失效** | 批量删除指向缺失目标的空链接 —— **整批移入一个回收站条目**（可一键整批还原，且链接的原有指向关系原样保留）；只动链接本身，**绝不跟随目标**，也不碰任何真实目录 |
| **重建链接** | 把失效链接重新指向现有技能库（支持 `<分类>/<技能>` 嵌套结构查找）；库里没有的自动跳过 |
| **删除 Skill** | 链接与真实目录**统一移入回收站**：链接保留指向关系、目录保留完整内容，都能一键恢复到原位；且只允许操作已声明的技能库目录内的内容 |
| **回收站与操作清单** | **独立的「回收站」页面**：统计（条目/对象/占用/时间）、按类型筛选、查看条目内容（逐对象列出原位置、链接目标、大小、是否可恢复）、整条或**按对象部分恢复**、按期清理（7/30 天前）、永久删除、清空回收站；「历史与审计」页提供入口与操作清单 |

**三条安全不变式**（所有 T2/T3 操作统一遵守）：

1. **先出计划** — 每个操作先 `plan_*` 出「将发生什么」，界面展示后才允许执行
2. **先留凭据** — 动手前写 manifest JSON，据此可撤销导入、重建被删链接、从回收站移回
3. **删除不毁灭** — 链接与目录**统一移入回收站**（链接保留指向、目录保留内容），都能一键还原；删除范围受技能库白名单限制

跨境场景也验证过：技能库在 C: 盘、数据目录在 A: 盘时，移动会走「复制 + 删除」回退（沙箱自检专门覆盖了这个 case）。

### 路径别名去重

`~/.config/opencode/skills` 这类位置**本身可能就是指向 `~/.agent/skills` 的 junction**。同一份内容被两条路径扫两遍会导致：计数虚高、批量操作把同一个对象当成两个（第二个必然报「找不到文件」）。

现在两层都做了去重：

- **扫描层**：按规范化路径归并技能库根目录，同组优先保留真实目录（非链接），别名目录跳过并给出告警
- **动作层**：批量删除按「父目录规范化路径 + 文件名」计算对象身份，重复别名合并为一次操作并如实标注（不再误报为失败）

## 环境档案 Profile（一键应用）

把「哪套 MCP + 哪个供应商 + 哪些 Skill」打包成一套环境，之后**一次确认**应用到任意多个 Agent —— 这是「单一事实源 → 一键分发」的完整形态。

| 能力 | 说明 |
|---|---|
| **可视化组合** | 分三个页签挑选资源（MCP / 供应商 / Skill），Skill 支持搜索与按分类筛选（已排除失效链接），并绑定「这套档案给谁用」 |
| **一键应用** | 复用同一套三屏向导：① 选目标 Agent → ② **逐个文件看 diff**（配置文件）+ **逐个 Skill 看动作**（部署 / 未变 / 跳过）→ ③ 执行结果 |
| **配置写入** | MCP 与供应商沿用已验证的引擎：结构合并（JSON）/ 托管块（TOML）、受管键跟踪、非受管同名条目默认跳过 |
| **Skill 部署** | 按目标 Agent 定义里 `role = "skills"` 的路径与 `deploy` 方式落位：**链接**（junction，不占空间）或**拷贝**；已存在的内容默认不动 |
| **可回滚** | 配置文件进入「配置备份」可一键回滚；Skill 部署写入可撤销清单 |
| **导出 / 导入** | 档案导出为**自包含 `.agenthub-profile.json`**（原子写入数据目录 `exports/`，**不含任何密钥字段**）；拷给别人即可导入 —— 同名自动加「（导入）」后缀，永不覆盖现有数据 |

## 快照对比（M1）

「上次扫描之后环境发生了什么」：在「历史与审计」页任选两次扫描（默认对比最近两份），按资源类型给出 **新增 / 移除 / 变化** 三类差异 —— Agent（状态与版本变化）、Skill（新增、移除、大小变化、链接失效/恢复）、MCP（增删与命令/URL 变化）、供应商线索、Python 环境（版本与包数量）、npm 包（版本变化），每条都带一句话说明。

## npm 全局包：安装 / 卸载 / 检查更新

| 能力 | 说明 |
|---|---|
| **声明式安装 / 卸载** | 「安装包」先看计划（`npm install -g` / `pnpm add -g`，T3 确认）再执行，完成后 `ls -g` 读回版本并落库为受管记录；卸载走包管理器可随时重装 |
| **检查更新 / 一键升级** | 「检查更新」跑 `npm outdated -g --json`（只读）：可更新的包显示「→ 最新版」徽章与「升级」按钮（升级即重装最新版，复用同一 T3 流程）。pnpm 视其版本对 `--json` 的支持，无法解析时返回空 |

## 模板渲染（file.render · M3.5 内核原语）

「Agent 定义与能力」页的「模板试渲染」：Tera（Jinja2 兼容）模板 + JSON 上下文 → 实时文本预览（变量、嵌套路径、循环、过滤器；**未定义变量报错而不是静默置空**）。这是 M3.5 适配器模版写入的原语 —— 编辑器三视图与反向生成向导仍在 M3.5 计划中，但渲染内核与 GUI 入口已就绪。

## 换机迁移（M4）

「设置 → 换机迁移」：一个目录带走全部环境定义，拷到新机器即可导入。

| 能力 | 说明 |
|---|---|
| **导出迁移包** | `exports/migrations/migration-<时间戳>/` 一个自包含目录：**设置**（扫描范围/代理/工具链覆盖；引导标记不带走）+ **全部环境档案**（自包含 JSON，包内序号命名避免非 ASCII 名称冲突）+ **自定义 Agent 定义**（只带走真改过的 —— 未改动的导出副本目标机上有内置，不浪费体积） |
| **导入** | 逐项可选：档案走同名「（导入）」后缀（永不覆盖）；定义同名**跳过**（不覆盖目标机手改）；设置保留目标机的引导标记与定义目录（源机路径不搬过来） |
| **密钥永不进包** | DPAPI 密文与当前用户绑定，换机后无法解密 —— 迁移包不含任何密钥，到新机器重新录入即可 |
| **无界面镜像** | 迁移包列表/导入结果与 GUI 同一份存储 |

## 供应商资源库 + 密钥保险库（DPAPI）

「换一个 Key 不用改 N 处」：供应商只在 AgentHub 里维护一份，密钥加密存本机，分发时按定义注入各 Agent 配置。

| 能力 | 说明 |
|---|---|
| **密钥保险库** | Windows **DPAPI**（`CryptProtectData`）加密，密文与当前用户绑定 —— 换用户或换机器都无法解密；数据库只存引用名，**落盘文件不含明文**（自检会断言这一点） |
| **供应商 CRUD** | 名称、类型（OpenAI 兼容 / Anthropic / OpenRouter / Ollama / Azure）、Base URL、模型列表、启用开关、备注 |
| **连通性测试** | 「测试」/「测试连接」对 Base URL 发一次最小只读请求（GET models）：OpenAI 兼容 404 自动兜底 `/v1/models`、Anthropic 补 `/v1`、Ollama 补协议与 `/api/tags`；结果三级 **ok（延迟+模型数）/ no_key（端点可达但还没录 Key）/ error（原因明确到 DNS、超时、代理、认证）**，落库常驻展示；Key 只在内存存活一次请求，响应体回显先打码 |
| **余额查询** | DeepSeek 类型供应商卡片上有「查余额」按钮（GET `/user/balance`）：返回**币种、总余额、赠送与充值拆分、账户可用状态**，结果落库常驻展示；base 带 `/v1` 时自动剥掉；未支持的类型返回 `unsupported`（界面不显示入口）。后续按类型扩展其它厂商 |
| **密钥操作** | 录入（直接加密入保险库）、掩码展示、显式「显示密钥」（需点一下，不写日志）、删除时可选一并清除密钥 |
| **从线索导入** | 把扫描到的 Base URL 等线索收编为受管资源；**密钥不会被导入**（AgentHub 从不读取明文密钥），需重新录入 |
| **分发到 Agent** | 复用同一套三屏向导：按定义里声明的 `[[providerWrite]]` 把字段注入目标文件；**变更详情与 diff 全程掩码**（密钥只以 `sk-a••••wxyz （37 字符）` 形式出现），写前自动备份 |
| **保险库自检** | 启动与界面都会检查「所有密钥是否可解密」，跨用户/跨机器的密文会被明确报出而不是静默失败 |

```toml
# 定义里声明「把供应商写到哪、写什么字段」（claude-code.toml 实例）
[[providerWrite]]
file = "~/.claude/settings.json"
root = "env"                 # Claude Code 从 settings.json 的 env 段读取
format = "json"
objectPerProvider = false    # 平铺：这个目标只能承载一个供应商
entries = [
  { key = "ANTHROPIC_BASE_URL", from = "baseUrl" },
  { key = "ANTHROPIC_API_KEY",  from = "apiKey" },
]
```

`key` 支持**点号路径**（如 `options.baseURL`），`from` 取值 `baseUrl` / `apiKey` / `name` / `models`；`objectPerProvider = true` 时每个供应商写成 root 下的一个对象。**字面量**（`value`）用于固定值：JSON 字面量（`"true"` → 布尔）或普通字符串（如 opencode 要求的 SDK 包名）。

> **密钥写入是官方机制**：Claude Code 等工具本身就通过配置文件读取 Base URL 与 Key，因此分发时确实会把明文写进这些文件（计划里会明确提示）。界面与日志中的密钥一律掩码。
>
> **opencode 的供应商写入已启用**（`maxTier` 升到 deploy）：形状按本机实际可用的自定义供应商条目校准 —— `provider.<id> = { npm: "@ai-sdk/openai-compatible", name, options: { baseURL, apiKey, modelsDiscovery: { enabled: true } } }`，其中 `npm` 与开关是字面量，模型由 opencode 的 modelsDiscovery 自动发现。键名取供应商资源名：与你手写条目同名时默认跳过，想接管就在向导里勾选「覆盖同名非受管条目」。codex 暂未启用（TOML 形态不同，随需求补）。

## MCP 资源库与配置分发（T2 写入）

把「单一事实源 → 一键分发」真正落地：MCP 资源只在 AgentHub 里维护一份，写入各 Agent 时按**定义文件里声明的目标位置与形状**生成。

| 能力 | 说明 |
|---|---|
| **资源库 CRUD** | 名称、传输方式（stdio/http/sse）、命令与参数、环境变量键值对、**http/sse 请求头（值支持 `%VAR%` 引用，认证类远端 MCP 的握手需要）**、URL、启用开关、备注 |
| **从扫描导入** | 把各 Agent 配置里已有的 MCP 条目一键收编为受管资源（含来源标注；http 条目的请求头键名一并提取，值需重新录入） |
| **内置模板库** | 「从模板添加」一键填充常用 MCP 的启动命令（filesystem、fetch、playwright、context7、sequential-thinking、memory、time、everything，按官方文档预填）——保存后就是普通受管资源，可随意改参数 |
| **握手健康检查** | 「握手」按钮**真实握手一次**：stdio 型真实 spawn 进程 → 写 JSON-RPC `initialize` → 读响应 → 再问 `tools/list` 清点工具数 → 无论成败都杀进程收割；http 型发 Streamable HTTP initialize（兼容 JSON 与 SSE 帧响应，**携带资源里声明的请求头**）。命令经 PATH+PATHEXT 解析（npx/.cmd 均可），环境变量与请求头里的 `%VAR%`/`$VAR` 引用发送前瞬间展开，静默进程 15 秒判 timeout 而不是挂死；结果（状态/耗时/协议版本/工具数）落库常驻展示，支持批量串行测试 |
| **分发向导（三屏）** | ① 选目标 Agent（只列出「定义里声明了 MCP 来源」且能力上限高于 observe 的）→ ② **逐文件 diff 预览**（键级变更 + 行级 diff 两种视图，含 +/~/-/跳过 计数）→ ③ 执行结果 |
| **两种写入策略** | **结构合并**（JSON：只增删受管的键，用户手写的其它内容与条目一律不动）· **托管块**（TOML：标记之间就地替换，标记之外一字不改） |
| **安全默认** | 同名但**非 AgentHub 管理**的条目默认**跳过**（保留你手写条目的特有字段），需要覆盖时在向导里显式勾选 |
| **备份与回滚** | 每次写入前自动备份目标文件（毫秒级时间戳 + 冲突递增，不会互相覆盖），「历史与审计 → 配置备份」可一键回滚；回滚前还会再备份当前内容 |
| **写入状态跟踪** | 记录每个 (Agent, 文件, 节点) 上次由 AgentHub 写入的键，因此从资源库移除某个 MCP 后，下次分发会把它从配置里清理掉 —— 且只清理这一部分 |

条目形状由定义决定（`entryStyle`）：`standard`（command/args/env | url）或 `opencode`（`type=local/remote`、`command` 为数组、环境变量键名为 `environment`）。

## 回收站管理

删除不是终点 —— 所有被删对象（链接与目录）都先进回收站，侧边栏「**回收站**」页提供完整管理：

| 能力 | 说明 |
|---|---|
| **统计概览** | 条目数 / 可恢复对象数 / 占用空间 / 最早与最新条目时间 |
| **按类型筛选** | 链接 / 目录 / 混合，支持全选当前筛选结果 |
| **查看条目内容** | 逐对象列出：原位置、链接目标、大小、**是否仍可恢复**（原位置被占用会标出） |
| **恢复** | 整条恢复，或勾选其中若干对象**部分恢复**（例如从 222 个链接里只还原某几个） |
| **按期清理** | 「清理 7 天前」「清理 30 天前」，以及**清空回收站** |
| **永久删除** | 选定条目或整站永久删除 —— 走计划 → 确认流程，并明确标注**不可恢复** |

永久删除全程受保护：计划里列出每条将删除的条目与释放空间，执行时还会二次校验「只允许删除回收站目录内的内容」。

命令行检查：`agenthub --trash-json` 打印回收站统计与各条目详情（不启动界面）。

## 命令行自检入口

```bash
cd src-tauri
cargo run -- --scan-json    # 输出整机扫描快照 JSON（进度到 stderr）
cargo run -- --trash-json   # 打印回收站统计与条目详情
cargo run -- --self-test    # 在临时沙箱里跑完整 T2/T3 链路（不碰真实技能库）
```

`--self-test` 覆盖：发现 → 导入计划 → 链接导入 → 链接可解析校验 → 重复导入跳过 → 制造失效 → **重建链接** → 再失效 → 清理入回收站 → **从回收站整批还原（指向关系保留）** → 依清单恢复 → 单独删除链接入回收站并还原 → **路径别名去重** → 删除白名单拒绝 → 目录移入回收站并还原 → 越权清理拒绝 → 回收站统计/详情/部分恢复/永久删除/清空 → **配置写入（JSON 结构合并：用户内容保留、受管键清理、幂等、备份、回滚、非受管条目默认跳过、文件不存在时新建）** → **TOML 托管块（就地替换、不重复堆叠）** → **密钥保险库（DPAPI 往返、落盘无明文、掩码、多密钥、删除）** → **供应商分发（密钥注入配置、diff 掩码、用户内容保留、写前备份）** → **环境档案（落库、Skill 链接部署、幂等、跳过已有目录、可撤销清单）** → **供应商连通性测试（本地 mock HTTP 服务器：OpenAI 兜底端点、no_key、Key 回显打码、连接失败、Anthropic/Ollama 端点、空 URL）** → **快照对比（资源级新增/移除/变化、相同快照零差异、落库读回）** → **档案导出导入（原子写入、无密钥字段、同名后缀、非法文件拒绝、导出目录清单）** → **path.glob（** 跨层、段内通配、能力目录登记）** → **MCP 握手（node 模拟 stdio 服务器、%VAR% 环境变量展开、静默进程 timeout、命令缺失、http 握手、端口关闭）** → **供应商余额查询（DeepSeek /user/balance：正常解析与拆分、/v1 剥离、no_key、Key 回显打码、畸形响应、unsupported、落库往返）**（共 215 项断言）。

设置 `AGENTHUB_DATA_DIR` 可切换数据目录（便携模式与沙箱测试都依赖它）。

> CI：GitHub Actions 在 `windows-latest` 上跑 `pnpm build` + `cargo build` + `--self-test`（见 `.github/workflows/ci.yml`）。

### 2. Agent 定义 = 数据文件

每个 Agent 由一份 `config/agents/<id>.toml` 描述：去哪找、怎么判定、每条路径扮演什么角色、允许写到哪一级。

```toml
[agent]
id = "opencode"
name = "opencode"
vendor = "SST"
kind = "cli"                 # cli | ide | extension | host（host 是宿主应用，不算 Agent）

# 无法用路径表达的判定才需要显式证据
[[evidence]]
capability = "which"         # ← 内核能力 id
strength = "strong"
label = "命令行程序 opencode"
args = ["opencode"]
paths = ["%APPDATA%/npm", "%PROGRAMDATA%/npm/npm"]

# 路径角色会自动派生默认证据：install→强 / config→中 / data·skills→弱
[[paths]]
label = "主配置"
path = "~/.config/opencode/opencode.json"
role = "config"
format = "json"

[[paths]]
label = "Skills 目录"
path = "~/.config/opencode/skills"
role = "skills"
deploy = "link"              # 期望的部署方式：link | copy

[[mcp]]                      # 明确声明 MCP 在哪个文件的哪个节点，不做格式猜测
file = "~/.config/opencode/opencode.json"
root = "mcp"
format = "json"

[[provider]]                 # 供应商线索来源
file = "~/.config/opencode/opencode.json"
root = "provider"
kind = "base-url"
baseUrlKey = "options.baseURL"
modelsKey = "models"

[capabilities]
maxTier = "parse"            # 能力上限
skillMethods = ["link", "copy"]
configWrite = ["merge-keys"]
```

**判定状态**由证据强度汇总得出：

| 证据 | 强度 | 判定 |
|---|---|---|
| 命令行程序 / 全局 npm 包 / 程序安装目录 | 强 | **已安装** |
| Agent 专属配置文件（json / toml / yaml） | 中 | **仅发现配置** |
| 只有数据 / 会话 / 技能目录 | 弱 | **残留** |
| 什么都没有 | — | 未安装 |

### 3. 定义加载

```
内置定义（编译进二进制，21 个）
      ↓ 首次启动自动导出
%APPDATA%\dev.agenthub.desktop\agents\*.toml   ← 直接编辑这个目录即可接入/改行为
      ↓ 同名文件覆盖内置
运行时判定（内置 + 用户目录）
```

- 与内置逐字相同的副本**不算自定义**（不会刷告警）；内容改过才提示「已覆盖内置定义」
- 写坏的定义会**报错并回退内置**，不会让整个程序挂掉
- 未知键**直接报错**而不是静默忽略 —— 这条规则来自一次真实事故：`keys_only` 因命名规范不一致被静默忽略，导致凭据值被读进内存

## M0 已交付

| 能力 | 说明 |
|---|---|
| **配置驱动的 Agent 判定** | 21 个内置定义 + 用户目录覆盖；`kind` 区分 CLI / IDE / 扩展 / 宿主；`status` 区分已安装 / 仅配置 / 残留 |
| **能力分级内核** | 27 个基础能力、四级授权（已实现 T0/T1 共 13 个），能力目录可在界面查看与检索 |
| **Inventory Scanner** | 只读扫描：22 项工具链、Agent 目标、Skill 目录（含失效链接检测）、MCP 配置、conda/uv/venv 环境、npm/pnpm 全局包、供应商线索 |
| **仪表盘** | KPI + **环境拓扑图**（Agent ⇄ Profile 层 ⇄ 资源池，可点击跳转）+ 工具链状态 + 告警 |
| **Agent 定义与能力页** | 能力目录（按级别、被引用次数）、每个定义的证据规则 / 路径角色 / MCP 与供应商来源 / 授权上限，**可直接编辑并保存**定义 |
| **首次启动引导** | 5 步向导：欢迎 → 扫描（实时阶段可视化）→ 结果 → Profile 预告 → 完成 |
| **设置页** | 工具链探测面板（可手动指定可执行文件路径）、定义目录、扫描范围、备份保留数 |
| **只读资源页** | Skills（SKILL.md 预览 + 分类筛选 + 失效链接标记）/ MCP / npm / Python 环境 / Agent 目标（含证据链）/ 供应商线索 |
| **回收站 + 操作清单** | 独立回收站管理页（统计/筛选/内容详情/部分恢复/按期清理/永久删除/清空）、可恢复操作清单、`--trash-json` 自检 |
| **MCP 资源库与分发（T2 写入）** | 资源 CRUD + 从扫描导入 + 三屏分发向导（逐文件 diff）+ 结构合并/托管块两种策略 + 写前备份与回滚，见下节 |
| **环境档案 Profile** | 可视化组合 MCP + 供应商 + Skill，一次确认应用到多个 Agent（配置写入 + Skill 部署），见下节 |
| **供应商资源库 + 密钥保险库** | DPAPI 加密的密钥存储、供应商 CRUD 与分发（`[[providerWrite]]`）、diff 全程掩码，见下节 |
| **Skill 快捷导入 / 清理 / 重建 / 删除** | T2/T3 首批落地能力，见下节 |
| **全局任务控制台** | 底部可折叠面板，实时展示各阶段进度与逐行日志 |
| **命令面板（M4 提前落地）** | `Ctrl+K` 全局搜索直达 13 个功能页与常用操作（含重新扫描），↑↓/Enter/Esc 键盘驱动，顶栏「搜索」按钮同入口 |
| **SQLite 存储** | 设置、扫描快照、Agent 定义相关状态（M1/M2 资源表已预置） |
| **无界面自检** | `agenthub --scan-json` 输出扫描快照 JSON（也是 M4 CLI 的种子） |

> **只读保证**：M0 不修改任何 Agent 配置文件。T2/T3 写入能力在 M2/M3 落地，且必须先展示 diff。

## 技术栈

- **后端**：Rust + Tauri 2、`rusqlite`（bundled SQLite）、`toml`、`ureq`（连通性测试 / HTTP 握手）、自实现进程探测（超时保护 + 隐藏控制台窗口）
- **前端**：React 18 + TypeScript + Vite 6 + Tailwind 3 + Zustand
- **界面**：全面**扁平化朴素风格** —— 中性灰底、单一青色强调、无渐变无发光无毛玻璃、实底色 + 1px 边框 + 收敛圆角；**深浅双主题**（全部颜色经 CSS 变量映射，`html.light` 一键切换）；**降噪规范**：徽章/图标一律纯线框无底色，彩色只保留语义（状态/主操作/diff 行），无装饰动画
- **可视化**：拓扑图为手写 SVG（零依赖）；diff 视图将用 Monaco（M2）；图表将用 Recharts（M5）
- **CI**：GitHub Actions（windows-latest）：`pnpm build` + `cargo build` + `--self-test` 143 项断言

## 目录结构

```
skill-manager/
├─ src/                          # React 前端
│  ├─ components/                # Sidebar / TaskConsole / Topology / Icon / ui 基础件
│  ├─ pages/                     # 13 个功能页
│  └─ lib/                       # types（与 Rust 对应）、api、store、format、hooks
├─ src-tauri/
│  ├─ config/agents/*.toml       # ★ Agent 定义（21 个，数据而非代码）
│  └─ src/
│     ├─ capability.rs           # ★ 内核基础能力 + 能力分级目录
│     ├─ agentdef.rs             # ★ 定义加载/校验/覆盖/导出
│     ├─ handshake.rs            # ★ MCP 握手健康检查（stdio / http）
│     ├─ probe.rs                # ★ 供应商连通性测试（GET models）
│     ├─ snapdiff.rs             # ★ 快照对比（资源级差异）
│     ├─ share.rs                # ★ 档案导出/导入（自包含 JSON）
│     ├─ yaml.rs                 # 极简 YAML 块提取（MCP / 凭证结构）
│     ├─ model.rs                # 统一数据模型（camelCase）
│     ├─ util.rs                 # 路径展开、进程执行、glob 匹配、脱敏
│     ├─ store.rs                # SQLite
│     ├─ commands.rs             # Tauri 命令层
│     ├─ lib.rs                  # 状态装配 + 命令注册 + CLI 自检
│     └─ scan/                   # 侦察引擎（全部由定义驱动）
│        ├─ agents.rs            # 证据规则求值（不含 Agent 知识）
│        ├─ mcp.rs               # 按 [[mcp]] 声明提取
│        ├─ skills.rs            # 扫描 role = skills 的路径 + 失效链接检测
│        ├─ providers.rs         # 环境变量 + [[provider]] 声明
│        ├─ python_envs.rs       # conda / uv / venv
│        ├─ npm_pkgs.rs          # npm / pnpm 全局包
│        └─ executables.rs       # 工具链探测
├─ docs/design/PLAN.md           # 完整方案（v3）
└─ tools/gen_icons.py            # 图标生成（Pillow）
```

## 运行

前置：Node.js ≥ 18、pnpm、Rust 工具链（`stable-x86_64-pc-windows-msvc`）、Visual Studio C++ 生成工具、WebView2。

```bash
pnpm install
pnpm app:dev          # 开发模式：Vite + Tauri 窗口
pnpm build            # 仅类型检查 + 构建前端
pnpm app:build        # 打包发布版（MSI / NSIS）
```

无界面自检（输出扫描快照 JSON 到 stdout，进度到 stderr）：

```bash
cd src-tauri
cargo run -- --scan-json > snapshot.json
```

## 接入一个自定义 Agent

1. 在 `%APPDATA%\dev.agenthub.desktop\agents\` 新建 `<你的 id>.toml`（或复制一份内置定义改）
2. 填 `[agent]`，用 `[[paths]]` 声明路径与角色（`install` / `config` / `data` / `skills`）
3. 无法用路径表达的证据（命令行程序、npm 包）用 `[[evidence]]` 引用 `which` / `npm.global.has`
4. 有 MCP 配置就加 `[[mcp]]`（含 `root`），有供应商配置就加 `[[provider]]`
5. 用 `[capabilities] maxTier` 声明允许 AgentHub 操作到哪一级
6. 回到应用点「重新扫描」；定义有语法错误会在界面上直接报出，并自动回退内置

## 数据与隐私

- 全部数据保存在 `%APPDATA%\dev.agenthub.desktop\`（SQLite + Agent 定义）
- **API Key / 凭据永不读取值**：定义里用 `keysOnly = true` 的来源只解析键名；内核另有硬规则 —— 键名命中 `key` / `token` / `secret` / `password` 一律脱敏并归为 api-key
- 扫描全程只读

## 路线图

| 阶段 | 内容 |
|---|---|
| **M0** ✅ | 工程骨架、能力分级内核、配置驱动的 Agent 定义、Scanner、仪表盘与拓扑图、Onboarding、设置面板、只读资源页、任务控制台 |
| **M2（Skill 部分）** ✅ | T2/T3 落地到 Skill：快捷导入（本地 / Git + 代理）、一键清理失效链接、重建链接、删除（回收站）、操作清单可恢复、沙箱自检 |
| **M1** ✅ | 五类资源 CRUD、Profile 可视化组合编辑、密钥保险库（DPAPI）、供应商连通性测试、快照对比、档案导出导入 |
| M2（其余） | Provider 分发到更多 Agent（opencode）、diff 三屏推广到其它资源、Agent 配置同步审计时间线 |
| M3 | ~~MCP 握手健康检查~~ ✅；余下：uv / conda 环境创建向导、npm 声明式安装与版本锁定 |
| M3.5 | 定义的模版语法（Tera）与可视化编辑器、反向生成向导、定义导出分享 |
| M4 | ~~命令面板~~ ✅（Ctrl+K）；余下：换机迁移导入导出、CLI 完整化、浅色主题与英文界面 |
| M5 | 本地统一网关（OpenAI 兼容 proxy）：用量统计、Key 轮换、故障切换 |

详见 [docs/design/PLAN.md](docs/design/PLAN.md)。