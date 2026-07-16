# 三方宿主快捷键与 Markdown 命令集成指南

本文说明第三方 Rust/GPUI 应用如何把 Cditor 的编辑能力接入宿主自己的快捷键设置系统。目标是让宿主负责保存和修改“按键 → 命令 ID”映射，Cditor 只负责执行命令、维护选区、Undo/Redo、Dirty 状态和文档事务。

适用的公共入口有两套：

- 轻量三方组件：`Editor` / `EditorHandle`，普通第三方应用优先使用。
- 完整组件 SDK：`CditorBuilder` / `CditorHandle`，适合需要 SQLite、PostgreSQL、诊断和完整组件生命周期的宿主。

两套 Handle 使用同一组稳定命令 ID。

## 1. 核心模型

宿主设置只保存稳定字符串，不保存 Cditor 内部 Rust 枚举：

```json
[
  {
    "keystrokes": "secondary-b",
    "command_id": "format.toggle_bold"
  },
  {
    "keystrokes": "secondary-shift-x",
    "command_id": "format.toggle_strike"
  },
  {
    "keystrokes": "secondary-1",
    "command_id": "block.set_heading_1"
  }
]
```

`secondary` 是 GPUI 的跨平台主编辑修饰键：

- macOS：`Command`
- Windows/Linux：`Control`

多段组合键使用空格分隔，例如：

```text
secondary-k secondary-1
```

数据流如下：

```text
宿主设置文件
    ↓
keystrokes + command_id
    ↓
CditorKeyBinding / CditorCommandAction
    ↓
Cditor 统一命令路由
    ↓
DocumentRuntime 事务、选区、Undo/Redo、Dirty、持久化
```

宿主不应直接修改 `DocumentRuntime` 来实现快捷键，否则容易绕开 Dirty、保存事件和组件状态同步。

## 2. 选择初始化方式

### 2.1 使用 Cditor 默认命令快捷键

如果宿主不提供快捷键设置页，使用兼容初始化：

```rust
cditor_gpui::init(cx);
```

它会安装：

- 输入、Enter、Tab、删除、方向键和选区键；
- 复制、剪切、粘贴；
- Undo、Redo、Select All；
- 加粗、斜体、下划线、行内代码；
- 默认块复制快捷键。

### 2.2 快捷键完全由宿主设置系统管理

如果命令快捷键在外层定义，使用：

```rust
cditor_gpui::init_for_external_keymap(cx);
```

该函数只安装编辑器必须的基础输入行为：

- Enter、软换行和新建下方段落；
- Tab / Shift+Tab；
- 光标移动和 Shift 扩展选择；
- Backspace / Delete；
- Copy / Cut / Paste；
- macOS/Windows 平台导航别名。

它不会安装 Undo、Redo、Select All、格式化或块命令快捷键。宿主随后调用 `bind_command_keys` 安装设置中的映射。

这样用户删除某条设置后，该命令不会又被 Cditor 的默认格式快捷键占用。

## 3. 从宿主设置绑定快捷键

宿主可以定义自己的可序列化设置类型，再转换成 `CditorKeyBinding`：

```rust
use cditor_gpui::{CditorKeyBinding, bind_command_keys};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct ShortcutSetting {
    keystrokes: String,
    command_id: String,
}

fn install_editor_shortcuts(
    cx: &mut gpui::App,
    settings: Vec<ShortcutSetting>,
) -> Result<(), cditor_gpui::CditorError> {
    let bindings = settings.into_iter().map(|setting| {
        CditorKeyBinding::new(setting.keystrokes, setting.command_id)
    });
    bind_command_keys(cx, bindings)
}
```

完整初始化示例：

```rust
fn initialize_cditor(
    cx: &mut gpui::App,
    shortcuts: Vec<ShortcutSetting>,
) -> Result<(), cditor_gpui::CditorError> {
    cditor_gpui::init_for_external_keymap(cx);
    install_editor_shortcuts(cx, shortcuts)?;
    Ok(())
}
```

`bind_command_keys` 会在注册前校验：

- `command_id` 是否是支持外部快捷键的稳定命令；
- `keystrokes` 是否为空；
- 每一段 GPUI keystroke 是否能成功解析。

错误返回 `CditorError::InvalidInput`，宿主可以在设置页标红对应配置，而不是等用户按键时静默失败。

建议在 GPUI 应用初始化阶段加载和绑定快捷键。若宿主支持运行时热更新 keymap，应由宿主自己的 GPUI keymap 管理器负责旧绑定的解除和替换；Cditor 同时公开了 `CditorCommandAction`，可供高级宿主直接接入自己的绑定生命周期。

## 4. 直接使用通用 GPUI Action

如果宿主已经有统一的 GPUI keymap 管理器，可以不使用 `bind_command_keys`，直接绑定公共 Action：

```rust
use cditor_gpui::CditorCommandAction;
use gpui::KeyBinding;

cx.bind_keys([
    KeyBinding::new(
        "secondary-b",
        CditorCommandAction::new("format.toggle_bold"),
        Some("CditorEditor"),
    ),
    KeyBinding::new(
        "secondary-shift-7",
        CditorCommandAction::new("block.toggle_ordered_list"),
        Some("CditorEditor"),
    ),
]);
```

通常优先使用 `bind_command_keys`，因为它会校验命令 ID 和按键语法。直接绑定 Action 适合已经自行完成配置校验、冲突检测和 Unbind 管理的宿主。

## 5. 创建轻量三方编辑器

```rust
use cditor_gpui::Editor;

let editor = Editor::builder()
    .document_id("document-1")
    .initial_markdown("# Title\n\nBody")
    .build(cx)?;
```

渲染时使用：

```rust
.child(editor.entity().clone())
```

宿主应长期保存 `EditorHandle`。快捷键 Action 在编辑器拥有 `CditorEditor` key context 时自动路由到当前编辑器实例。

## 6. 设置页枚举可用命令

设置页不需要硬编码命令清单：

```rust
let commands = editor.shortcut_commands();

for command in commands {
    println!("{}: {}", command.id, command.title);
}
```

也可以不依赖实例：

```rust
use cditor_gpui::CditorCommand;

let commands = CditorCommand::shortcut_descriptors();
```

返回类型是：

```rust
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
}
```

`id` 用于持久化和执行；`title` 可用于默认英文显示。需要中文或其他语言时，宿主可以按 `id` 做自己的 i18n 映射，命令 ID 不随展示语言变化。

## 7. 主动执行命令

除了键盘 Action，菜单、工具栏、命令面板和自动化测试也应复用同一命令入口。

### 7.1 `EditorHandle`

按稳定 ID 执行：

```rust
let outcome = editor.execute_command_by_id(
    "format.toggle_bold",
    cx,
)?;

if outcome.changed {
    // 文档实际发生了变化。
}
```

使用强类型命令执行：

```rust
use cditor_gpui::CditorCommand;

editor.execute_command(CditorCommand::ToggleBold, cx)?;
```

### 7.2 `CditorHandle`

完整组件 SDK 的方法名略有不同：

```rust
let outcome = component.handle.execute_by_id(
    "block.toggle_quote",
    cx,
)?;
```

或者：

```rust
component
    .handle
    .execute(CditorCommand::ToggleStrike, cx)?;
```

命令执行统一进入 Cditor 的内容变化、Undo/Redo 和 Dirty 路径。`CommandOutcome::changed == false` 表示命令合法，但当前选区或块状态不满足操作条件，或者目标状态已经存在。

## 8. 查询 enabled / active 状态

设置页通常只需要命令目录；工具栏、菜单和命令面板还应查询当前状态：

```rust
let state = editor.command_state_by_id(
    "format.toggle_bold",
    cx,
)?;

if state.visible {
    render_button(
        state.enabled,
        state.active,
    );
}
```

字段语义：

- `enabled`：当前是否可以执行。例如没有非空富文本选区时，加粗命令为 disabled。
- `active`：当前选区或焦点块是否已经处于该格式。例如选中文本全为粗体时为 active。
- `visible`：命令是否建议显示。当前内置快捷键命令均为 visible。

完整组件 SDK 使用：

```rust
let state = component.handle.command_state_by_id(
    "block.toggle_bullet_list",
    cx,
)?;
```

## 9. Typora 风格示例配置

下面只是宿主默认值示例。用户可以在外层设置页修改或删除任意映射：

```rust
use cditor_gpui::CditorKeyBinding;

let bindings = vec![
    CditorKeyBinding::new("secondary-z", "edit.undo"),
    CditorKeyBinding::new("secondary-shift-z", "edit.redo"),
    CditorKeyBinding::new("secondary-a", "edit.select_all"),

    CditorKeyBinding::new("secondary-b", "format.toggle_bold"),
    CditorKeyBinding::new("secondary-i", "format.toggle_italic"),
    CditorKeyBinding::new("secondary-u", "format.toggle_underline"),
    CditorKeyBinding::new("secondary-shift-x", "format.toggle_strike"),
    CditorKeyBinding::new("secondary-e", "format.toggle_inline_code"),

    CditorKeyBinding::new("secondary-0", "block.set_paragraph"),
    CditorKeyBinding::new("secondary-1", "block.set_heading_1"),
    CditorKeyBinding::new("secondary-2", "block.set_heading_2"),
    CditorKeyBinding::new("secondary-3", "block.set_heading_3"),
    CditorKeyBinding::new("secondary-4", "block.set_heading_4"),
    CditorKeyBinding::new("secondary-5", "block.set_heading_5"),
    CditorKeyBinding::new("secondary-6", "block.set_heading_6"),

    CditorKeyBinding::new("secondary-shift-8", "block.toggle_bullet_list"),
    CditorKeyBinding::new("secondary-shift-7", "block.toggle_ordered_list"),
    CditorKeyBinding::new("secondary-shift-t", "block.toggle_task_list"),
    CditorKeyBinding::new("secondary-shift-q", "block.toggle_quote"),
    CditorKeyBinding::new("secondary-alt-c", "block.toggle_code"),

    CditorKeyBinding::new("tab", "block.indent"),
    CditorKeyBinding::new("shift-tab", "block.outdent"),
    CditorKeyBinding::new("secondary-enter", "block.insert_paragraph_after"),
    CditorKeyBinding::new("secondary-d", "block.duplicate_selected"),
];
```

注意：`init_for_external_keymap` 已经为 Tab、Shift+Tab 和 `secondary-enter` 安装基础编辑行为。如果宿主使用相同按键注册命令映射，应明确采用自己的优先级和冲突策略。通常只需要把格式化、标题、列表、引用等命令放进外部设置；保留 Enter、Tab、光标、剪贴板给编辑器基础 keymap 管理会更简单。

## 10. 当前稳定命令 ID

### 10.1 编辑与历史

| 命令 ID | 含义 |
| --- | --- |
| `edit.undo` | 撤销 |
| `edit.redo` | 重做 |
| `edit.select_all` | 渐进式全选：先选中当前块文本，再扩展到文档 |
| `edit.delete_selection` | 删除当前有效选区 |

### 10.2 行内格式

| 命令 ID | 含义 |
| --- | --- |
| `format.toggle_bold` | 加粗/取消加粗 |
| `format.toggle_italic` | 斜体/取消斜体 |
| `format.toggle_underline` | 下划线/取消下划线 |
| `format.toggle_strike` | 删除线/取消删除线 |
| `format.toggle_inline_code` | 行内代码/取消行内代码 |

### 10.3 块类型

| 命令 ID | 含义 |
| --- | --- |
| `block.set_paragraph` | 转为正文段落 |
| `block.set_heading_1` … `block.set_heading_6` | 转为 1–6 级标题 |
| `block.toggle_bullet_list` | 项目符号列表；重复触发回到正文 |
| `block.toggle_ordered_list` | 有序列表；重复触发回到正文 |
| `block.toggle_task_list` | 待办列表；重复触发回到正文 |
| `block.toggle_quote` | 引用块；重复触发回到正文 |
| `block.toggle_callout` | Callout；重复触发回到正文 |
| `block.toggle_toggle` | 折叠块；重复触发回到正文 |
| `block.toggle_code` | 代码块；重复触发回到正文 |
| `block.toggle_math` | 公式块；重复触发回到正文 |
| `block.toggle_mermaid` | Mermaid 块；重复触发回到正文 |
| `block.toggle_todo_checked` | 切换当前待办块的完成状态 |

### 10.4 块结构

| 命令 ID | 含义 |
| --- | --- |
| `block.insert_paragraph_after` | 在当前块后插入正文段落并聚焦 |
| `block.indent` | 增加当前块缩进；代码等软 Tab 块会插入缩进文本 |
| `block.outdent` | 减少当前块缩进 |
| `block.delete_current` | 删除当前块；文档最后一个块会重置为空正文 |
| `block.delete_selected` | 删除块选择中的块 |
| `block.duplicate_selected` | 复制已选择块；没有块选择时复制当前焦点块 |
| `heading.fold` | 折叠当前标题 |
| `heading.unfold` | 展开当前标题 |

## 11. 选区与焦点规则

### 11.1 行内格式

当前行内格式命令要求：

- 编辑器已 Ready；
- 非只读；
- 选区位于一个支持 RichText spans 的块内；
- 选区非空。

只有光标、没有选择文字时，行内格式命令当前不会设置“后续输入样式”，因此 `enabled == false` 或执行结果 `changed == false`。这是现阶段与 Typora 的一个明确差异。

跨块文本选区目前不执行行内格式，以免部分块不支持相同 mark 时产生不完整事务。

### 11.2 块命令

块类型、缩进、插入和当前块删除命令作用于焦点块。`block.delete_selected` 和 `block.duplicate_selected` 优先作用于块选择。

列表、引用、Callout、代码块等 `block.toggle_*` 命令已经处于目标类型时，会转换回 Paragraph；标题命令是 `set` 语义，重复触发同一级标题不会再次修改文档。

### 11.3 浮层和特殊编辑模式

AI 输入框、代码语言输入框和表格菜单拥有键盘焦点时，文档级命令 Action 会被消费，不会修改浮层后面的文档。

## 12. 只读、Undo 和保存

- 只读模式下，修改命令返回 `EditorError::Readonly` 或 `CditorError::Readonly`。
- 命令产生内容变化后会进入 Cditor Dirty 和内容版本路径。
- 行内格式、块转换、待办状态和块复制均进入 Undo/Redo 记录。
- `EditorHandle` 会同步三方集成层的文档指纹、Changed 事件和自动保存状态。
- `CditorHandle` 会发出组件 SDK 的 `ContentChanged`、`DirtyChanged` 等事件。

宿主不需要在命令执行后手动调用 `set_markdown` 或重建文档。

## 13. 错误处理建议

```rust
match editor.execute_command_by_id(command_id, cx) {
    Ok(outcome) => {
        if !outcome.changed {
            // 合法命令，但当前选区/块不支持或状态未变化。
        }
    }
    Err(cditor_gpui::EditorError::InvalidCommand(id)) => {
        // 设置里保存了当前版本不认识的命令 ID。
        mark_shortcut_invalid(&id);
    }
    Err(cditor_gpui::EditorError::Readonly) => {
        show_readonly_hint();
    }
    Err(error) => {
        report_editor_error(error);
    }
}
```

升级 Cditor 后，宿主可以重新调用 `shortcut_commands()`，将设置中已经不存在的 ID 标记为失效；不要自动把未知 ID 改成其他命令。

## 14. 尚未作为无参数快捷键开放的能力

以下公共 `CditorCommand` 变体需要额外参数或宿主 Provider，因此没有加入 `from_stable_id` 和快捷键目录：

- 任意 `InsertBlock(BlockInput)`；
- 指定行列数的 `InsertTable { rows, columns }`；
- 需要文件选择器或 Asset Provider 的图片插入；
- 需要额外创建参数的 Whiteboard、Mermaid 插入。

宿主可以在菜单或对话框收集参数后使用强类型命令调用，但不能仅凭一个无参数命令 ID 安全构造这些操作。后续如果要支持快捷键，可以新增固定语义命令，例如“插入默认 3×3 表格”或“打开图片选择器”，并分配新的稳定 ID。

## 15. 最小可复制示例

```rust
use cditor_gpui::{
    CditorKeyBinding,
    Editor,
    bind_command_keys,
    init_for_external_keymap,
};

fn initialize(cx: &mut gpui::App) -> Result<(), cditor_gpui::CditorError> {
    init_for_external_keymap(cx);
    bind_command_keys(
        cx,
        [
            CditorKeyBinding::new("secondary-b", "format.toggle_bold"),
            CditorKeyBinding::new("secondary-i", "format.toggle_italic"),
            CditorKeyBinding::new("secondary-shift-x", "format.toggle_strike"),
            CditorKeyBinding::new("secondary-1", "block.set_heading_1"),
            CditorKeyBinding::new("secondary-shift-8", "block.toggle_bullet_list"),
            CditorKeyBinding::new("secondary-shift-q", "block.toggle_quote"),
        ],
    )?;
    Ok(())
}

fn create_editor(cx: &mut gpui::App) -> Result<cditor_gpui::EditorHandle, cditor_gpui::EditorError> {
    Editor::builder()
        .document_id("document-1")
        .initial_markdown("# Title\n\nSelect text and press the configured shortcut.")
        .build(cx)
}
```

三方接入的关键原则只有两个：宿主保存稳定命令 ID，所有修改仍由 Cditor 的统一命令层执行。
