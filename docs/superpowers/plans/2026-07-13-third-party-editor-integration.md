# Third-Party Editor Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a stable third-party editor API with native document snapshots, Markdown/JSON interchange, events, manual persistence, autosave, reload, readonly and focus control.

**Architecture:** Add an additive `cditor_app::integration` facade around the existing GPUI view and runtime. The facade owns integration-specific version/save state, converts complete runtimes to a versioned serializable snapshot, and runs object-safe third-party persistence callbacks in GPUI background tasks without blocking input.

**Tech Stack:** Rust 2024, GPUI, Serde/Serde JSON, existing Cditor core/runtime and test infrastructure.

## Global Constraints

- Existing `Cditor` Demo, Memory, LargeDemo and PostgreSQL behavior must remain compatible.
- Do not remove or rename an existing public API.
- `EditorDocument` native JSON is the lossless persistence format; Markdown is an interchange format.
- Third-party persistence must never run on the input/render hot path.
- Layout, selection, focus and scroll changes must not mark content dirty.
- Clean is valid only when persisted version equals current document version.
- `set_document` and `set_markdown` establish a clean baseline.
- Unsupported future `EditorDocument` schema versions must return a typed error.
- Integration-created memory documents must export complete snapshots; partial runtime export must fail explicitly.
- All shell commands in this repository must be prefixed with `rtk`.

---

### Task 1: Versioned native document snapshot and Markdown/JSON conversion

**Files:**
- Create: `crates/app/src/integration/document.rs`
- Create: `crates/app/src/integration/error.rs`
- Create: `crates/app/src/integration/mod.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/runtime/src/document_runtime/structure_edit.rs`
- Test: `crates/app/src/integration/document.rs`

**Interfaces:**
- Consumes: `BlockIndexRecord`, `BlockPayloadRecord`, `RichTextDocument`, `parse_markdown_document`, `export_plain_markdown`, `DocumentRuntime::from_payloads`.
- Produces: `EditorDocument`, `EditorBlock`, `EditorError`, runtime complete snapshot accessors and conversion helpers used by all later tasks.

- [ ] **Step 1: Write failing document conversion tests**

Add tests for JSON round-trip, Markdown import/export, unsupported schema and complete runtime snapshot:

```rust
#[test]
fn editor_document_json_round_trip_preserves_blocks() {
    let document = EditorDocument::from_markdown("doc-1", "# Title\n\nBody").unwrap();
    let json = document.to_json().unwrap();
    assert_eq!(EditorDocument::from_json(&json).unwrap(), document);
}

#[test]
fn editor_document_rejects_future_schema() {
    let json = r#"{"schema_version":999,"document_id":"doc-1","structure_version":1,"blocks":[]}"#;
    assert!(matches!(
        EditorDocument::from_json(json),
        Err(EditorError::UnsupportedSchemaVersion { version: 999 })
    ));
}

#[test]
fn runtime_snapshot_exports_markdown() {
    let runtime = DocumentRuntime::demo();
    let document = EditorDocument::from_runtime("doc-1", &runtime).unwrap();
    assert!(document.to_markdown().unwrap().contains("Cditor"));
}
```

- [ ] **Step 2: Run the focused test and verify failure**

Run:

```bash
rtk cargo test -p cditor-app integration::document::tests --lib
```

Expected: compilation fails because `integration`, `EditorDocument` and `EditorError` do not exist.

- [ ] **Step 3: Add complete runtime snapshot accessors**

Add an accessor returning ordered index and payload records only when every visible Block has a loaded payload:

```rust
pub fn complete_document_snapshot(
    &self,
) -> Option<(Vec<BlockIndexRecord>, Vec<BlockPayloadRecord>)> {
    let records = self.index_records_snapshot();
    let payloads = records
        .iter()
        .map(|record| self.block_payload_record(record.block_id))
        .collect::<Option<Vec<_>>>()?;
    Some((records, payloads))
}
```

- [ ] **Step 4: Implement integration errors and document types**

Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    NotReady,
    PersistenceNotConfigured,
    InvalidMarkdown(String),
    InvalidDocument(String),
    InvalidJson(String),
    UnsupportedSchemaVersion { version: u32 },
    IncompleteDocument,
    DocumentIdMismatch { expected: String, actual: String },
    Persistence(EditorPersistenceError),
    EntityUpdate(String),
}
```

Define `EditorBlock` with `index: BlockIndexRecord` and `payload: BlockPayloadRecord`, and `EditorDocument` with the exact fields from the design. Implement `from_markdown`, `to_markdown`, `from_json`, `to_json`, `from_runtime` and `into_runtime`.

Use a deterministic runtime ID derived from the external string ID, while retaining the original string in `EditorDocument`.

- [ ] **Step 5: Export the new module**

Add:

```rust
pub mod integration;
pub use integration::{EditorBlock, EditorDocument, EditorError};
```

- [ ] **Step 6: Run focused and crate tests**

Run:

```bash
rtk cargo test -p cditor-app integration::document::tests --lib
rtk cargo test -p cditor-app --lib
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
rtk git add crates/app/src/integration crates/app/src/lib.rs crates/runtime/src/document_runtime/structure_edit.rs
rtk git commit -m "feat: add serializable editor document snapshot"
```

### Task 2: Persistence protocol, events and optimistic save state

**Files:**
- Create: `crates/app/src/integration/persistence.rs`
- Create: `crates/app/src/integration/events.rs`
- Modify: `crates/app/src/integration/error.rs`
- Modify: `crates/app/src/integration/mod.rs`
- Test: `crates/app/src/integration/persistence.rs`

**Interfaces:**
- Consumes: `EditorDocument`, `EditorError`.
- Produces: `EditorPersistence`, `EditorSaveRequest`, `EditorSaveReason`, `EditorSaveState`, `EditorEvent`, `IntegrationPersistenceState`.

- [ ] **Step 1: Write failing optimistic state tests**

```rust
#[test]
fn older_save_success_does_not_clean_newer_edit() {
    let mut state = IntegrationPersistenceState::new(true);
    state.mark_changed();
    let saving = state.begin_save().unwrap();
    state.mark_changed();
    state.save_succeeded(saving);
    assert_eq!(state.public_state(), EditorSaveState::Dirty);
}

#[test]
fn save_failure_preserves_dirty_version() {
    let mut state = IntegrationPersistenceState::new(true);
    state.mark_changed();
    let saving = state.begin_save().unwrap();
    state.save_failed(saving, "disk full".into());
    assert!(matches!(state.public_state(), EditorSaveState::SaveFailed { .. }));
    assert!(state.is_dirty());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

```bash
rtk cargo test -p cditor-app integration::persistence::tests --lib
```

Expected: compilation fails because persistence types do not exist.

- [ ] **Step 3: Implement public persistence types**

Implement the exact design contracts:

```rust
pub trait EditorPersistence: Send + Sync + 'static {
    fn load(&self, document_id: &str)
        -> Result<Option<EditorDocument>, EditorPersistenceError>;
    fn save(&self, request: EditorSaveRequest)
        -> Result<(), EditorPersistenceError>;
}
```

Add `EditorPersistenceError`, `EditorSaveReason`, `EditorSaveRequest`, `EditorSaveState` and `EditorEvent` with `Clone`, `Debug` and equality derives where valid.

- [ ] **Step 4: Implement optimistic integration state**

Track `document_version`, `persisted_version`, `saving_version`, `last_error`, `enabled`, `autosave_generation` and `load_generation`. Implement baseline reset, dirty change, begin/save success/save failure and stale generation helpers.

- [ ] **Step 5: Run tests**

```bash
rtk cargo test -p cditor-app integration::persistence::tests --lib
```

Expected: all persistence tests pass.

- [ ] **Step 6: Commit**

```bash
rtk git add crates/app/src/integration
rtk git commit -m "feat: define editor persistence and event contracts"
```

### Task 3: View integration state and document-change bridge

**Files:**
- Create: `crates/app/src/gui/app/integration_bridge.rs`
- Modify: `crates/app/src/gui/app/mod.rs`
- Modify: `crates/app/src/gui/app/cditor_v2_view.rs`
- Modify: `crates/app/src/gui/app/lifecycle.rs`
- Modify: common GUI mutation completion paths under `crates/app/src/gui/app/input/` and `crates/app/src/gui/app/cditor_v2_view/`
- Test: `crates/app/src/gui/app/cditor_v2_view_tests.rs`

**Interfaces:**
- Consumes: `IntegrationPersistenceState`, `EditorEvent`, runtime structure/content versions.
- Produces: view methods to install integration state, mark committed document changes, snapshot a complete document, set readonly, focus, and dispatch events after mutations.

- [ ] **Step 1: Write failing change-boundary tests**

Create tests that install integration state, perform a real text edit, and assert one version increment. Also perform selection and scroll operations and assert no increment.

```rust
#[test]
fn committed_text_edit_marks_integration_dirty_once() {
    let mut view = test_view();
    view.enable_test_integration("doc-1");
    let before = view.integration_document_version().unwrap();
    view.runtime_mut_for_test().insert_char('x').unwrap();
    view.commit_integration_change_for_test();
    assert_eq!(view.integration_document_version(), Some(before + 1));
    assert_eq!(view.integration_save_state(), Some(EditorSaveState::Dirty));
}
```

- [ ] **Step 2: Run focused test and verify failure**

```bash
rtk cargo test -p cditor-app gui::app::cditor_v2_view_tests --lib
```

Expected: compilation fails because the integration bridge is absent.

- [ ] **Step 3: Add optional integration state to `CditorV2View`**

Store an optional shared integration controller containing document ID, persistence state, callback, persistence implementation and autosave duration. Existing `Cditor` construction leaves it disabled.

- [ ] **Step 4: Add view-level integration methods**

Implement crate-visible methods for baseline installation, complete snapshot, Markdown export, readonly update, focus request, state reads and event queueing. Events must dispatch after mutable borrows are released.

- [ ] **Step 5: Attach dirty marking to common committed mutations**

Identify the shared success paths for text, composition, structure, table, paste, media and AI mutations. On `changed == true`, call one integration change marker. Do not mark changes from selection, focus, scroll, projection, layout or cache updates.

- [ ] **Step 6: Run app tests**

```bash
rtk cargo test -p cditor-app --lib
```

Expected: existing tests and new integration change tests pass.

- [ ] **Step 7: Commit**

```bash
rtk git add crates/app/src/gui/app
rtk git commit -m "feat: bridge editor changes to integration state"
```

### Task 4: Editor builder and stable handle

**Files:**
- Create: `crates/app/src/integration/builder.rs`
- Create: `crates/app/src/integration/handle.rs`
- Modify: `crates/app/src/integration/mod.rs`
- Modify: `crates/app/src/lib.rs`
- Test: `crates/app/src/integration/builder.rs`
- Test: `crates/app/src/integration/handle.rs`

**Interfaces:**
- Consumes: document conversion, view bridge, persistence contracts.
- Produces: `Editor`, `EditorBuilder`, `EditorHandle` and the public synchronous control methods.

- [ ] **Step 1: Write failing builder and handle tests**

Cover default empty content, initial Markdown precedence, set/get Markdown, set/get native document, dirty reads, readonly and persistence-not-configured errors.

- [ ] **Step 2: Run focused tests and verify failure**

```bash
rtk cargo test -p cditor-app integration::builder::tests --lib
rtk cargo test -p cditor-app integration::handle::tests --lib
```

Expected: compilation fails because builder and handle do not exist.

- [ ] **Step 3: Implement `Editor` and `EditorBuilder`**

Use:

```rust
pub struct Editor;

impl Editor {
    pub fn builder() -> EditorBuilder {
        EditorBuilder::default()
    }
}
```

Build a ready memory view without persistence. With persistence, build a Loading view, increment load generation and run `load` in background; persisted content wins, then initial content, then an empty document.

- [ ] **Step 4: Implement `EditorHandle` synchronous methods**

Implement the signatures from the design. `set_markdown` parses first and changes no state on parse failure. `set_document` validates document identity and installs a clean baseline. `save` and `reload` delegate to asynchronous controller helpers while returning immediate scheduling errors.

- [ ] **Step 5: Re-export the stable API**

Update `crates/app/src/lib.rs` to export all integration types listed in the design.

- [ ] **Step 6: Run focused and crate tests**

```bash
rtk cargo test -p cditor-app integration --lib
rtk cargo test -p cditor-app --lib
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
rtk git add crates/app/src/integration crates/app/src/lib.rs
rtk git commit -m "feat: expose third-party editor builder and handle"
```

### Task 5: Background load/save, autosave, reload and event ordering

**Files:**
- Modify: `crates/app/src/integration/handle.rs`
- Modify: `crates/app/src/integration/builder.rs`
- Modify: `crates/app/src/integration/persistence.rs`
- Modify: `crates/app/src/gui/app/integration_bridge.rs`
- Test: `crates/app/src/integration/handle.rs`

**Interfaces:**
- Consumes: handle, shared controller, persistence trait, snapshot conversion, GPUI background task/update APIs.
- Produces: manual save, debounced autosave, reload, stale-result discard and event sequencing.

- [ ] **Step 1: Add failing mock persistence tests**

Create a thread-safe mock with configurable load/save results and captured requests. Test manual success/failure, autosave debounce, stale older save, reload and event order.

```rust
#[test]
fn save_v5_finishing_after_edit_v6_stays_dirty() {
    // Arrange a blocking mock save for version 5, edit to version 6,
    // release save 5, then assert Dirty and no Clean event.
}
```

- [ ] **Step 2: Run focused tests and verify failure**

```bash
rtk cargo test -p cditor-app integration::handle::tests --lib
```

Expected: new persistence behavior tests fail.

- [ ] **Step 3: Implement manual background save**

Snapshot on the GPUI thread, mark Saving, dispatch `SaveStateChanged`, call `EditorPersistence::save` in background, then apply success/failure with saving-version validation and dispatch `Saved` or `SaveFailed`.

- [ ] **Step 4: Implement debounced autosave**

Each dirty change advances autosave generation. The timer discards stale generations before snapshot/save. No serialization or third-party persistence call occurs synchronously in the edit path.

- [ ] **Step 5: Implement reload**

If dirty, schedule `BeforeReload` save and continue only on success. Load in background, reject stale generation, install the result as Clean, then emit Ready. Emit LoadFailed without replacing current content on failure.

- [ ] **Step 6: Run integration and workspace tests**

```bash
rtk cargo test -p cditor-app integration --lib
rtk cargo test --workspace
```

Expected: all non-ignored tests pass.

- [ ] **Step 7: Commit**

```bash
rtk git add crates/app/src/integration crates/app/src/gui/app/integration_bridge.rs
rtk git commit -m "feat: add editor autosave and reload workflows"
```

### Task 6: Regenerate integration guide and complete delivery gates

**Files:**
- Replace: `doc/guides/editor-integration.md`
- Modify: `doc/README.md`
- Modify if required: `README.md`
- Test: public examples through app doctests or compile-test module.

**Interfaces:**
- Consumes: final public API and actual pushed Git revision.
- Produces: copyable third-party integration documentation and verified delivery.

- [ ] **Step 1: Add a public API compile example test**

Add a test or doctest that imports the root re-exports and builds an `EditorBuilder` with a mock persistence implementation. It must compile without importing private modules.

- [ ] **Step 2: Regenerate the guide from the final API**

Document the exact Git dependency, minimal embed, initial Markdown/native JSON, event callback, manual export/save, `EditorPersistence`, autosave, reload, readonly, errors and limitations. Do not document internal View fields or runtime-only APIs as the recommended path.

- [ ] **Step 3: Run documentation and structure checks**

```bash
rtk rg -n 'TBD|TODO|PLACEHOLDER' doc/guides/editor-integration.md
rtk git diff --check
rtk bash scripts/dev/check_structure.sh
```

Expected: placeholder search finds nothing; diff and structure checks pass.

- [ ] **Step 4: Run the full verification gate**

```bash
rtk bash scripts/dev/check_workspace.sh
```

Expected: formatting, workspace check and all non-ignored workspace tests pass.

- [ ] **Step 5: Audit requirements against the design**

Verify root exports, builder, handle methods, native snapshot, Markdown/JSON, events, manual persistence, trait persistence, dirty/version semantics, autosave, reload, readonly, focus, errors, compatibility, tests and documentation with direct file/test evidence.

- [ ] **Step 6: Commit all intended documentation and API example changes**

```bash
rtk git add doc/README.md doc/guides/editor-integration.md README.md crates/app
rtk git commit -m "docs: publish third-party editor integration guide"
```

- [ ] **Step 7: Push the current branch**

```bash
rtk git push origin HEAD
```

Expected: push succeeds and remote reports the new commits.
