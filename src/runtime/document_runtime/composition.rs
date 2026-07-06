use super::*;

impl DocumentRuntime {
    pub fn active_composition(&self) -> Option<&CompositionState> {
        self.editing.as_ref()?.composition.as_ref()
    }

    pub fn composition_preview_text(&self) -> Option<String> {
        let composition = self.active_composition()?;
        let model = self.text_models.get(&composition.block_id)?;
        let range = safe_char_range(
            model.text(),
            composition.range_start as usize..composition.range_end as usize,
        );
        let mut preview = String::with_capacity(
            model.text().len() - (range.end - range.start) + composition.preview_text.len(),
        );
        preview.push_str(&model.text()[..range.start]);
        preview.push_str(&composition.preview_text);
        preview.push_str(&model.text()[range.end..]);
        Some(preview)
    }

    pub fn focused_text_for_platform_input(&self) -> Option<(BlockId, String)> {
        let block_id = self.focused_block_id()?;
        if self
            .active_composition()
            .is_some_and(|composition| composition.block_id == block_id)
        {
            return self.composition_preview_text().map(|text| (block_id, text));
        }
        self.focused_text_owned()
    }

    pub fn active_composition_marked_range(&self) -> Option<std::ops::Range<usize>> {
        let composition = self.active_composition()?;
        let model = self.text_models.get(&composition.block_id)?;
        let range = safe_char_range(
            model.text(),
            composition.range_start as usize..composition.range_end as usize,
        );
        Some(range.start..range.start + composition.preview_text.len())
    }

    pub fn active_composition_selected_range(&self) -> Option<std::ops::Range<usize>> {
        let composition = self.active_composition()?;
        let start = composition.selected_range_start? as usize;
        let end = composition.selected_range_end? as usize;
        let preview_text = self.composition_preview_text()?;
        Some(safe_char_range(&preview_text, start..end))
    }

    pub fn begin_or_update_composition(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
    ) -> Result<(), String> {
        self.begin_or_update_composition_with_selection(block_id, range, preview_text, None)
    }

    pub fn begin_or_update_composition_with_selection(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
        selected_range: Option<Range<usize>>,
    ) -> Result<(), String> {
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let requested_range = range.clone();
        let range = safe_char_range(model.text(), range);
        let preview_text = preview_text.into();
        let marked_range = range.start..range.start + preview_text.len();
        let selected_range = selected_range
            .map(|selected| safe_char_range(&preview_text, selected))
            .map(|selected| marked_range.start + selected.start..marked_range.start + selected.end);
        let caret_offset = selected_range
            .as_ref()
            .map(|selected| selected.end)
            .unwrap_or(marked_range.end);
        trace_input(
            "begin_or_update_composition",
            format_args!(
                "block={block_id} requested_range={requested_range:?} clamped_range={range:?} preview_len={} selected_range={selected_range:?} caret_offset={caret_offset} text_len={}",
                preview_text.len(),
                model.len()
            ),
        );
        self.document_selection = None;
        self.focused_text_selection = None;
        let editing = self.editing.as_mut().expect("editing session exists");
        editing
            .update_composition(CompositionState {
                block_id,
                range_start: range.start as u64,
                range_end: range.end as u64,
                preview_text,
                selected_range_start: selected_range.as_ref().map(|range| range.start as u64),
                selected_range_end: selected_range.as_ref().map(|range| range.end as u64),
            })
            .map_err(|error| format!("{error:?}"))?;
        editing.caret_anchor.text_offset = caret_offset as u64;
        editing.caret_anchor.block_id = block_id;
        Ok(())
    }

    pub fn cancel_composition(&mut self) {
        if let Some(editing) = self.editing.as_mut() {
            editing.clear_composition();
        }
    }

    pub fn commit_composition(&mut self) -> Result<bool, String> {
        let Some(composition) = self
            .editing
            .as_ref()
            .and_then(|editing| editing.composition.clone())
        else {
            return Ok(false);
        };
        let block_id = composition.block_id;
        self.push_undo_snapshot(block_id)?;
        let model = self
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let range = safe_char_range(
            model.text(),
            composition.range_start as usize..composition.range_end as usize,
        );
        let replaced_range = range.clone();
        let preview_text = composition.preview_text.clone();
        let inserted = model
            .replace_range(range, &preview_text)
            .map_err(|error| format!("{error:?}"))?;
        let editing = self.editing.as_mut().expect("editing session exists");
        editing.clear_composition();
        editing.content_version += 1;
        editing.caret_anchor.text_offset = inserted.end as u64;
        sync_payload_from_model_after_replace(
            &mut self.payload_window,
            block_id,
            editing.content_version,
            model,
            replaced_range,
            &preview_text,
        );
        Ok(true)
    }

    pub(super) fn payload_with_composition_preview(
        &self,
        block_id: BlockId,
        mut payload: BlockPayloadRecord,
    ) -> BlockPayloadRecord {
        if self
            .active_composition()
            .is_some_and(|composition| composition.block_id == block_id)
            && let Some(preview_text) = self.composition_preview_text()
        {
            payload.payload = text_payload_for_existing(&payload.payload, &preview_text);
        }
        payload
    }
}
