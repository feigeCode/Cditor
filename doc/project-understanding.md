# Cditor 项目理解与开发者导览

> 本文基于 2026-07-13 的 `main` 分支源码、Cargo 配置、数据库迁移、开发脚本及现有架构文档整理，目标是帮助新接手的开发者快速建立对项目现状的准确理解。
>
> 当前代码版本：`b8117b9`（`Update README.md`）。历史设计材料集中在 `doc/archive/`，只用于理解演进过程，不应作为当前实现依据。

## 1. 项目定位

Cditor 是一个使用 Rust 和 GPUI 构建的原生桌面块式富文本编辑器。项目面向大规模文档场景，核心目标包括：

- 支持十万级 Block 的文档结构；
- 通过窗口化渲染和虚拟滚动控制 UI 与布局成本；
- 支持跨 Block 富文本编辑、选区、剪贴板、撤销重做和结构编辑；
- 支持表格、代码、图片、Mermaid、白板等复杂 Block；
- 通过 PostgreSQL 保存结构、内容、布局缓存、事务、资源和恢复队列；
- 在不阻塞输入热路径的前提下集成流式 AI 能力。

项目最重要的架构原则是：

> UI 只是当前视口状态的投影；文档、选区、布局高度和全局滚动状态必须存在于编辑器内核中，不能依赖 GPUI Entity 的生命周期。

这意味着屏幕外的 Block 即使没有对应 UI Entity，仍然可以参与搜索、复制、撤销、保存和滚动定位。

## 2. 技术栈与工程形态

| 领域 | 当前技术 |
| --- | --- |
| 语言与构建 | Rust 2024 Edition、Cargo workspace、resolver 3 |
| 桌面 UI | GPUI，固定到 Zed 仓库提交 `1d217ee39d381ac101b7cf49d3d22451ac1093fe` |
| 数据库 | PostgreSQL 16、SQLx 0.8、Tokio |
| 序列化 | Serde、Serde JSON |
| 网络与 AI | Reqwest 0.12、OpenAI-compatible provider、DeepSeek 风格配置 |
| 文本处理 | Unicode Segmentation，显式处理 UTF-8、UTF-16 和 grapheme 边界 |
| 富媒体 | `image`、原生 Mermaid 渲染、独立 `ding-board` 白板 crate |
| 许可证 | GPL-3.0-or-later；仓库同时保留 Apache 与第三方声明文件 |

根 workspace 当前包含 8 个 crate，默认构建成员是 `crates/app`，统一版本为 `0.1.0`。

## 3. Workspace 分层

```text
                         ┌───────────────┐
                         │  cditor-app   │  GPUI 窗口、渲染、输入、浮层、保存桥接
                         └───────┬───────┘
                                 │
               ┌─────────────────┼──────────────────┐
               ▼                 ▼                  ▼
       cditor-runtime      cditor-editor       ding-board
       编辑状态与编排       视口/滚动算法         独立白板
               │                 │
        ┌──────┼──────────┐      │
        ▼      ▼          ▼      ▼
 cditor-core  cditor-ai  cditor-storage
 领域内核      AI Provider  存储抽象
                            │
                            ▼
                  cditor-storage-postgres
                     PostgreSQL 实现
```

实际 Cargo 依赖中，`cditor-runtime` 还直接依赖 `cditor-storage-postgres`，用于 PostgreSQL 冷启动与兼容路径。这是当前已知的分层例外：未来新增存储后端时，应优先依赖 `cditor-storage` trait，避免继续扩大 runtime 对具体数据库的耦合。

### 3.1 `cditor-core`

路径：`crates/core`

这是无 GPUI、无 SQLx 的领域内核，主要负责：

- Block 类型、Block 属性、列表信息、拖拽状态；
- RichText 文档、inline runs、marks、Markdown 导入导出；
- 表格结构、样式、剪贴板与结构操作；
- DocumentIndex、VisibleDocumentIndex；
- 文档级 Selection、Transaction、Undo、文本偏移转换；
- BlockHeightIndex、PageLayoutIndex 和布局模型；
- ID、版本号及大型演示数据。

新增领域规则应优先放在这里，而不是写入 GPUI View。

### 3.2 `cditor-editor`

路径：`crates/editor`

名称容易造成误解：该 crate 不是 UI 编辑器，而是一组与 GPUI 无关的视口算法，主要包括：

- `VirtualScrollState` 和全局 offset 映射；
- 滚轮累积、滚动条拖动、anchor restore；
- 高度修正管线；
- RenderWindow、WindowPlanner、两阶段 window commit；
- hit test 模型；
- debug overlay view model；
- trace event log 和滚动回放回归门禁。

它依赖 `cditor-core`，但不依赖 `cditor-app`。

### 3.3 `cditor-runtime`

路径：`crates/runtime`

runtime 是系统的编辑编排中心，`DocumentRuntime` 聚合并协调：

- 文档结构、payload、选区、焦点和可见投影；
- 文本编辑、IME composition、Markdown 快捷输入；
- 表格编辑、选择、导航、复制粘贴、尺寸与顺序变更；
- 结构插入、删除、移动和 undo/redo；
- payload window、媒体缓存、查询索引与内容安全策略；
- virtual scroll、窗口投影和高度回写；
- 主线程预算、布局调度、worker lane 和异步版本控制；
- AI 请求、流式预览和应用结果。

该层不拥有应用窗口或具体 GPUI 组件。它对 UI 输出 `EditorViewProjection`，使 UI 成为 runtime 状态的投影。

### 3.4 `cditor-storage`

路径：`crates/store`

通用存储层提供：

- 文档、payload、布局、事务、资源等存储契约；
- 布局缓存 schema 与恢复策略；
- 高度写入 debounce；
- 乐观持久化状态机；
- dirty block pin、失败恢复队列和关闭保护报告。

乐观保存使用 `persisted_version`、`memory_version` 和 `saving_version` 区分内存最新状态与已落盘状态。保存旧版本成功时，如果用户已经继续编辑到更新版本，Block 仍保持 Dirty，不会被错误标记为 Clean。

### 3.5 `cditor-storage-postgres`

路径：`crates/store-postgres`

它实现具体的 PostgreSQL 存储能力：

- 连接池、迁移、健康检查和同步 runtime 桥接；
- document index snapshot 和 payload window 加载；
- layout/page cache；
- edit transaction；
- 全文检索；
- asset 与 block-asset 关系；
- persistence queue、crash recovery 和 runtime snapshot；
- sync outbox、client state 和 tombstone；
- 大型演示文档 seed。

### 3.6 `cditor-app`

路径：`crates/app`

这是最终组装层，也是默认二进制所在位置，负责：

- 创建 GPUI Application 和 1200×800 主窗口；
- 把 `Cditor` builder 转换为具体冷启动方案；
- 渲染文档 surface、Block、表格、文本和骨架屏；
- 处理键盘、鼠标、IME、剪贴板和平台输入；
- 管理 slash menu、格式工具栏、toast、图片预览等 overlay；
- 管理 Mermaid 缓存、白板缩略图和白板编辑会话；
- 将 runtime 的 dirty 状态桥接到 PostgreSQL autosave；
- 展示保存状态、加载失败和关闭保护。

`Cditor` 同时是可嵌入 API，可以通过 builder 选择 Demo、LargeDemo、Memory、PostgreSQL URL/Pool 或 Cloud 配置。

### 3.7 `cditor-ai`

路径：`crates/ai`

提供统一 `AiProvider` 接口、OpenAI-compatible 实现、Mock Provider、取消令牌和有界流式 channel。没有 API key 时应用仍可运行，并回退到 mock provider。

### 3.8 `ding-board`

路径：`crates/ding-board`

独立、可嵌入的无限画布白板。它拥有自己的场景模型、相机、渲染、输入和 JSON 序列化，不反向依赖 Cditor core；Cditor 通过 app 层将白板作为一种 Block 集成。

## 4. 应用启动链

程序入口是 `crates/app/src/main.rs`：

```text
main
  → gpui_platform::application()
  → 创建 1200×800 窗口
  → cditor_from_env()
  → Cditor::build_view()
  → CditorV2View
```

### 4.1 二进制启动模式

`cditor_from_env()` 的选择顺序是：

1. `CDITOR_LARGE_DEMO=true`：创建大型混合演示文档；
2. 否则 `CDITOR_SMALL_DEMO=true`：创建普通 Demo；
3. 否则：显式选择 Memory 后端；
4. 如果存在 `CDITOR_DATABASE_URL`，再覆盖为 PostgreSQL 后端，文档 ID 默认为 1。

这里存在一个容易忽略的差异：`Cditor::new()` 的 API 默认后端是 Demo，但仓库自带二进制在没有任何环境变量时会调用 `.memory()`，因此打开的是空内存文档。

### 4.2 PostgreSQL 冷启动

PostgreSQL 模式不会阻塞 GPUI 主线程：

```text
创建 Loading View
  → background_spawn 加载 PostgreSQL runtime
  → 加载结构、payload 与缓存
  → 构造 DocumentRuntime
  → 回到 View 更新 runtime
  → 安装 PostgresPersistenceTarget
```

加载失败会切换为错误状态，而不是让窗口创建失败。

### 4.3 Cloud 状态

API 已暴露 `with_cloud_endpoint()`，当前源码会创建 Cloud loading view，但在本次检查范围内没有看到对应的云端加载完成链路。因此它更接近预留接口，不能按已完整实现的后端对待。

## 5. 核心运行时数据流

### 5.1 输入与编辑

```text
平台键盘/鼠标/IME事件
  → app 输入适配层
  → DocumentRuntime 编辑命令
  → core transaction 修改内存真相
  → 更新 selection / focus / version
  → 标记局部 layout dirty
  → 生成新的 EditorViewProjection
  → GPUI 重新渲染当前窗口
```

单字符输入热路径禁止同步执行数据库 IO、全量 payload 加载、全局 reflow 或等待后台任务。高成本工作进入布局调度器、worker lane 或持久化队列。

IME composition 独立追踪 composing 状态、候选框几何与目标 Block；编辑中的 Block 会被 pin，避免窗口切换时被回收。

### 5.2 大文档渲染与滚动

大文档并不把所有 Block 同时实例化为 GPUI Entity：

```text
DocumentIndex / VisibleDocumentIndex
  → BlockHeightIndex / PageLayoutIndex
  → VirtualScrollState
  → WindowPlanner 计算当前 render window
  → runtime 生成窗口投影
  → app 只渲染窗口内 Block
  → 实测高度回写
  → anchor restore 修正视口
```

远处未测量内容可使用估算高度；靠近视口后逐步替换为精确高度。全局坐标与局部 GPUI 坐标分离，避免十万级文档累计高度直接进入低精度绘制坐标。

窗口切换采用准备与提交分离的思路，防止半加载状态造成空白、焦点丢失或选区跳动。异步布局结果携带 generation、content version、layout version 和 width bucket；过期结果会被丢弃。

### 5.3 投影与 UI

`EditorViewProjection` 是 runtime 到 app 的主要只读边界，包含当前窗口的 Block 快照、选区、表格视图状态、AI 预览等 UI 所需信息。

GPUI 层可以缓存文本 layout、表格 cell layout、Mermaid 渲染和白板缩略图，但这些缓存都不是文档真相。

### 5.4 保存与恢复

```text
内存事务提交
  → Block memory_version 增长并标记 Dirty
  → autosave / 显式保存创建持久化任务
  → PostgreSQL 写入 payload、结构、事务与相关缓存
  → 成功：推进 persisted_version
  → 失败：进入 SaveFailed 和 recovery queue，Block 保持 pin
```

数据库还提供 runtime snapshot、persistence queue、sync outbox 和 tombstone，为崩溃恢复、重试与未来同步提供基础。

## 6. PostgreSQL 数据模型

初始迁移位于 `crates/store-postgres/migrations/0001_initial.sql`。主要表组如下：

| 表组 | 用途 |
| --- | --- |
| `workspaces`、`documents`、`document_tree` | 工作区、文档元数据与文档树 |
| `blocks`、`block_attrs`、`block_payloads` | Block 结构、属性和正文 payload |
| `block_code_meta` | 代码块语言、行数、折叠与语法版本 |
| `block_tables`、`block_table_rows`、`block_table_cells` | 表格结构和单元格内容 |
| `assets`、`block_assets` | 媒体资源、稳定尺寸与 Block 绑定 |
| `block_layout`、`page_layout` | Block/Page 高度缓存、置信度和布局版本 |
| `document_index_snapshot` | 大文档可见索引快照 |
| `edit_transactions`、`undo_snapshots` | 编辑事务及大型撤销数据 |
| FTS 相关表 | 服务端全文检索与增量更新 |
| `persistence_queue`、runtime snapshot | 持久化重试与崩溃恢复 |
| sync outbox、client state、tombstone | 同步队列、客户端进度与删除传播 |

结构与 payload 被拆开保存，使冷启动和滚动可以先加载轻量索引，再按窗口加载重内容。布局缓存带结构版本、内容版本、宽度 bucket、主题/字体/缩放等校验字段，旧缓存只能作为提示，不能覆盖新的精确测量。

## 7. 已实现的主要产品能力

从当前源码模块和测试覆盖看，项目已包含：

- Paragraph、Heading、Quote、Callout、Todo、List、Toggle、Code 等常规 Block；
- 富文本 marks、Markdown 导入导出和增量 Markdown 输入；
- 跨 Block 选区、复制、剪切、粘贴、结构删除、移动和撤销重做；
- CJK、Emoji、组合字符、UTF-16 平台偏移与 IME composition；
- 表格单元格编辑、多选、复制粘贴、导航、样式、resize、reorder 和横向滚动；
- 图片/文件类媒体 Block、图片加载与预览；
- Mermaid 渲染及源码显示切换；
- 白板 Block、缩略图缓存和独立白板编辑器；
- slash menu、格式工具栏、代码语言编辑和多类 overlay；
- 内联 AI prompt、流式 preview、取消和替换/插入应用模式；
- PostgreSQL 文档、payload、布局、事务、搜索、资源、恢复和同步队列；
- 大文档打开、滚动、编辑、结构修改的模型化 acceptance suite 和 trace replay。

“已存在模块与自动测试”不等于所有生产场景均完成真实长时间性能验收。涉及 10 万 Block、超大表格、图片密集文档、远程数据库和平台 IME 的场景仍应在目标操作系统上进行人工和性能验收。

## 8. 配置与运行

### 8.1 基础运行

```bash
cargo run -p cditor-app
CDITOR_SMALL_DEMO=1 cargo run -p cditor-app
CDITOR_LARGE_DEMO=1 cargo run -p cditor-app
```

### 8.2 本地 PostgreSQL

```bash
docker compose up -d postgres
./scripts/dev/run_editor.sh
```

默认开发连接为：

```text
postgres://cditor:cditor@localhost:5432/cditor_dev
```

### 8.3 重要环境变量

| 变量 | 含义 |
| --- | --- |
| `CDITOR_DATABASE_URL` | 启用 PostgreSQL 后端 |
| `CDITOR_DOCUMENT_ID` | PostgreSQL 文档 ID，默认 1 |
| `CDITOR_WORKSPACE_ID` | 工作区 ID |
| `CDITOR_SMALL_DEMO` / `CDITOR_LARGE_DEMO` | 选择演示数据规模 |
| `CDITOR_READONLY` | 只读模式 |
| `CDITOR_DEBUG_OVERLAY` | 显示布局、视口和滚动调试信息 |
| `CDITOR_PAYLOAD_WINDOW_SIZE` | payload 窗口大小，最小为 1 |
| `CDITOR_SEED_LARGE_DEMO` | 向 PostgreSQL 写入大型演示数据 |
| `CDITOR_SEED_LARGE_DEMO_BLOCKS` | seed 的 Block 数量 |
| `CDITOR_FORCE_RESEED_LARGE_DEMO` | 强制重新生成演示数据 |
| `CDITOR_TRACE_INPUT` | 输入与 IME trace |
| `CDITOR_TRACE_TABLE` | 表格交互 trace |
| `CDITOR_TRACE_MARKDOWN` | Markdown 与剪贴板 trace |

### 8.4 AI 配置

非敏感默认值位于 `config/ai.toml`。API key 通过 `CDITOR_AI_API_KEY`、兼容的 OpenAI 环境变量或本地 `.env` 提供，不应提交到仓库。

配置优先级为：进程环境变量 → 本地 `.env` → TOML 配置 → 内置默认值。

## 9. 测试和质量门禁

常用命令：

```bash
cargo test --workspace
./scripts/dev/check_structure.sh
./scripts/dev/check_workspace.sh
```

`check_workspace.sh` 顺序执行：

1. 目录与源码结构检查；
2. `cargo fmt --all -- --check`；
3. `cargo check --workspace`；
4. `cargo test --workspace`。

结构规则要求除白板模块外的 Rust 源文件不超过 700 行，同时检查废弃 `crates/engine` 路径和系统元数据文件。

### 9.1 本次验证结果

2026-07-13 在当前工作区执行：

```text
./scripts/dev/check_structure.sh
→ Structure checks passed.

cargo test --workspace
→ 877 passed, 58 ignored（17 suites）
```

ignored 测试主要包含需要外部 PostgreSQL 或高成本大型数据集的集成场景。标准 workspace 测试通过并不能替代这些测试。

### 9.2 PostgreSQL 集成测试

```bash
docker compose up -d postgres_test
export CDITOR_TEST_DATABASE_URL='postgres://cditor:cditor@localhost:5433/cditor_test'
cargo test -p cditor-storage-postgres -- --ignored
cargo test -p cditor-runtime -- --ignored
cargo test -p cditor-app --lib -- --ignored
```

部分 ignored 测试会构造或加载十万级 Block，执行时间和资源消耗明显高于普通单元测试。

## 10. 当前架构风险与接手注意事项

### 10.1 runtime 对 PostgreSQL 的直接依赖

理想边界是 runtime 只依赖存储 trait，但当前冷启动兼容代码使其直接依赖 `cditor-storage-postgres`。扩展 SQLite、云存储或其他后端前，应先明确是否抽离该依赖。

### 10.2 API 默认值与二进制默认行为不同

`Cditor::new()` 默认 Demo，`cargo run -p cditor-app` 在无环境变量时则使用 Memory。调试“为什么没有演示内容”时应先检查这一差异。

### 10.3 Cloud 后端是预留路径

当前有 Cloud 枚举和 loading UI，但未形成与 PostgreSQL 等价的完整加载、保存和错误恢复链路。

### 10.4 旧架构文档可能描述历史状态

`doc/large-document-rich-text-implementation-status.md` 中仍保留“尚未接真实 GPUI/SQLite”等早期描述，而当前仓库已经具有真实 GPUI app 和 PostgreSQL 实现。判断现状时应以 Cargo workspace、当前源码和近期测试为准。

### 10.5 大文档正确性依赖多个版本不变量

异步布局、payload window、结构修改和高度缓存都依赖 generation/version 校验。新增后台任务时如果遗漏 `content_version`、`layout_version`、width bucket 或结构版本检查，容易造成高度回跳、旧结果覆盖新内容或选区漂移。

### 10.6 UI 缓存不能成为业务真相

文本 layout、表格 cell layout、Mermaid 缓存、白板缩略图和 GPUI Entity 都可以被销毁。复制、保存、搜索、撤销和跨页选区必须从 core/runtime/store 读取。

### 10.7 输入路径不能引入同步重活

任何触及单字符输入和 IME 的改动，都应检查是否同步触发数据库、网络、全量 shaping、全页 reflow 或远端资源 decode。

### 10.8 自动化验证仍有外部依赖缺口

58 个 ignored 测试没有在标准门禁中执行；另外，真实平台 IME、长时间滚动 soak、远程 PostgreSQL、GPU/字体差异仍需要人工或专项 CI 覆盖。

## 11. 推荐阅读顺序

新开发者建议按以下顺序阅读：

1. `README.md`：功能、命令和整体分层；
2. 本文：建立当前项目地图；
3. `crates/app/src/main.rs` 与 `crates/app/src/api/cditor.rs`：理解启动和嵌入 API；
4. `crates/runtime/src/document_runtime/`：理解编辑器状态与命令编排；
5. `crates/core/src/document/`、`edit/`、`rich_text/`、`layout/`：理解领域真相；
6. `crates/editor/src/scroll/` 与 `window/`：理解虚拟滚动和窗口化；
7. `crates/app/src/gui/app/` 与 `gui/block/`：理解 GPUI 投影和交互；
8. `crates/store/src/traits.rs` 与 PostgreSQL migration/stores：理解持久化边界；
9. `doc/large-document-rich-text-architecture.md`：理解大文档设计背景；
10. `doc/plans/`、`doc/refactor/`：查看待办和正在推进的重构；
11. `doc/archive/`：仅在追溯历史迁移决策时阅读。

## 12. 按功能定位代码

| 要修改的功能 | 首选位置 |
| --- | --- |
| 新 Block 类型或 payload schema | `crates/core/src/block`、`crates/core/src/rich_text` |
| 文档结构、选区和事务 | `crates/core/src/document`、`crates/core/src/edit` |
| 高度索引和布局模型 | `crates/core/src/layout` |
| 虚拟滚动、anchor、窗口规划 | `crates/editor/src/scroll`、`crates/editor/src/window` |
| 编辑行为和运行时状态 | `crates/runtime/src/document_runtime`、`crates/runtime/src/editing` |
| 投影、调度和性能预算 | `crates/runtime/src/projection`、`crates/runtime/src/scheduling` |
| 通用存储契约 | `crates/store/src` |
| PostgreSQL schema 和查询 | `crates/store-postgres/migrations`、`crates/store-postgres/src/stores` |
| GPUI Block 展示 | `crates/app/src/gui/block` |
| 键盘、鼠标、IME | `crates/app/src/gui/input`、`crates/app/src/gui/app/input` |
| 浮层和弹出交互 | `crates/app/src/gui/overlay` |
| 保存 UI 与 PostgreSQL 桥接 | `crates/app/src/gui/persistence`、`gui/app/persistence_bridge.rs` |
| AI Provider | `crates/ai/src` |
| 白板自身能力 | `crates/ding-board` |
| 白板与 Cditor 的集成 | `crates/app/src/gui/block/whiteboard` |

## 13. 总结

Cditor 当前不是单一 View 驱动的普通富文本控件，而是一个围绕“大文档内核 + 窗口化投影 + 异步持久化”构建的编辑器系统：

- `core` 定义稳定的领域真相；
- `editor` 解决大坐标、滚动和窗口规划；
- `runtime` 组织编辑、投影、调度和异步任务；
- `app` 把投影映射为 GPUI 界面并接入平台输入；
- `storage` 与 `store-postgres` 保证内容、布局和恢复状态可持久化；
- `ai` 与 `ding-board` 作为相对独立能力被最终组装。

接手项目时最需要持续守住三条边界：UI 不是文档真相；输入热路径不能同步阻塞；异步结果必须经过完整版本校验。只要这三条不变量不被破坏，功能扩展通常可以沿现有 crate 职责自然落位。
