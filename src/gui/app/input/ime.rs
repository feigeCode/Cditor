use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, Size, UTF16Selection, Window, px};

use crate::core::ids::BlockId;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::input_trace::trace_input;
use crate::gui::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};
use crate::gui::text::{platform_index_for_point, platform_range_bounds};
use crate::runtime::DocumentRuntime;

impl EntityInputHandler for CditorV2View {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let runtime = self.ready_runtime()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        let actual = utf8_range_to_utf16_range(&text, &range);
        actual_range.replace(actual.clone());
        trace_input(
            "text_for_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} utf8_range={range:?} actual_utf16={actual:?} text_len={}",
                text.len()
            ),
        );
        text.get(range).map(ToOwned::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let runtime = self.ready_runtime()?;
        let selection = platform_selected_text_range(runtime);
        trace_input(
            "selected_text_range",
            format_args!(
                "focused={:?} selection={selection:?}",
                runtime.focused_block_id()
            ),
        );
        selection
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let marked = runtime
            .active_composition_marked_range()
            .map(|range| utf8_range_to_utf16_range(&text, &range));
        trace_input(
            "marked_text_range",
            format_args!(
                "block={block_id} marked_utf16={marked:?} text_len={}",
                text.len()
            ),
        );
        marked
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(runtime) = self.ready_runtime() {
            runtime.cancel_composition();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        let focused = runtime.focused_block_id();
        let range = ime_replacement_range(runtime, range_utf16.clone());
        trace_input(
            "replace_text_in_range",
            format_args!(
                "focused={focused:?} range_utf16={range_utf16:?} resolved_utf8={range:?} text_len={}",
                text.len()
            ),
        );
        match runtime.replace_text_in_focused_range(range, text) {
            Ok(true) => {
                self.mark_dirty(cx);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => {
                self.save_status = crate::gui::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        let Some(block_id) = runtime.focused_block_id() else {
            return;
        };
        let range_from_ime = ime_replacement_range(runtime, range_utf16.clone());
        let range = range_from_ime
            .clone()
            .unwrap_or_else(|| platform_input_fallback_range(runtime, block_id));
        let selected_range = new_selected_range
            .clone()
            .map(|range| utf16_range_to_utf8_range(new_text, &range));
        trace_input(
            "replace_and_mark_text_in_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} range_from_ime={range_from_ime:?} resolved_utf8={range:?} new_text_len={} new_selected_utf16={new_selected_range:?} selected_utf8={selected_range:?}",
                new_text.len()
            ),
        );
        if runtime
            .begin_or_update_composition_with_selection(block_id, range, new_text, selected_range)
            .is_ok()
        {
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        let cache = self.current_text_layout_cache(runtime, block_id)?;
        platform_range_bounds(cache, range).or(Some(Bounds {
            origin: element_bounds.origin,
            size: Size {
                width: px(1.0),
                height: px(24.0),
            },
        }))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let cache = self.current_text_layout_cache(runtime, block_id)?;
        let utf8 = platform_index_for_point(cache, point).min(text.len());
        let utf16 = utf8_to_utf16_offset(&text, utf8);
        trace_input(
            "character_index_for_point",
            format_args!(
                "block={block_id} point={point:?} utf8={utf8} utf16={utf16} text_len={}",
                text.len()
            ),
        );
        Some(utf16)
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        !self.readonly && matches!(self.state, CditorViewState::Ready(_))
    }
}

pub(in crate::gui::app) fn platform_selected_text_range(
    runtime: &DocumentRuntime,
) -> Option<UTF16Selection> {
    let (_block_id, text) = runtime.focused_text_for_platform_input()?;
    if let Some(selection) = runtime.active_composition_selected_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: false,
        });
    }
    if let Some(marked_range) = runtime.active_composition_marked_range() {
        let caret = utf8_to_utf16_offset(&text, marked_range.end.min(text.len()));
        return Some(UTF16Selection {
            range: caret..caret,
            reversed: false,
        });
    }
    if let Some(selection) = runtime.focused_text_selection_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: false,
        });
    }
    let caret = runtime
        .editing
        .as_ref()
        .map(|editing| editing.caret_anchor.text_offset as usize)
        .unwrap_or(text.len())
        .min(text.len());
    let caret = utf8_to_utf16_offset(&text, caret);
    Some(UTF16Selection {
        range: caret..caret,
        reversed: false,
    })
}

pub(in crate::gui::app) fn platform_input_fallback_range(
    runtime: &DocumentRuntime,
    block_id: BlockId,
) -> Range<usize> {
    runtime
        .active_composition()
        .filter(|composition| composition.block_id == block_id)
        .map(|composition| composition.range_start as usize..composition.range_end as usize)
        .or_else(|| runtime.focused_text_selection_range())
        .unwrap_or_else(|| {
            let caret = runtime
                .editing
                .as_ref()
                .map(|editing| editing.caret_anchor.text_offset as usize)
                .unwrap_or_else(|| runtime.focused_text().map(str::len).unwrap_or(0));
            caret..caret
        })
}

fn ime_replacement_range(
    runtime: &DocumentRuntime,
    range_utf16: Option<Range<usize>>,
) -> Option<Range<usize>> {
    let range_utf16 = range_utf16?;
    let (_block_id, text) = runtime.focused_text_for_platform_input()?;
    let preview_range = utf16_range_to_utf8_range(&text, &range_utf16);
    let Some(composition) = runtime.active_composition() else {
        return Some(preview_range);
    };
    let preview_marked_range = runtime.active_composition_marked_range()?;
    let base_marked_range = composition.range_start as usize..composition.range_end as usize;
    Some(marked_preview_range_to_base_range(
        preview_range,
        base_marked_range,
        preview_marked_range,
    ))
}
