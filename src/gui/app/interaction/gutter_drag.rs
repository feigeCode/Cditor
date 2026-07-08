use std::time::Duration;

use gpui::{AppContext, Context, Pixels, Point, Window};

use crate::core::block::{BlockDropTarget, DragPoint, GutterBlockDragState};
use crate::core::ids::BlockId;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::BlockDragOverlaySnapshot;
use crate::gui::input::BlockDragSelectionController;

use super::geometry::{drop_target_for_document_y_from_rects, parent_drop_target_from_rects};

const GUTTER_DRAG_AUTO_SCROLL_EDGE_PX: f64 = 40.0;
const GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX: f64 = 24.0;
const GUTTER_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;

fn gutter_drag_guideline_y_px(
    rects: &[super::geometry::ProjectedBlockRect],
    target: Option<BlockDropTarget>,
    pointer_document_y: f32,
) -> f32 {
    target
        .and_then(|target| {
            if let Some(block_id) = target.insert_before_block_id {
                rects
                    .iter()
                    .find(|rect| rect.block_id == block_id)
                    .map(|rect| rect.document_top as f32)
            } else {
                rects.last().map(|rect| {
                    if pointer_document_y > rect.document_bottom as f32 {
                        pointer_document_y
                    } else {
                        rect.document_bottom as f32
                    }
                })
            }
        })
        .unwrap_or(pointer_document_y)
}

fn gutter_drag_pointer_document_y(viewport_y: f32, scroll_top: f64) -> f32 {
    viewport_y + scroll_top as f32
}

fn gutter_drag_target_indent_px(
    rects: &[super::geometry::ProjectedBlockRect],
    target: Option<BlockDropTarget>,
) -> f32 {
    target
        .and_then(|target| {
            if let Some(block_id) = target.insert_before_block_id {
                rects
                    .iter()
                    .find(|rect| rect.block_id == block_id)
                    .map(|rect| rect.indent_px)
            } else {
                rects.last().map(|rect| rect.indent_px)
            }
        })
        .unwrap_or(0.0)
}

fn gutter_drag_pointer_document_y_for_view(view: &CditorV2View, viewport_y: f32) -> f32 {
    gutter_drag_pointer_document_y(
        viewport_y,
        view.ready_runtime_ref()
            .map(|runtime| runtime.scroll.global_scroll_top)
            .unwrap_or(0.0),
    )
}

fn gutter_drag_guideline_for_view(view: &CditorV2View, drag: GutterBlockDragState) -> f32 {
    gutter_drag_guideline_y_px(
        &view.projected_block_rects,
        drag.target,
        gutter_drag_pointer_document_y_for_view(view, drag.current_position.y),
    )
}

fn gutter_drag_indent_for_view(view: &CditorV2View, target: Option<BlockDropTarget>) -> f32 {
    gutter_drag_target_indent_px(&view.projected_block_rects, target)
}

pub(in crate::gui::app) fn gutter_drag_auto_scroll_delta(
    pointer_y: f64,
    viewport_height: f64,
) -> f64 {
    if viewport_height <= GUTTER_DRAG_AUTO_SCROLL_EDGE_PX * 2.0 {
        return 0.0;
    }
    if pointer_y < GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        -((GUTTER_DRAG_AUTO_SCROLL_EDGE_PX - pointer_y) / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else if pointer_y > viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        ((pointer_y - (viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX))
            / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else {
        0.0
    }
}

impl CditorV2View {
    pub(crate) fn gutter_mouse_down_from_gui(
        &mut self,
        block_id: BlockId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.gutter_block_drag = Some(GutterBlockDragState::new(
            block_id,
            DragPoint::new(f32::from(position.x), f32::from(position.y)),
        ));
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    pub(in crate::gui::app) fn update_gutter_block_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let point = DragPoint::new(f32::from(position.x), f32::from(position.y));
        let threshold_changed = drag.update_position(point);
        let auto_scrolled = if drag.exceeded_threshold {
            self.apply_gutter_drag_auto_scroll(f64::from(position.y))
        } else {
            false
        };
        self.gutter_block_drag = Some(drag);
        let target_changed = self.refresh_gutter_block_drag_target();
        if self.should_continue_gutter_drag_auto_scroll() {
            self.schedule_gutter_drag_auto_scroll_tick(cx);
        }
        if threshold_changed || target_changed || auto_scrolled {
            cx.notify();
        }
        true
    }

    fn refresh_gutter_block_drag_target(&mut self) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let pointer_document_y = f64::from(drag.current_position.y)
            + self
                .ready_runtime_ref()
                .map(|runtime| runtime.scroll.global_scroll_top)
                .unwrap_or(0.0);
        let target = drag
            .exceeded_threshold
            .then(|| self.drop_target_for_document_y(drag.block_id, pointer_document_y))
            .flatten();
        let target_changed = drag.target != target;
        drag.target = target;
        self.gutter_block_drag = Some(drag);
        target_changed
    }

    fn should_continue_gutter_drag_auto_scroll(&self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        gutter_drag_auto_scroll_delta(
            f64::from(drag.current_position.y),
            runtime.scroll.viewport_height,
        )
        .abs()
            >= f64::EPSILON
    }

    fn schedule_gutter_drag_auto_scroll_tick(&mut self, cx: &mut Context<Self>) {
        if self.gutter_drag_auto_scroll_scheduled {
            return;
        }
        self.gutter_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(GUTTER_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.gutter_drag_auto_scroll_scheduled = false;
                let changed = view.tick_gutter_drag_auto_scroll();
                if changed {
                    cx.notify();
                }
                if view.should_continue_gutter_drag_auto_scroll() {
                    view.schedule_gutter_drag_auto_scroll_tick(cx);
                }
            });
        })
        .detach();
    }

    fn tick_gutter_drag_auto_scroll(&mut self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let auto_scrolled = self.apply_gutter_drag_auto_scroll(f64::from(drag.current_position.y));
        let target_changed = self.refresh_gutter_block_drag_target();
        auto_scrolled || target_changed
    }

    fn apply_gutter_drag_auto_scroll(&mut self, pointer_y: f64) -> bool {
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        let delta = gutter_drag_auto_scroll_delta(pointer_y, runtime.scroll.viewport_height);
        if delta.abs() < f64::EPSILON {
            return false;
        }
        let before = runtime.scroll.global_scroll_top;
        runtime.scroll_by_delta(delta).is_ok() && runtime.scroll.global_scroll_top != before
    }

    pub(in crate::gui::app) fn commit_gutter_block_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.gutter_block_drag.take() else {
            self.gutter_drag_auto_scroll_scheduled = false;
            return false;
        };
        self.gutter_drag_auto_scroll_scheduled = false;
        let Some(target) = drag.target.filter(|_| drag.exceeded_threshold) else {
            cx.notify();
            return true;
        };
        let horizontal_delta = drag.current_position.x - drag.start_position.x;
        let parent_target = (horizontal_delta >= crate::gui::block::chrome::BLOCK_INDENT_STEP_PX)
            .then(|| {
                parent_drop_target_from_rects(&self.projected_block_rects, drag.block_id, target)
            })
            .flatten();
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let moved = if let Some(parent_target) = parent_target {
                runtime
                    .move_block_subtree_to_parent(
                        drag.block_id,
                        Some(parent_target.parent_id),
                        parent_target.sibling_index,
                    )
                    .unwrap_or(false)
            } else {
                runtime
                    .move_block_subtree_before(drag.block_id, target.insert_before_block_id)
                    .unwrap_or(false)
            };
            if moved {
                self.mark_dirty(cx);
            }
        }
        cx.notify();
        true
    }

    fn drop_target_for_document_y(
        &self,
        source_block_id: BlockId,
        document_y: f64,
    ) -> Option<BlockDropTarget> {
        drop_target_for_document_y_from_rects(
            &self.projected_block_rects,
            source_block_id,
            document_y,
        )
    }

    pub(in crate::gui::app) fn block_drag_overlay_snapshot(
        &self,
    ) -> Option<BlockDragOverlaySnapshot> {
        let drag = self.gutter_block_drag?;
        if !drag.exceeded_threshold {
            return None;
        }

        let y_px = gutter_drag_guideline_for_view(self, drag);
        let indent_px = gutter_drag_indent_for_view(self, drag.target);

        Some(BlockDragOverlaySnapshot {
            y_px,
            indent_px,
            visible: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::interaction::geometry::ProjectedBlockRect;

    fn rect(block_id: BlockId, top: f64, bottom: f64) -> ProjectedBlockRect {
        ProjectedBlockRect {
            block_id,
            visible_index: block_id as usize - 1,
            depth: 0,
            document_top: top,
            document_bottom: bottom,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
        }
    }

    #[test]
    fn gutter_drag_guideline_uses_document_target_coordinates() {
        let rects = vec![rect(1, 100.0, 132.0), rect(2, 132.0, 164.0)];

        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: Some(2),
                    target_visible_index: 1,
                }),
                12.0,
            ),
            132.0,
        );
        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                }),
                150.0,
            ),
            164.0,
        );
        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                }),
                220.0,
            ),
            220.0,
        );
    }
}
