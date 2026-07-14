pub mod cditor_v2_view;

mod input;
mod input_trace;
mod integration_bridge;
mod interaction;
mod lifecycle;
#[cfg(feature = "postgres")]
mod persistence_bridge;
#[cfg(not(feature = "postgres"))]
mod persistence_bridge_stub;
mod render;
mod state;
mod text_hit;

pub use cditor_v2_view::CditorV2View;
pub(crate) use cditor_v2_view::GuiPlatformInputTarget;
