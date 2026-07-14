# Lightweight Third-Party Editor Component Refactor Plan

## Goal

Provide a dedicated `cditor-gpui` dependency for third-party GPUI applications. Its default dependency graph must not contain PostgreSQL, `sqlx`, `reqwest`, Tokio, or the official application launcher, while the existing `cditor-app` default build keeps its current PostgreSQL, OpenAI-compatible provider, and remote-media behavior.

## Architecture

The first migration stage establishes the public crate and dependency boundary without mechanically moving the large GUI module tree:

```text
cditor-gpui
└── cditor-app (default-features = false)
    ├── cditor-runtime (default-features = false)
    ├── cditor-core
    ├── cditor-editor
    └── gpui

cditor-app (default features)
├── feature: postgres
├── cditor-storage-postgres
└── sqlx
```

`cditor-gpui` re-exports only the supported editor integration surface. The application crate remains the implementation owner during this stage. A later source-layout-only migration may move `gui` and `integration` into `cditor-gpui` without changing the public API.

## Constraints

- Existing default `cditor-app` behavior remains PostgreSQL-enabled and API-compatible.
- `cditor-gpui` uses no default features that activate a database backend.
- Backend-neutral `EditorPersistence` remains the recommended persistence contract.
- PostgreSQL-specific APIs are available only with the `postgres` feature.
- OpenAI-compatible networking is available only with the `ai-openai` feature.
- HTTP image loading is available only with the `remote-media` feature; local image loading remains available in minimal builds.
- The minimal dependency gate is verified with `cargo tree`, not only source inspection.
- All repository shell commands are prefixed with `rtk`.

## Tasks

### 1. Remove runtime PostgreSQL coupling

- Keep `cditor-runtime` entirely independent of `cditor-storage` and `cditor-storage-postgres`.
- Keep cold-start construction and payload hydration inputs backend-neutral.
- Load PostgreSQL rows and payload windows in `cditor-app`, the composition root.
- Keep backend-neutral snapshot, editing, layout, projection, and persistence contracts in runtime.

### 2. Isolate application PostgreSQL support

- Add a default-enabled `postgres` feature to `cditor-app` for compatibility.
- Make `sqlx` and `cditor-storage-postgres` optional.
- Provide no-op PostgreSQL persistence state in minimal builds so the common editor mutation/render paths do not fork.
- Hide PostgreSQL builder methods, backend variants, cold-start code, and re-exports when the feature is disabled.

### 3. Add the public component crate

- Add workspace member `crates/gpui` with package name `cditor-gpui`.
- Depend on `cditor-app` with `default-features = false`.
- Re-export `Editor`, `EditorBuilder`, `EditorHandle`, document, event, error, and persistence types.
- Re-export the embeddable view type only as an advanced compatibility surface.

### 4. Documentation and verification

- Update the third-party integration guide to use `cditor-gpui`.
- Verify `cditor-gpui` compiles with no default features.
- Verify `sqlx` and `cditor-storage-postgres` are absent from its dependency graph.
- Verify the default `cditor-app` build and tests remain valid.

### 5. Isolate optional network capabilities

- Keep AI provider protocols and the mock provider available without networking.
- Move the OpenAI-compatible provider and its environment/configuration dependencies behind `cditor-ai/openai`.
- Forward OpenAI support through `cditor-app/ai-openai` and `cditor-gpui/ai-openai`.
- Move HTTP image fetching behind `cditor-app/remote-media` and `cditor-gpui/remote-media`.
- Preserve local file image decoding when `remote-media` is disabled.

## Completion Gates

```bash
rtk cargo check -p cditor-gpui
rtk cargo tree -p cditor-gpui -i sqlx
rtk cargo tree -p cditor-gpui -i cditor-storage-postgres
rtk cargo tree -p cditor-gpui -i reqwest
rtk cargo tree -p cditor-gpui -i tokio
rtk cargo check -p cditor-app
rtk cargo test -p cditor-app --lib
rtk git diff --check
```

The reverse dependency checks must report that the package is not present in the default `cditor-gpui` graph.
