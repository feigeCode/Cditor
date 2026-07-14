use std::time::Duration;

#[cfg(feature = "postgres")]
use sqlx::PgPool;

use cditor_core::ids::DocumentId;

pub type WorkspaceId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CditorOptions {
    pub workspace_id: Option<WorkspaceId>,
    pub document_id: Option<DocumentId>,
    pub backend: CditorBackend,
    pub readonly: bool,
    pub debug_overlay: bool,
    pub payload_window_size: usize,
    pub autosave_interval: Option<Duration>,
    pub seed_large_demo_to_postgres: bool,
    pub seed_large_demo_block_count: usize,
    pub force_reseed_large_demo: bool,
}

#[derive(Debug, Clone)]
pub enum CditorBackend {
    Demo,
    LargeDemo,
    Memory,
    #[cfg(feature = "postgres")]
    PostgresUrl {
        url: String,
    },
    #[cfg(feature = "postgres")]
    PostgresPool {
        pool: PgPool,
    },
    Cloud {
        endpoint: String,
    },
}

impl PartialEq for CditorBackend {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Demo, Self::Demo)
            | (Self::LargeDemo, Self::LargeDemo)
            | (Self::Memory, Self::Memory) => true,
            #[cfg(feature = "postgres")]
            (Self::PostgresUrl { url: a }, Self::PostgresUrl { url: b }) => a == b,
            #[cfg(feature = "postgres")]
            (Self::PostgresPool { .. }, Self::PostgresPool { .. }) => true,
            (Self::Cloud { endpoint: a }, Self::Cloud { endpoint: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for CditorBackend {}

impl Default for CditorOptions {
    fn default() -> Self {
        Self {
            workspace_id: None,
            document_id: None,
            backend: CditorBackend::Demo,
            readonly: false,
            debug_overlay: false,
            payload_window_size: 128,
            autosave_interval: None,
            seed_large_demo_to_postgres: false,
            seed_large_demo_block_count: cditor_core::demo_fixtures::LARGE_MIXED_DEMO_BLOCKS,
            force_reseed_large_demo: false,
        }
    }
}
