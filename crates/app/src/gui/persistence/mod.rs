pub mod close_guard;
mod payload_loader;
#[cfg(feature = "postgres")]
pub mod postgres_saver;
#[cfg(not(feature = "postgres"))]
mod postgres_saver_stub;
pub mod save_indicator;

#[cfg(feature = "postgres")]
pub(crate) use payload_loader::POSTGRES_VIEWPORT_LOAD_TIMEOUT;
pub(crate) use payload_loader::{PayloadWindowLoadSchedule, PayloadWindowLoadScheduler};
#[cfg(feature = "postgres")]
pub use postgres_saver::{
    DEFAULT_POSTGRES_SAVE_DEBOUNCE, PostgresPersistenceState, PostgresPersistenceTarget,
    PostgresSaveOutcome, mark_dirty_and_schedule_postgres_save, save_postgres_batch,
};
#[cfg(not(feature = "postgres"))]
pub use postgres_saver_stub::{
    DEFAULT_POSTGRES_SAVE_DEBOUNCE, PostgresPersistenceState, PostgresPersistenceTarget,
    mark_dirty_and_schedule_postgres_save,
};
pub use save_indicator::{
    EditorLoadStateLabel, EditorSaveStatus, render_load_state, render_save_indicator,
};
