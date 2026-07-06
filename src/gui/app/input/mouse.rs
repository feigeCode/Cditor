use std::time::Instant;

use gpui::{Context, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, Window};

use crate::editor::scroll::{ScrollDeltaMode, ScrollDevice, ScrollInput, ScrollPhase};
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::scrollbar::scrollbar_policy;

impl CditorV2View {
    pub(in crate::gui::app) fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_wheel_delta_y = scroll_delta_y(event);
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let before = runtime.scroll.global_scroll_top;
            let start = Instant::now();
            self.scroll_accumulator.push_input(
                ScrollInput {
                    delta_y: self.last_wheel_delta_y,
                    mode: ScrollDeltaMode::Pixel,
                    phase: scroll_phase_from_touch(event.touch_phase),
                    device: ScrollDevice::Trackpad,
                    timestamp: start,
                },
                runtime.scroll.viewport_height,
            );
            let _ = self.scroll_accumulator.apply_frame(&mut runtime.scroll);
            eprintln!(
                "[cditor][wheel] delta_y={:.2} scroll_top {:.2}->{:.2} interaction={:?} elapsed_ms={:.2}",
                self.last_wheel_delta_y,
                before,
                runtime.scroll.global_scroll_top,
                self.scroll_accumulator.interaction_state,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::gui::app) fn on_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.dragging() && self.image_resize_drag.is_some() {
            if self.update_image_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.gutter_block_drag.is_some() {
            if self.update_gutter_block_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let Some(drag) = self.scrollbar_drag else {
            if event.dragging() {
                if !self.block_drag_selection.is_dragging() {
                    self.update_text_drag_selection(event.position, cx);
                }
            } else {
                self.finish_text_drag_selection();
                self.finish_block_drag_selection();
            }
            return;
        };
        if !event.dragging() {
            self.finish_gui_scrollbar_drag(cx);
            self.finish_text_drag_selection();
            self.finish_block_drag_selection();
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            self.scrollbar_drag = None;
            return;
        };
        let policy = scrollbar_policy(runtime);
        let thumb_top = f64::from(event.position.y) - drag.pointer_y_offset_in_thumb;
        let _ = runtime.drag_scrollbar_to_thumb_top(policy, thumb_top);
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::gui::app) fn on_scrollbar_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commit_image_resize_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_gutter_block_drag(cx) {
            cx.stop_propagation();
        }
        self.finish_gui_scrollbar_drag(cx);
        self.finish_text_drag_selection();
        self.finish_block_drag_selection();
    }
}

pub(in crate::gui::app) fn scroll_delta_y(event: &ScrollWheelEvent) -> f64 {
    match event.delta {
        ScrollDelta::Pixels(delta) => -(f32::from(delta.y) as f64),
        ScrollDelta::Lines(delta) => -(delta.y as f64 * 16.0),
    }
}

fn scroll_phase_from_touch(phase: gpui::TouchPhase) -> ScrollPhase {
    match phase {
        gpui::TouchPhase::Started => ScrollPhase::Began,
        gpui::TouchPhase::Moved => ScrollPhase::Changed,
        gpui::TouchPhase::Ended => ScrollPhase::Ended,
    }
}
