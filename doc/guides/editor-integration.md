# Cditor 第三方编辑器接入指南

本文说明第三方 Rust/GPUI 应用如何通过 Git 仓库接入 Cditor 编辑器，加载和导出文档，监听变化，并把内容保存到文件、SQLite、HTTP API 或任意自定义存储。

第三方推荐依赖轻量组件 `cditor-gpui`，只使用其根模块公开的 `Editor`、`EditorHandle`、`EditorDocument` 和 `EditorPersistence`，不直接依赖 `CditorV2View` 或 `DocumentRuntime` 内部实现。该组件默认不会引入 Cditor 的 PostgreSQL、`sqlx`、`reqwest`、Tokio 或官方应用启动器依赖。

## 1. Git 依赖

### 1.1 固定 revision，推荐

```toml
[dependencies]
cditor-gpui = {
    git = "https://github.com/feigeCode/Cditor.git",
    package = "cditor-gpui",
    rev = "cc20fa6"
}
```

固定 revision 可以保证开发机、CI 和发布构建使用相同 API。

### 1.2 跟随 main

```toml
[dependencies]
cditor-gpui = {
    git = "https://github.com/feigeCode/Cditor.git",
    package = "cditor-gpui",
    branch = "main"
}
```

跟随 `main` 适合开发阶段。发布前建议把 `branch` 替换为实际测试过的 `rev`。

应用项目应提交 `Cargo.lock`，以锁定 Git commit 和传递依赖版本。

### 1.3 可选网络能力

默认构建支持本地图片和 Mock AI provider，但不会发起网络请求。如果宿主明确需要 OpenAI-compatible provider 或 HTTP/HTTPS 图片加载，可以启用对应 feature：

```toml
[dependencies]
cditor-gpui = {
    git = "https://github.com/feigeCode/Cditor.git",
    package = "cditor-gpui",
    rev = "cc20fa6",
    features = ["ai-openai", "remote-media"]
}
```

- `ai-openai`：启用 OpenAI-compatible provider、环境变量和 TOML 配置加载；
- `remote-media`：启用 HTTP/HTTPS 图片下载；
- 不启用 `remote-media` 时，本地路径和 `file://` 图片仍可加载，网络图片显示稳定占位。

## 2. 最小接入

```rust
use cditor_gpui::Editor;

let editor = Editor::builder()
    .document_id("document-1")
    .build(cx)?;
```

`build` 返回 `EditorHandle`。编辑器默认创建一个空 Paragraph，使用内存文档，不需要配置持久化。

在宿主 View 中显示编辑器 Entity：

```rust
div()
    .size_full()
    .child(editor.entity().clone())
```

推荐由宿主长期保存 `EditorHandle`：

```rust
use cditor_gpui::EditorHandle;

struct AppView {
    editor: EditorHandle,
}
```

## 3. 设置初始 Markdown

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .initial_markdown(
        "# 标题\n\n这是第三方应用传入的初始内容。"
    )
    .build(cx)?;
```

读取 Markdown：

```rust
let markdown = editor.get_markdown(cx)?;
```

替换当前 Markdown：

```rust
editor.set_markdown(
    "# 新文档\n\n替换后的内容",
    cx,
)?;
```

`set_markdown` 会把新内容作为干净基线，不会立即标记为 Dirty。之后的用户编辑才会增加文档版本并进入 Dirty 状态。

Markdown 是交换格式，不是完整的无损持久化格式。复杂表格样式、媒体属性、白板和部分 Block 信息应使用 `EditorDocument` JSON 保存。

## 4. 原生文档格式

读取完整文档：

```rust
let document = editor.get_document(cx)?;
```

序列化为 JSON：

```rust
let json = document.to_json()?;
```

从 JSON 恢复：

```rust
use cditor_gpui::EditorDocument;

let document = EditorDocument::from_json(&json)?;
editor.set_document(document, cx)?;
```

直接创建初始原生文档：

```rust
let document = EditorDocument::from_markdown(
    "document-1",
    "# 标题\n\n正文",
)?;

let editor = Editor::builder()
    .initial_document(document)
    .build(cx)?;
```

`EditorDocument` 包含：

```rust
pub struct EditorDocument {
    pub schema_version: u32,
    pub document_id: String,
    pub structure_version: u64,
    pub blocks: Vec<EditorBlock>,
}
```

JSON 中包含 `schema_version`。当前版本不接受未知的未来 schema，避免静默损坏文档。

## 5. EditorHandle 方法

### 5.1 Entity

```rust
let entity = editor.entity();
```

用于把编辑器作为 GPUI 子 View 渲染。正常接入不需要读取 Entity 内部字段。

### 5.2 内容

```rust
editor.set_markdown(markdown, cx)?;
let markdown = editor.get_markdown(cx)?;

editor.set_document(document, cx)?;
let document = editor.get_document(cx)?;
```

传给 `set_document` 的 `document_id` 必须与 Editor Builder 的 `document_id` 一致，否则返回 `EditorError::DocumentIdMismatch`。

### 5.3 保存和重载

```rust
editor.save(cx)?;
editor.reload(cx)?;
```

这两个方法要求配置 `EditorPersistence`。没有配置时返回 `EditorError::PersistenceNotConfigured`。

如果 Dirty 状态下调用 `reload`，编辑器会先使用 `BeforeReload` 原因保存当前版本；保存成功后才重新加载，保存失败时保留当前内存内容。

### 5.4 状态

```rust
let dirty = editor.is_dirty(cx);
let version = editor.document_version(cx);
let state = editor.save_state(cx);
```

保存状态：

```rust
pub enum EditorSaveState {
    Disabled,
    Clean,
    Dirty,
    Saving,
    SaveFailed { message: String },
}
```

`Disabled` 表示未配置持久化。即使状态是 Disabled，手动调用 `get_document`、`get_markdown`、`to_json` 仍然可用。

### 5.5 只读和焦点

```rust
editor.set_readonly(true, cx)?;
editor.focus(cx)?;
```

只读模式阻止用户编辑，但仍允许导出内容或由宿主程序调用 `set_document` 和 `set_markdown`。

`focus` 会请求编辑器在下一次渲染时获得主编辑焦点。

## 6. 手动持久化

不实现 `EditorPersistence` 也可以由宿主手动保存：

```rust
let document = editor.get_document(cx)?;
let json = document.to_json()?;
std::fs::write("document.json", json)?;
```

恢复：

```rust
let json = std::fs::read_to_string("document.json")?;
let document = EditorDocument::from_json(&json)?;
editor.set_document(document, cx)?;
```

这种方式适合：

- 宿主已经有统一保存流程；
- 保存由菜单、快捷键或关闭窗口事件触发；
- 不需要编辑器自动加载或自动保存；
- 只需要 Markdown/JSON 导入导出。

## 7. 自定义 EditorPersistence

实现以下 trait：

```rust
pub trait EditorPersistence: Send + Sync + 'static {
    fn load(
        &self,
        document_id: &str,
    ) -> Result<Option<EditorDocument>, EditorPersistenceError>;

    fn save(
        &self,
        request: EditorSaveRequest,
    ) -> Result<(), EditorPersistenceError>;
}
```

Trait 方法是同步接口，但 Cditor 会在 GPUI 后台任务中调用它们，不会在键盘输入或渲染热路径上执行第三方存储操作。

### 7.1 文件持久化示例

```rust
use std::path::PathBuf;

use cditor_gpui::{
    EditorDocument,
    EditorPersistence,
    EditorPersistenceError,
    EditorSaveRequest,
};

#[derive(Clone)]
struct FilePersistence {
    directory: PathBuf,
}

impl FilePersistence {
    fn path(&self, document_id: &str) -> PathBuf {
        self.directory.join(format!("{document_id}.json"))
    }
}

impl EditorPersistence for FilePersistence {
    fn load(
        &self,
        document_id: &str,
    ) -> Result<Option<EditorDocument>, EditorPersistenceError> {
        let path = self.path(document_id);
        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(path)
            .map_err(|error| EditorPersistenceError::new(error.to_string()))?;

        EditorDocument::from_json(&json)
            .map(Some)
            .map_err(|error| EditorPersistenceError::new(error.to_string()))
    }

    fn save(
        &self,
        request: EditorSaveRequest,
    ) -> Result<(), EditorPersistenceError> {
        std::fs::create_dir_all(&self.directory)
            .map_err(|error| EditorPersistenceError::new(error.to_string()))?;

        let json = request
            .document
            .to_json()
            .map_err(|error| EditorPersistenceError::new(error.to_string()))?;

        std::fs::write(self.path(&request.document_id), json)
            .map_err(|error| EditorPersistenceError::new(error.to_string()))
    }
}
```

接入：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .initial_markdown("# 新文档")
    .persistence(FilePersistence {
        directory: "./documents".into(),
    })
    .build(cx)?;
```

加载规则：

1. `EditorPersistence::load` 返回 `Some(document)`：使用持久化文档；
2. 返回 `None`：使用 Builder 的初始 Markdown/Document；
3. 没有初始内容：使用空 Paragraph；
4. 返回错误：保留当前内存内容并发送 `LoadFailed` 事件。

## 8. 自动保存

```rust
use std::time::Duration;

let editor = Editor::builder()
    .document_id("document-1")
    .persistence(FilePersistence {
        directory: "./documents".into(),
    })
    .autosave(Duration::from_secs(3))
    .build(cx)?;
```

自动保存使用 debounce：连续输入会刷新计时，只保存最后一个稳定版本。

保存操作包含版本号：

```rust
pub struct EditorSaveRequest {
    pub document_id: String,
    pub document: EditorDocument,
    pub document_version: u64,
    pub reason: EditorSaveReason,
}
```

保存原因：

```rust
pub enum EditorSaveReason {
    Manual,
    Autosave,
    BeforeReload,
    BeforeClose,
}
```

如果版本 5 正在保存时用户继续编辑到版本 6，版本 5 保存成功不会把编辑器错误标记为 Clean；当前状态仍然是 Dirty，随后可继续自动保存版本 6。

## 9. 编辑器事件

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .on_event(|event| {
        println!("editor event: {event:?}");
    })
    .build(cx)?;
```

事件类型：

```rust
pub enum EditorEvent {
    Ready { document_id: String },
    Changed {
        document_id: String,
        document_version: u64,
    },
    SaveStateChanged { state: EditorSaveState },
    Saved {
        document_id: String,
        document_version: u64,
        reason: EditorSaveReason,
    },
    SaveFailed {
        document_id: String,
        document_version: u64,
        message: String,
    },
    LoadFailed {
        document_id: String,
        message: String,
    },
}
```

`Changed` 只对应文档内容或结构变化。滚动、选区、焦点、布局测量和调试浮层更新不会把文档标记为 Dirty。

事件回调在编辑器内部状态完成转换之后调用。回调中应避免执行长时间阻塞操作；耗时业务应转交宿主自己的后台任务。

## 10. 完整示例

```rust
use std::time::Duration;

use cditor_gpui::{Editor, EditorHandle};
use gpui::*;

struct AppView {
    editor: EditorHandle,
}

impl AppView {
    fn new(cx: &mut Context<Self>) -> Self {
        let editor = Editor::builder()
            .document_id("document-1")
            .initial_markdown("# Cditor\n\n开始编辑。")
            .persistence(FilePersistence {
                directory: "./documents".into(),
            })
            .autosave(Duration::from_secs(3))
            .on_event(|event| {
                println!("{event:?}");
            })
            .build(cx)
            .expect("build editor");

        Self { editor }
    }
}

impl Render for AppView {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .child(self.editor.entity().clone())
    }
}
```

## 11. 错误处理

所有接入操作使用 `EditorError`：

```rust
match editor.save(cx) {
    Ok(()) => {}
    Err(error) => eprintln!("save scheduling failed: {error}"),
}
```

常见错误：

- `NotReady`：编辑器尚未准备好；
- `PersistenceNotConfigured`：调用了需要持久化实现的方法；
- `InvalidDocument` / `InvalidJson`：文档数据不合法；
- `UnsupportedSchemaVersion`：JSON 来自不受支持的未来格式；
- `IncompleteDocument`：runtime 尚未加载所有 Block payload，不能无损导出；
- `DocumentIdMismatch`：设置了属于其他文档 ID 的快照；
- `Persistence`：第三方 load/save 返回错误。

后台保存失败不会丢弃当前内存内容。错误通过 `SaveFailed` 状态和事件暴露，第三方可以提示用户并重试。

## 12. 当前边界

- 当前接入 API 面向 Rust/GPUI 宿主，没有 C、JavaScript 或其他语言绑定；
- `EditorPersistence` 是同步 trait，但始终由 Cditor 后台任务调用；
- 原生 JSON 是无损持久化格式，Markdown 不能表达所有复杂 Block 信息；
- `EditorHandle` 提供稳定的加载、导出、保存、重载、只读和焦点接口，没有暴露全部底层编辑命令；
- 第三方不应依赖 `CditorV2View` 私有字段或 `DocumentRuntime` 内部状态；
- 固定 Git revision 是当前推荐的版本管理方式。

## 13. 最小结论

依赖：

```toml
cditor-gpui = {
    git = "https://github.com/feigeCode/Cditor.git",
    package = "cditor-gpui",
    rev = "cc20fa6"
}
```

创建：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .build(cx)?;
```

渲染：

```rust
.child(editor.entity().clone())
```

保存：

```rust
let json = editor.get_document(cx)?.to_json()?;
```

或者配置持久化与自动保存：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .persistence(MyPersistence::new())
    .autosave(Duration::from_secs(3))
    .build(cx)?;
```
