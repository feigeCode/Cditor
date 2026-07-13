# Third-Party Editor Integration Design

## Goal

Provide a stable, small public API that lets third-party Rust/GPUI applications embed Cditor, load and export documents, observe changes, control basic editor state, and persist content through either a custom persistence implementation or manual import/export calls.

The integration API must hide `CditorV2View` and `DocumentRuntime` internals for normal use while preserving existing `Cditor` APIs for compatibility.

## Scope

This change includes:

- a recommended `Editor` builder for third-party embedding;
- an `EditorHandle` control surface;
- a serializable native `EditorDocument` snapshot;
- Markdown and JSON import/export;
- document change and save-state events;
- a backend-neutral `EditorPersistence` trait;
- manual and debounced automatic save;
- reload, readonly, focus, dirty-state, version and save-state operations;
- public API tests and regenerated integration documentation.

This change does not replace the existing PostgreSQL integration, expose every `DocumentRuntime` command, create bindings for non-Rust languages, or define a network protocol.

## Public Module Layout

Add the following modules under `crates/app/src/integration/`:

```text
integration/
├── mod.rs
├── builder.rs
├── document.rs
├── error.rs
├── events.rs
├── handle.rs
└── persistence.rs
```

`crates/app/src/lib.rs` re-exports the supported integration surface:

```rust
pub use integration::{
    Editor,
    EditorBuilder,
    EditorDocument,
    EditorError,
    EditorEvent,
    EditorHandle,
    EditorPersistence,
    EditorPersistenceError,
    EditorSaveReason,
    EditorSaveRequest,
    EditorSaveState,
};
```

Existing exports such as `Cditor`, `CditorV2View`, `DocumentRuntime`, and PostgreSQL types remain available and behavior-compatible.

## Editor Builder

The recommended entry point is:

```rust
let editor = Editor::builder()
    .document_id("document-1")
    .initial_markdown("# Title")
    .readonly(false)
    .debug_overlay(false)
    .persistence(persistence)
    .autosave(Duration::from_secs(3))
    .on_event(callback)
    .build(cx)?;
```

The builder supports:

- `document_id(String)`;
- `initial_markdown(String)`;
- `initial_document(EditorDocument)`;
- `readonly(bool)`;
- `debug_overlay(bool)`;
- `persistence(Arc<dyn EditorPersistence>)` or a generic convenience accepting an implementation;
- `autosave(Duration)`;
- `on_event(Fn(EditorEvent) + Send + Sync + 'static)`;
- `build(&mut App) -> Result<EditorHandle, EditorError>`.

Initial content precedence is explicit and mutually exclusive: setting `initial_document` replaces a prior initial Markdown value, and setting `initial_markdown` replaces a prior initial document value. If persistence is configured and contains a document, loaded persisted content wins over the builder's initial content. Initial content is the fallback for a persistence `load` result of `None`.

Without persistence, build completes synchronously with initial content or one empty Paragraph. With persistence, the handle is created in Loading state and performs load work in a GPUI background task.

## EditorHandle

`EditorHandle` owns the editor `Entity<CditorV2View>` and shared integration state. It provides:

```rust
impl EditorHandle {
    pub fn entity(&self) -> &Entity<CditorV2View>;

    pub fn set_markdown(
        &self,
        markdown: impl Into<String>,
        cx: &mut App,
    ) -> Result<(), EditorError>;

    pub fn get_markdown(&self, cx: &App) -> Result<String, EditorError>;

    pub fn set_document(
        &self,
        document: EditorDocument,
        cx: &mut App,
    ) -> Result<(), EditorError>;

    pub fn get_document(&self, cx: &App) -> Result<EditorDocument, EditorError>;

    pub fn save(&self, cx: &mut App) -> Result<(), EditorError>;
    pub fn reload(&self, cx: &mut App) -> Result<(), EditorError>;
    pub fn focus(&self, cx: &mut App) -> Result<(), EditorError>;
    pub fn is_dirty(&self, cx: &App) -> bool;
    pub fn save_state(&self, cx: &App) -> EditorSaveState;
    pub fn document_version(&self, cx: &App) -> u64;

    pub fn set_readonly(
        &self,
        readonly: bool,
        cx: &mut App,
    ) -> Result<(), EditorError>;
}
```

The handle is cloneable. All mutations enter the GPUI entity through its public update mechanism. Third parties do not receive mutable references to internal view fields.

`set_document` and `set_markdown` install a new clean baseline and reset integration dirty/save state. User edits after installation create new document versions and mark the handle dirty.

## EditorDocument

Markdown is intentionally not the persistence source of truth because it cannot losslessly represent every Cditor block, table style, merged cell, media attribute, Mermaid configuration, or whiteboard payload.

The native snapshot is:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorDocument {
    pub schema_version: u32,
    pub document_id: String,
    pub structure_version: u64,
    pub blocks: Vec<EditorBlock>,
}
```

`EditorBlock` contains the existing serializable block index and payload data required to rebuild a `DocumentRuntime`. Integration types may wrap existing core records, but their serialized field names and schema version form the public compatibility contract.

Required helpers:

```rust
impl EditorDocument {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    pub fn from_markdown(
        document_id: impl Into<String>,
        markdown: &str,
    ) -> Result<Self, EditorError>;

    pub fn to_markdown(&self) -> Result<String, EditorError>;
    pub fn from_json(json: &str) -> Result<Self, EditorError>;
    pub fn to_json(&self) -> Result<String, EditorError>;
}
```

Deserialization rejects unsupported future schema versions with a typed error. Conversion between external string document IDs and runtime numeric IDs is deterministic within the integration layer; the original external string remains in the snapshot and persistence requests.

## Runtime Snapshot Conversion

Add runtime snapshot methods sufficient to export all loaded content for an integration-owned in-memory document. The integration API does not silently export partial documents. If a runtime contains unloaded payload ranges, export returns a typed incomplete-document error unless the integration layer has a loader capable of completing the snapshot.

For integration-created memory documents, all payloads are loaded, so `get_document` and `get_markdown` are complete.

Markdown import uses the existing `parse_markdown_document` path. Markdown export rebuilds a `RichTextDocument` or equivalent ordered representation and uses `export_plain_markdown`.

## Persistence Contract

Third parties may implement:

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

The synchronous trait is deliberately object-safe and does not require `async_trait`. Cditor always invokes `load` and `save` in GPUI background work, never on the input/render hot path.

The save request is:

```rust
#[derive(Clone, Debug)]
pub struct EditorSaveRequest {
    pub document_id: String,
    pub document: EditorDocument,
    pub document_version: u64,
    pub reason: EditorSaveReason,
}
```

Reasons are:

```rust
pub enum EditorSaveReason {
    Manual,
    Autosave,
    BeforeReload,
    BeforeClose,
}
```

No persistence configuration is required for manual `get_document`, `get_markdown`, `set_document`, or `set_markdown`. Calling `save` or `reload` without persistence returns `EditorError::PersistenceNotConfigured`.

## Change Detection

The integration layer maintains an external document version independent of layout versions. Every committed user-visible content or structure change increments this version exactly once and sets the save state to Dirty.

Change detection is attached at the common mutation boundary used by GUI editing commands, not by polling rendered projections. Programmatic baseline replacement through `set_document` or `set_markdown` resets the state to Clean instead of emitting a user-change dirty transition.

The existing runtime transaction and content/structure version signals should be reused where possible. Layout-only changes, scroll changes, selection changes, focus changes, cache updates, and debug-overlay updates must not mark the document dirty.

## Save State and Optimistic Versioning

Public states are:

```rust
pub enum EditorSaveState {
    Disabled,
    Clean,
    Dirty,
    Saving,
    SaveFailed { message: String },
}
```

The integration state tracks:

- current document version;
- last persisted version;
- optional saving version;
- last error;
- autosave generation.

If version 5 is saving and the user edits to version 6, completion of version 5 advances the persisted version but leaves the state Dirty. Clean is valid only when persisted version equals the current document version and no save is in flight.

Save failures never discard in-memory content. The state becomes `SaveFailed`, the failure event is emitted, and a later edit or explicit save may retry.

## Autosave

Autosave is enabled only when both a persistence implementation and an autosave duration are configured.

On each document change:

1. mark Dirty and emit Changed;
2. increment an autosave generation;
3. start a background debounce timer carrying that generation;
4. when the timer completes, discard it if its generation is stale;
5. snapshot the current document on the GPUI thread;
6. invoke persistence save in background work;
7. apply the result on the GPUI thread using optimistic version checks.

Typing does not synchronously serialize JSON or call third-party storage. Snapshot creation occurs only after debounce or an explicit save request.

## Reload

`reload` requires persistence. If the editor is dirty, it first saves with `BeforeReload`; reload continues only after a successful save. It then loads the persisted document in background work and installs it as a clean baseline.

Concurrent stale load results carry a load generation and are discarded if a newer set/reload operation has superseded them.

## Events

The public event stream is callback-based in the first version:

```rust
pub enum EditorEvent {
    Ready {
        document_id: String,
    },
    Changed {
        document_id: String,
        document_version: u64,
    },
    SaveStateChanged {
        state: EditorSaveState,
    },
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

Callbacks are invoked after the editor releases mutable runtime/view borrows. A callback must not be invoked while internal state is mid-transition. Panics from user callbacks must not corrupt editor state; state transitions complete before callback dispatch.

## Readonly and Focus

Readonly is runtime-configurable through `EditorHandle::set_readonly`. It prevents user editing but does not prevent programmatic document replacement or export. The current value is kept in integration state and mirrored to `CditorV2View`.

`focus` requests the editor's primary GPUI focus handle. It returns a typed unavailable/not-ready error if the view is not ready to receive focus.

## Error Model

`EditorError` includes typed variants for:

- editor not ready;
- persistence not configured;
- persistence load/save failure;
- invalid Markdown/document/JSON;
- unsupported schema version;
- incomplete runtime snapshot;
- document ID mismatch;
- GPUI entity update failure;
- readonly or invalid operation where applicable.

`EditorPersistenceError` is an owned message plus optional category suitable for crossing background task boundaries. Public errors implement `Display` and `std::error::Error`.

## Compatibility

- Existing `Cditor::new().memory()`, Demo, LargeDemo and PostgreSQL behavior remains unchanged.
- No existing public method is removed or renamed.
- Integration code is additive and becomes the recommended third-party API.
- Serialized `EditorDocument` compatibility is governed by `schema_version`.
- The Git integration guide recommends pinning an exact revision.

## Testing

Add tests covering:

1. builder defaults and mutually exclusive initial content;
2. empty memory editor creation;
3. Markdown import/export round-trip for supported structures;
4. native document JSON round-trip;
5. unsupported schema rejection;
6. `set_document`/`get_document` and `set_markdown`/`get_markdown`;
7. dirty/version transition after text and structure edits;
8. selection, scroll and layout changes do not mark dirty;
9. manual save success and failure;
10. persistence-not-configured errors;
11. save version 5 completing after edit version 6 remains Dirty;
12. autosave debounce and stale generation discard;
13. reload success, failure and stale load discard;
14. readonly toggling;
15. Ready, Changed, Saved, SaveFailed and LoadFailed event ordering;
16. mock persistence end-to-end behavior;
17. public API compile examples.

The final verification gate is:

```bash
./scripts/dev/check_workspace.sh
```

PostgreSQL ignored integration tests are not required for the backend-neutral integration layer unless existing PostgreSQL code is modified.

## Documentation and Delivery

Regenerate `doc/guides/editor-integration.md` after implementation. It must include:

- Git dependency using `https://github.com/feigeCode/Cditor.git` and an exact revision;
- minimal embed example;
- initial Markdown and native document examples;
- `EditorHandle` method reference;
- event handling;
- manual JSON/Markdown persistence;
- custom `EditorPersistence` implementation;
- manual save, autosave and reload;
- readonly and error handling;
- current limitations and compatibility guidance.

After tests and documentation verification, commit all intended changes and push the current branch to `origin` as explicitly authorized by the user.
