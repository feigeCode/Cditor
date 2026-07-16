# 三方宿主 AI Provider 与模型切换集成指南

本文说明第三方 Rust/GPUI 宿主如何把自己的 AI 网关、模型注册表和流式生成能力接入 Cditor，并由 Cditor 的 AI 面板显示和切换模型。

三方接入遵循以下边界：

- 宿主拥有模型、鉴权、配额、路由、审计和实际网络请求。
- Cditor 拥有光标/选区上下文、AI 输入面板、流式预览、过期结果保护和文档事务。
- Cditor 把用户当前选择的 `model_id` 放进每次 `AiRequest`，宿主据此路由模型。
- AI 结果先进入预览；用户接受后才通过 Undo/Redo、Dirty 和保存链路写入文档。

## 1. 公共类型

宿主只依赖 `cditor-gpui` 根模块即可实现 Provider：

```rust
use cditor_gpui::{
    AiCancellationToken,
    AiModelDescriptor,
    AiProvider,
    AiProviderError,
    AiRequest,
    AiStreamEvent,
    AiStreamSender,
    AiTaskKind,
};
```

也可以通过 `cditor_gpui::ai` 使用底层 AI crate 的完整导出。

## 2. 模型目录

宿主通过 `AiProvider::models()` 返回 AI 面板可选择的模型：

```rust
pub struct AiModelDescriptor {
    pub id: String,
    pub display_name: String,
    pub provider_name: String,
    pub description: Option<String>,
}
```

字段语义：

| 字段 | 说明 |
| --- | --- |
| `id` | 稳定且唯一的模型路由 ID，会原样放入 `AiRequest::model_id` |
| `display_name` | 下拉列表主标题，例如 `CPA / gpt-5.5` |
| `provider_name` | 下拉列表第二行的提供方，例如 `OpenAI Compatible` |
| `description` | 可选说明，例如 `正式模型`、`测试模型`、`本地模型` |

示例：

```rust
fn models(&self) -> Vec<AiModelDescriptor> {
    vec![
        AiModelDescriptor::new(
            "cpa:gpt-5.5",
            "CPA / gpt-5.5",
            "OpenAI Compatible",
        )
        .with_description("正式模型"),

        AiModelDescriptor::new(
            "cpa:gpt-5.6-sol",
            "CPA / gpt-5.6-sol",
            "OpenAI Compatible",
        )
        .with_description("正式模型"),

        AiModelDescriptor::new(
            "navop:deepseek-v4-flash",
            "OnetCli AI / deepseek-v4-flash",
            "Navop",
        )
        .with_description("正式模型"),

        AiModelDescriptor::new(
            "deepseek:deepseek-v4-flash",
            "DeepSeek / deepseek-v4-flash",
            "DeepSeek",
        )
        .with_description("正式模型"),

        AiModelDescriptor::new(
            "ollama:qwen3-14b",
            "本地 ollama / qwen3:14b",
            "Ollama",
        )
        .with_description("正式模型"),
    ]
}
```

模型 `id` 不必等于服务端真实 model name。宿主可以使用带命名空间的稳定 ID，再在自己的路由层映射实际模型：

```text
cpa:gpt-5.5              -> CPA gateway / gpt-5.5
deepseek:v4-flash        -> DeepSeek API / deepseek-v4-flash
ollama:qwen3-14b         -> http://127.0.0.1:11434 / qwen3:14b
```

不要仅用 `gpt-5.5` 作为全局 ID，因为不同 Provider 可能存在同名模型。

## 3. 实现宿主 Provider

完整示例：

```rust
use std::sync::Arc;

use cditor_gpui::{
    AiCancellationToken,
    AiModelDescriptor,
    AiProvider,
    AiProviderError,
    AiRequest,
    AiStreamEvent,
    AiStreamSender,
    AiTaskKind,
};

pub trait HostAiService: Send + Sync {
    fn stream(
        &self,
        request: HostAiRequest,
        on_delta: &mut dyn FnMut(&str) -> Result<(), HostAiError>,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<(), HostAiError>;
}

pub struct HostAiProvider {
    service: Arc<dyn HostAiService>,
}

impl HostAiProvider {
    pub fn new(service: Arc<dyn HostAiService>) -> Self {
        Self { service }
    }
}

impl AiProvider for HostAiProvider {
    fn id(&self) -> &str {
        "host-ai"
    }

    fn models(&self) -> Vec<AiModelDescriptor> {
        vec![
            AiModelDescriptor::new(
                "cpa:gpt-5.5",
                "CPA / gpt-5.5",
                "OpenAI Compatible",
            )
            .with_description("正式模型"),
            AiModelDescriptor::new(
                "deepseek:v4-flash",
                "DeepSeek / deepseek-v4-flash",
                "DeepSeek",
            )
            .with_description("正式模型"),
            AiModelDescriptor::new(
                "ollama:qwen3-14b",
                "本地 ollama / qwen3:14b",
                "Ollama",
            )
            .with_description("本地模型"),
        ]
    }

    fn default_model_id(&self) -> Option<String> {
        Some("deepseek:v4-flash".to_owned())
    }

    fn stream(
        &self,
        request: AiRequest,
        sender: AiStreamSender,
        cancellation: AiCancellationToken,
    ) -> Result<(), AiProviderError> {
        let request_id = request.request_id;
        let model_id = request
            .model_id
            .clone()
            .ok_or_else(|| AiProviderError::Request("未选择 AI 模型".to_owned()))?;

        let host_request = HostAiRequest {
            model_id,
            task: match request.task {
                AiTaskKind::InlineCompletion => HostAiTask::ContinueWriting,
                AiTaskKind::RewriteSelection => HostAiTask::RewriteSelection,
                AiTaskKind::RewriteBlocks => HostAiTask::RewriteBlocks,
            },
            instruction: request.instruction,
            selected_text: request.selected_text,
            prefix: request.prefix,
            suffix: request.suffix,
        };

        self.service
            .stream(
                host_request,
                &mut |delta| {
                    if cancellation.is_cancelled() {
                        return Err(HostAiError::Cancelled);
                    }
                    sender
                        .send_blocking(AiStreamEvent::Delta {
                            request_id,
                            text: delta.to_owned(),
                        })
                        .map_err(|_| HostAiError::ReceiverClosed)
                },
                &|| cancellation.is_cancelled(),
            )
            .map_err(|error| {
                if cancellation.is_cancelled() {
                    AiProviderError::Cancelled
                } else {
                    AiProviderError::Request(error.to_string())
                }
            })?;

        if cancellation.is_cancelled() {
            return Err(AiProviderError::Cancelled);
        }

        sender
            .send_blocking(AiStreamEvent::Done { request_id })
            .map_err(|_| AiProviderError::ChannelClosed)
    }
}
```

`AiProvider::stream` 是阻塞接口，但 Cditor 始终在 GPUI 后台执行器调用，不会阻塞渲染线程。宿主若只有 async SDK，可以在 Provider 内使用宿主已有 runtime 的同步桥接，但不能从 Provider 直接修改 GPUI View。

## 4. 请求内容

每次请求的数据结构：

```rust
pub struct AiRequest {
    pub request_id: u64,
    pub task: AiTaskKind,
    pub model_id: Option<String>,
    pub instruction: String,
    pub selected_text: String,
    pub prefix: String,
    pub suffix: String,
}
```

宿主最重要的是使用 `model_id` 路由：

```rust
match request.model_id.as_deref() {
    Some("cpa:gpt-5.5") => call_cpa("gpt-5.5", request),
    Some("deepseek:v4-flash") => call_deepseek("deepseek-v4-flash", request),
    Some("ollama:qwen3-14b") => call_ollama("qwen3:14b", request),
    Some(other) => Err(format!("unknown model: {other}")),
    None => Err("model is not selected".to_owned()),
}
```

`prefix` 和 `suffix` 是光标/选区附近的上下文，不是整个文档。需要全文 RAG、知识库、工具调用或企业上下文时，由宿主 AI 服务自行扩展。

## 5. 流事件

Provider 向 Cditor 发送：

```rust
pub enum AiStreamEvent {
    Delta {
        request_id: u64,
        text: String,
    },
    Done {
        request_id: u64,
    },
    Error {
        request_id: u64,
        message: String,
    },
}
```

正常顺序：

```text
Delta("第一段")
Delta("第二段")
Done
```

要求：

- 所有事件使用原始 `request_id`。
- `Done` 或 `Error` 是终止事件。
- 取消后尽快停止网络读取和 Delta 发送。
- 返回需要插入/替换的正文，不要自动附加“以下是结果”等解释。

Provider 也可以直接返回 `AiProviderError::Request`。Cditor 会把非取消错误转换成当前请求的 Error 流事件。

## 6. 轻量 `Editor` 接入

值类型 Provider：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .initial_markdown("# Title")
    .ai_provider(HostAiProvider::new(host_ai_service))
    .build(cx)?;
```

共享 trait object：

```rust
let provider: Arc<dyn AiProvider> = Arc::new(
    HostAiProvider::new(host_ai_service),
);

let editor = Editor::builder()
    .document_id("document-1")
    .ai_provider_arc(provider)
    .build(cx)?;
```

关闭 AI：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .without_ai()
    .build(cx)?;
```

## 7. 完整 `CditorBuilder` 接入

```rust
let component = CditorBuilder::new()
    .with_document_id(42)
    .with_ai_provider(Arc::new(
        HostAiProvider::new(host_ai_service),
    ))
    .build(cx)?;
```

关闭 AI：

```rust
let component = CditorBuilder::new()
    .with_document_id(42)
    .without_ai()
    .build(cx)?;
```

## 8. 运行时替换 Provider

轻量 Handle：

```rust
editor.set_ai_provider(new_provider, cx)?;

editor.set_ai_provider_arc(
    Arc::new(new_provider),
    cx,
)?;
```

完整组件 Handle：

```rust
component.handle.set_ai_provider(
    Arc::new(new_provider),
    cx,
)?;
```

替换 Provider 会：

- 取消当前 AI 请求；
- 关闭当前 AI 输入状态；
- 重新读取模型目录；
- 当前模型在新目录仍有效时保留，否则选择新 Provider 的默认模型；
- 重新启用 AI。

## 9. 启用和关闭 AI

```rust
editor.set_ai_enabled(false, cx)?;
assert!(!editor.is_ai_enabled(cx));

editor.set_ai_enabled(true, cx)?;
```

完整组件：

```rust
component.handle.set_ai_enabled(false, cx)?;
```

关闭 AI 时，正在进行的请求会被取消，AI 输入框和模型菜单会关闭。

## 10. 查询和切换模型

模型列表：

```rust
let models = editor.ai_models(cx);
```

当前模型：

```rust
let selected = editor.selected_ai_model(cx);
```

宿主主动切换：

```rust
editor.select_ai_model("ollama:qwen3-14b", cx)?;
```

完整组件：

```rust
component
    .handle
    .select_ai_model("deepseek:v4-flash", cx)?;
```

无效模型 ID 返回错误，不会静默回退。

## 11. 动态模型目录

如果宿主的模型列表会因为登录用户、工作区、权限或本地 Ollama 状态变化而变化，可以让 `models()` 从共享注册表读取：

```rust
fn models(&self) -> Vec<AiModelDescriptor> {
    self.registry.current_models()
}
```

注册表变化后通知 Cditor：

```rust
editor.refresh_ai_models(cx)?;
```

完整组件：

```rust
component.handle.refresh_ai_models(cx)?;
```

刷新时：

- 当前模型仍存在则保留；
- 当前模型消失则尝试 `default_model_id()`；
- 默认模型无效则选择列表第一项；
- 空目录时隐藏模型选择器，并向 Provider 传递 `model_id: None`。

## 12. 模型切换事件

用户在 Cditor AI 面板切换模型，或者宿主调用 `select_ai_model` 时，会发出模型变化事件。

轻量集成：

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .ai_provider(provider)
    .on_event(|event| {
        if let EditorEvent::AiModelChanged { model } = event {
            save_last_ai_model(model.id);
        }
    })
    .build(cx)?;
```

完整组件 SDK 会发出：

```rust
CditorEvent::AiModelChanged {
    model: AiModelDescriptor,
}
```

模型切换不是文档内容变化，不会把文档标记为 Dirty。

## 13. AI 面板行为

当 Provider 返回至少一个模型时，Cditor 会在以下位置显示模型选择器：

- 独立 AI 输入条；
- 文本选区/块工具栏中的 AI 区域。

下拉行展示：

```text
CPA / gpt-5.5
OpenAI Compatible · 正式模型
```

模型条目很多时菜单内部滚动；菜单会根据面板位置选择向上或向下展开，并根据横向空间选择向左或向右展开。

Provider 只返回一个模型时仍显示当前模型，但不展示可点击的切换箭头。Provider 返回空目录时隐藏模型选择器，保持旧式单模型 Provider 兼容。

## 14. 默认模型选择规则

1. 保留当前仍有效的模型 ID。
2. 使用 `AiProvider::default_model_id()`。
3. 使用 `models()` 第一项。
4. 模型目录为空则使用 `None`。

宿主如果要恢复用户上次选择，可以在 Editor 创建完成后调用：

```rust
if let Some(model_id) = load_last_ai_model() {
    let _ = editor.select_ai_model(&model_id, cx);
}
```

如果上次模型已经下线，该调用会返回错误，宿主可以保留 Cditor 已选择的 Provider 默认模型。

## 15. 线程与取消

- `AiProvider` 必须是 `Send + Sync`。
- `stream` 在后台运行，可以执行阻塞式 HTTP/SSE 读取。
- 不要从 Provider 访问 `CditorV2View` 或 GPUI Entity。
- 定期检查 `AiCancellationToken::is_cancelled()`。
- 用户编辑目标块、改变选区、发起新请求、关闭 AI 或替换 Provider 时，旧请求会被取消或判定过期。

即使宿主没有及时停止旧请求，Cditor 也会通过 `request_id` 和目标内容版本忽略过期事件，但宿主仍应及时取消以节省模型费用和连接资源。

## 16. 最小可复制示例

```rust
use std::sync::Arc;

use cditor_gpui::{Editor, EditorEvent};

fn create_editor(
    cx: &mut gpui::App,
    host_ai: Arc<dyn HostAiService>,
) -> Result<cditor_gpui::EditorHandle, cditor_gpui::EditorError> {
    Editor::builder()
        .document_id("document-1")
        .initial_markdown("# Title\n\nSelect text and open AI.")
        .ai_provider(HostAiProvider::new(host_ai))
        .on_event(|event| {
            if let EditorEvent::AiModelChanged { model } = event {
                println!("selected AI model: {}", model.id);
            }
        })
        .build(cx)
}
```

核心原则是：模型选择由 Cditor UI 管理，模型能力和请求路由由宿主 Provider 管理，双方只通过稳定的模型 ID 和流事件通信。
