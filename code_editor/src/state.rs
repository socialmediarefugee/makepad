use {
    crate::{diff::Strategy, Diff, Point, Position, Range, Rect, Selection, Settings, Size, Text},
    std::{
        collections::{HashMap, HashSet},
        ops::ControlFlow,
        slice,
    },
};

#[derive(Clone, Debug, Default)]
pub struct State {
    settings: Settings,
    session_id: usize,
    sessions: HashMap<SessionId, Session>,
    document_id: usize,
    documents: HashMap<DocumentId, Document>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_settings(settings: Settings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    pub fn view(&self, session_id: SessionId) -> View<'_> {
        let session = &self.sessions[&session_id];
        let document = &self.documents[&session.document_id];
        View {
            settings: &self.settings,
            text: &document.text,
            inline_inlays: &document.inline_inlays,
            soft_breaks: &session.soft_breaks,
            scale: &session.scale,
            fold_column_index: &session.fold_column_index,
            block_inlays: &document.block_inlays,
            summed_heights: &session.summed_heights,
            selections: &session.selections,
        }
    }

    pub fn view_mut(&mut self, session_id: SessionId) -> ViewMut<'_> {
        let session = self.sessions.get_mut(&session_id).unwrap();
        let document = self.documents.get_mut(&session.document_id).unwrap();
        ViewMut {
            settings: &mut self.settings,
            text: &mut document.text,
            inline_inlays: &mut document.inline_inlays,
            soft_breaks: &mut session.soft_breaks,
            scale: &mut session.scale,
            fold_column_index: &mut session.fold_column_index,
            block_inlays: &mut document.block_inlays,
            summed_heights: &mut session.summed_heights,
            selections: &mut session.selections,
            last_added_selection_index: &mut session.last_added_selection_index,
            folding_lines: &mut session.folding_lines,
            unfolding_lines: &mut session.unfolding_lines,
        }
    }

    pub fn open_session(&mut self) -> SessionId {
        let document_id = self.open_document();
        let session_id = SessionId(self.session_id);
        self.session_id += 1;
        let line_count = self.documents[&document_id].text.as_lines().len();
        self.sessions.insert(
            session_id,
            Session {
                document_id,
                soft_breaks: (0..line_count).map(|_| [].into()).collect(),
                fold_column_index: (0..line_count).map(|_| 0).collect(),
                scale: (0..line_count).map(|_| 1.0).collect(),
                summed_heights: Vec::new(),
                selections: vec![Selection::default()],
                last_added_selection_index: 0,
                folding_lines: HashSet::new(),
                unfolding_lines: HashSet::new(),
            },
        );
        let mut view = self.view_mut(session_id);
        view.update_summed_heights();
        session_id
    }

    fn open_document(&mut self) -> DocumentId {
        let document_id = DocumentId(self.document_id);
        self.document_id += 1;
        let text: Text = include_str!("state.rs").into();
        let line_count = text.as_lines().len();
        self.documents.insert(
            document_id,
            Document {
                text,
                inline_inlays: (0..line_count)
                    .map(|line_index| {
                        if line_index % 2 == 0 {
                            [
                                (20, InlineInlay::Text("uvw xyz".into())),
                                (40, InlineInlay::Text("uvw xyz".into())),
                                (60, InlineInlay::Text("uvw xyz".into())),
                                (80, InlineInlay::Text("uvw xyz".into())),
                            ]
                            .into()
                        } else {
                            [].into()
                        }
                    })
                    .collect(),
                block_inlays: [
                    /*
                    (10, BlockInlay::Line(LineInlay::new("UVW XYZ".into()))),
                    (20, BlockInlay::Line(LineInlay::new("UVW XYZ".into()))),
                    (30, BlockInlay::Line(LineInlay::new("UVW XYZ".into()))),
                    (40, BlockInlay::Line(LineInlay::new("UVW XYZ".into()))),
                    */
                ]
                .into(),
            },
        );
        document_id
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View<'a> {
    settings: &'a Settings,
    text: &'a Text,
    inline_inlays: &'a [Vec<(usize, InlineInlay)>],
    soft_breaks: &'a [Vec<usize>],
    fold_column_index: &'a [usize],
    scale: &'a [f64],
    summed_heights: &'a [f64],
    block_inlays: &'a [(usize, BlockInlay)],
    selections: &'a [Selection],
}

impl<'a> View<'a> {
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn text(&self) -> &Text {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.text.as_lines().len()
    }

    pub fn line(&self, line_index: usize) -> Line<'a> {
        Line {
            text: &self.text.as_lines()[line_index],
            inline_inlays: &self.inline_inlays[line_index],
            soft_breaks: &self.soft_breaks[line_index],
            fold_column_index: self.fold_column_index[line_index],
            scale: self.scale[line_index],
        }
    }

    pub fn lines(&self, start_line_index: usize, end_line_index: usize) -> Lines<'a> {
        Lines {
            text: self.text.as_lines()[start_line_index..end_line_index].iter(),
            inline_inlays: self.inline_inlays[start_line_index..end_line_index].iter(),
            soft_breaks: self.soft_breaks[start_line_index..end_line_index].iter(),
            fold_column_index: self.fold_column_index[start_line_index..end_line_index].iter(),
            scale: self.scale[start_line_index..end_line_index].iter(),
        }
    }

    pub fn blocks(&self, start_line_index: usize, end_line_index: usize) -> Blocks<'a> {
        Blocks {
            lines: self.lines(start_line_index, end_line_index),
            block_inlays: self.block_inlays[self
                .block_inlays
                .iter()
                .position(|&(index, _)| index >= start_line_index)
                .unwrap_or(self.block_inlays.len())..]
                .iter(),
            line_index: start_line_index,
        }
    }

    pub fn width(&self, tab_column_count: usize) -> f64 {
        let mut max_column_count = 0.0f64;
        for block in self.blocks(0, self.line_count()) {
            max_column_count = max_column_count.max(block.width(tab_column_count));
        }
        max_column_count
    }

    pub fn height(&self) -> f64 {
        self.summed_heights[self.line_count() - 1]
    }

    pub fn find_first_line_ending_after_y(&self, y: f64) -> usize {
        match self
            .summed_heights
            .binary_search_by(|summed_height| summed_height.partial_cmp(&y).unwrap())
        {
            Ok(line_index) => line_index + 1,
            Err(line_index) => line_index,
        }
    }

    pub fn find_first_line_starting_after_y(&self, y: f64) -> usize {
        match self
            .summed_heights
            .binary_search_by(|summed_height| summed_height.partial_cmp(&y).unwrap())
        {
            Ok(line_index) => line_index + 1,
            Err(line_index) => {
                if line_index == self.line_count() {
                    line_index
                } else {
                    line_index + 1
                }
            }
        }
    }

    pub fn layout<T>(
        &self,
        start_line_index: usize,
        end_line_index: usize,
        mut handle_event: impl FnMut(LayoutEvent<'_>) -> ControlFlow<T, bool>,
    ) -> ControlFlow<T, bool> {
        use crate::str::StrExt;

        let mut y = if start_line_index == 0 {
            0.0
        } else {
            self.summed_heights[start_line_index - 1]
        };
        for block in self.blocks(start_line_index, end_line_index) {
            match block {
                Block::Line { is_inlay, line } => {
                    if !handle_event(LayoutEvent {
                        rect: Rect::new(
                            Point::new(0.0, y),
                            Size::new(line.width(self.settings.tab_column_count), line.height()),
                        ),
                        kind: LayoutEventKind::Line { is_inlay, line },
                    })? {
                        y += line.height();
                        continue;
                    }
                    let mut column_index = 0;
                    for wrapped_inline in line.wrapped_inlines() {
                        match wrapped_inline {
                            WrappedInline::Inline(inline) => match inline {
                                Inline::Text { is_inlay, text } => {
                                    for grapheme in text.graphemes() {
                                        let x = line.column_index_to_x(column_index);
                                        let next_column_index = column_index
                                            + grapheme.column_count(self.settings.tab_column_count);
                                        handle_event(LayoutEvent {
                                            rect: Rect::new(
                                                Point::new(x, y),
                                                Size::new(
                                                    line.column_index_to_x(next_column_index) - x,
                                                    line.scale(),
                                                ),
                                            ),
                                            kind: LayoutEventKind::Grapheme {
                                                is_inlay,
                                                text: grapheme,
                                            },
                                        })?;
                                        column_index = next_column_index;
                                    }
                                }
                                Inline::Widget(widget) => {
                                    let x = line.column_index_to_x(column_index);
                                    let next_column_index = column_index + widget.column_count;
                                    handle_event(LayoutEvent {
                                        rect: Rect::new(
                                            Point::new(x, y),
                                            Size::new(
                                                line.column_index_to_x(next_column_index) - x,
                                                line.scale(),
                                            ),
                                        ),
                                        kind: LayoutEventKind::Widget { id: widget.id },
                                    })?;
                                    column_index = next_column_index;
                                }
                            },
                            WrappedInline::SoftBreak => {
                                let x = line.column_index_to_x(column_index);
                                handle_event(LayoutEvent {
                                    rect: Rect::new(
                                        Point::new(x, y),
                                        Size::new(
                                            line.column_index_to_x(column_index + 1) - x,
                                            line.scale(),
                                        ),
                                    ),
                                    kind: LayoutEventKind::Break { is_soft: true },
                                })?;
                                y += line.scale();
                                column_index = 0;
                            }
                        }
                    }
                    let x = line.column_index_to_x(column_index);
                    handle_event(LayoutEvent {
                        rect: Rect::new(
                            Point::new(x, y),
                            Size::new(line.column_index_to_x(column_index + 1) - x, line.scale()),
                        ),
                        kind: LayoutEventKind::Break { is_soft: false },
                    })?;
                    y += line.scale();
                }
                Block::Widget(widget) => {
                    handle_event(LayoutEvent {
                        rect: Rect::new(Point::new(0.0, y), widget.size),
                        kind: LayoutEventKind::Widget { id: widget.id },
                    })?;
                    y += widget.size.height;
                }
            }
        }
        ControlFlow::Continue(true)
    }

    pub fn pick(&self, point: Point) -> Option<Position> {
        let line_index = self.find_first_line_ending_after_y(point.y);
        let mut position = Position::new(line_index, 0);
        match self.layout(line_index, line_index + 1, |event| {
            match event.kind {
                LayoutEventKind::Line { is_inlay: true, .. } => {
                    if event.rect.contains(point) {
                        return ControlFlow::Break(Some(position));
                    }
                    return ControlFlow::Continue(false);
                }
                LayoutEventKind::Grapheme { is_inlay, text } => {
                    let half_width = event.rect.size.width / 2.0;
                    let half_width_size = Size::new(half_width, event.rect.size.height);
                    if Rect::new(event.rect.origin, half_width_size).contains(point) {
                        return ControlFlow::Break(Some(position));
                    }
                    if !is_inlay {
                        position.byte_index += text.len();
                    }
                    if Rect::new(
                        Point::new(event.rect.origin.x + half_width, event.rect.origin.y),
                        half_width_size,
                    )
                    .contains(point)
                    {
                        return ControlFlow::Break(Some(position));
                    }
                }
                LayoutEventKind::Break { is_soft: false } => {
                    if point.y >= event.rect.origin.y
                        && point.y <= event.rect.origin.y + event.rect.size.height
                    {
                        return ControlFlow::Break(Some(position));
                    }
                    position.line_index += 1;
                    position.byte_index = 0;
                }
                LayoutEventKind::Widget { .. } => {
                    return ControlFlow::Break(None);
                }
                _ => {}
            }
            ControlFlow::Continue(true)
        }) {
            ControlFlow::Continue(_) => None,
            ControlFlow::Break(position) => position,
        }
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }
}

#[derive(Debug)]
pub struct ViewMut<'a> {
    settings: &'a mut Settings,
    text: &'a mut Text,
    inline_inlays: &'a mut Vec<Vec<(usize, InlineInlay)>>,
    soft_breaks: &'a mut Vec<Vec<usize>>,
    scale: &'a mut Vec<f64>,
    fold_column_index: &'a mut Vec<usize>,
    block_inlays: &'a mut Vec<(usize, BlockInlay)>,
    summed_heights: &'a mut Vec<f64>,
    selections: &'a mut Vec<Selection>,
    last_added_selection_index: &'a mut usize,
    folding_lines: &'a mut HashSet<usize>,
    unfolding_lines: &'a mut HashSet<usize>,
}

impl<'a> ViewMut<'a> {
    pub fn as_view(&self) -> View<'_> {
        View {
            settings: &self.settings,
            text: &self.text,
            inline_inlays: &self.inline_inlays,
            soft_breaks: &self.soft_breaks,
            scale: self.scale,
            fold_column_index: self.fold_column_index,
            summed_heights: &self.summed_heights,
            block_inlays: &self.block_inlays,
            selections: &self.selections,
        }
    }

    pub fn wrap_lines(&mut self, max_column_count: usize, tab_column_count: usize) {
        use {crate::str::StrExt, std::mem};

        for line_index in 0..self.as_view().line_count() {
            let old_soft_break_count = self.soft_breaks[line_index].len();
            self.soft_breaks[line_index].clear();
            let mut soft_breaks = mem::take(&mut self.soft_breaks[line_index]);
            let mut inlay_byte_index = 0;
            let mut column_count = 0;
            for inline in self.as_view().line(line_index).inlines() {
                if let Inline::Text { text, .. } = inline {
                    for string in text.split_whitespace_boundaries() {
                        let mut next_column_count =
                            column_count + string.column_count(tab_column_count);
                        if next_column_count > max_column_count
                            && soft_breaks.last().copied().unwrap_or(0) != inlay_byte_index
                        {
                            next_column_count = 0;
                            soft_breaks.push(inlay_byte_index);
                        }
                        inlay_byte_index += string.len();
                        column_count = next_column_count;
                    }
                } else {
                    let mut next_column_count =
                        column_count + inline.column_count(tab_column_count);
                    if next_column_count > max_column_count
                        && soft_breaks.last().copied().unwrap_or(0) != inlay_byte_index
                    {
                        next_column_count = 0;
                        soft_breaks.push(inlay_byte_index);
                    }
                    column_count = next_column_count;
                }
            }
            self.soft_breaks[line_index] = soft_breaks;
            if self.soft_breaks[line_index].len() != old_soft_break_count {
                self.summed_heights.truncate(line_index);
            }
        }
        self.update_summed_heights();
    }

    pub fn set_cursor(&mut self, cursor: Position) {
        self.selections.clear();
        self.selections.push(Selection::from_cursor(cursor));
        *self.last_added_selection_index = 0;
    }

    pub fn add_cursor(&mut self, cursor: Position) {
        use std::cmp::Ordering;

        let selection = Selection::from_cursor(cursor);
        *self.last_added_selection_index = match self.selections.binary_search_by(|selection| {
            if selection.end() <= cursor {
                return Ordering::Less;
            }
            if selection.start() >= cursor {
                return Ordering::Greater;
            }
            Ordering::Equal
        }) {
            Ok(index) => {
                self.selections[index] = selection;
                index
            }
            Err(index) => {
                self.selections.insert(index, selection);
                index
            }
        };
    }

    pub fn move_cursor_to(&mut self, select: bool, cursor: Position) {
        let mut current_index = *self.last_added_selection_index;
        self.selections[current_index].cursor = cursor;
        if !select {
            self.selections[current_index].anchor = cursor;
        }
        while current_index > 0 {
            let previous_index = current_index - 1;
            let previous_selection = self.selections[current_index - 1];
            let current_selection = self.selections[current_index];
            if previous_selection.should_merge(current_selection) {
                self.selections.remove(previous_index);
                current_index -= 1;
            } else {
                break;
            }
        }
        while current_index + 1 < self.selections.len() {
            let next_index = current_index + 1;
            let current_selection = self.selections[current_index];
            let next_selection = self.selections[next_index];
            if current_selection.should_merge(next_selection) {
                self.selections.remove(next_index);
            } else {
                break;
            }
        }
        *self.last_added_selection_index = current_index;
    }

    pub fn move_cursors_left(&mut self, select: bool) {
        use crate::move_ops;

        self.modify_selections(select, |view, selection| {
            selection
                .update_cursor(|position, _| (move_ops::move_left(view.text(), position), None))
        });
    }

    pub fn move_cursors_right(&mut self, select: bool) {
        use crate::move_ops;

        self.modify_selections(select, |view, selection| {
            selection
                .update_cursor(|position, _| (move_ops::move_right(view.text(), position), None))
        });
    }

    pub fn move_cursors_up(&mut self, select: bool) {
        use crate::move_ops;

        self.modify_selections(select, |view, selection| {
            selection.update_cursor(|position, column_index| {
                move_ops::move_up(view, position, column_index)
            })
        });
    }

    pub fn move_cursors_down(&mut self, select: bool) {
        use crate::move_ops;

        self.modify_selections(select, |view, selection| {
            selection.update_cursor(|position, column_index| {
                move_ops::move_down(view, position, column_index)
            })
        });
    }

    pub fn replace(&mut self, replace_with: Text) {
        use crate::edit_ops;

        self.modify_text(|_, range| edit_ops::replace(range, replace_with.clone()))
    }

    pub fn enter(&mut self) {
        self.replace('\n'.into())
    }

    pub fn delete(&mut self) {
        use crate::edit_ops;

        self.modify_text(|_, range| edit_ops::delete(range))
    }

    pub fn backspace(&mut self) {
        use crate::edit_ops;

        self.modify_text(edit_ops::backspace)
    }

    pub fn fold_line(&mut self, line_index: usize) {
        self.unfolding_lines.remove(&line_index);
        self.folding_lines.insert(line_index);
    }

    pub fn unfold_line(&mut self, line_index: usize) {
        self.folding_lines.remove(&line_index);
        self.unfolding_lines.insert(line_index);
    }

    pub fn update_fold_animations(&mut self) -> bool {
        use std::mem;

        if self.folding_lines.is_empty() && self.unfolding_lines.is_empty() {
            return false;
        }
        let folding_lines = mem::take(self.folding_lines);
        let mut new_folding_lines = HashSet::new();
        for line in folding_lines {
            self.scale[line] *= 0.9;
            if self.scale[line] < 0.001 {
                self.scale[line] = 0.0;
            } else {
                new_folding_lines.insert(line);
            }
            self.summed_heights.truncate(line);
        }
        *self.folding_lines = new_folding_lines;
        let unfolding_lines = mem::take(self.unfolding_lines);
        let mut new_unfolding_lines = HashSet::new();
        for line in unfolding_lines {
            self.scale[line] = 1.0 - 0.9 * (1.0 - self.scale[line]);
            if self.scale[line] > 1.0 - 0.001 {
                self.scale[line] = 1.0;
            } else {
                new_unfolding_lines.insert(line);
            }
            self.summed_heights.truncate(line);
        }
        *self.unfolding_lines = new_unfolding_lines;
        self.update_summed_heights();
        true
    }

    fn modify_selections(
        &mut self,
        select: bool,
        mut f: impl FnMut(&View<'_>, Selection) -> Selection,
    ) {
        use std::mem;

        let mut selections = mem::take(self.selections);
        let view = self.as_view();
        for selection in &mut selections {
            *selection = f(&view, *selection);
            if !select {
                *selection = selection.reset_anchor();
            }
        }
        *self.selections = selections;
        let mut current_index = 0;
        while current_index + 1 < self.selections.len() {
            let next_index = current_index + 1;
            let current_selection = self.selections[current_index];
            let next_selection = self.selections[next_index];
            assert!(current_selection.start() <= next_selection.start());
            if !current_selection.should_merge(next_selection) {
                current_index += 1;
                continue;
            }
            let winner_index;
            let loser_index;
            if next_index == *self.last_added_selection_index {
                winner_index = next_index;
                loser_index = current_index;
            } else {
                winner_index = current_index;
                loser_index = next_index;
            }
            let winner_selection = self.selections[winner_index];
            let start = current_selection.start().min(next_selection.start());
            let end = current_selection.end().max(next_selection.end());
            let anchor;
            let cursor;
            if winner_selection.anchor <= winner_selection.cursor {
                anchor = start;
                cursor = end;
            } else {
                anchor = end;
                cursor = start;
            }
            self.selections[current_index] =
                Selection::new(anchor, cursor, winner_selection.column_index);
            self.selections.remove(loser_index);
            if loser_index < *self.last_added_selection_index {
                *self.last_added_selection_index -= 1;
            }
        }
    }

    fn modify_text(&mut self, mut f: impl FnMut(&mut Text, Range) -> Diff) {
        let mut composite_diff = Diff::new();
        let mut prev_end = Position::origin();
        let mut diffed_prev_end = Position::origin();
        for selection in &mut *self.selections {
            let distance_from_prev_end = selection.start() - prev_end;
            let diffed_start = diffed_prev_end + distance_from_prev_end;
            let diffed_end = diffed_start + selection.length();
            let diff = f(&mut self.text, Range::new(diffed_start, diffed_end));
            let diffed_start = diffed_start.apply_diff(&diff, Strategy::InsertBefore);
            let diffed_end = diffed_end.apply_diff(&diff, Strategy::InsertBefore);
            self.text.apply_diff(diff.clone());
            composite_diff = composite_diff.compose(diff);
            prev_end = selection.end();
            diffed_prev_end = diffed_end;
            *selection = if selection.anchor <= selection.cursor {
                Selection::new(diffed_start, diffed_end, selection.column_index)
            } else {
                Selection::new(diffed_end, diffed_start, selection.column_index)
            };
        }
        self.update_after_modify_text(composite_diff);
    }

    fn update_after_modify_text(&mut self, diff: Diff) {
        use crate::diff::OperationInfo;

        let mut position = Position::origin();
        for operation in diff {
            match operation.info() {
                OperationInfo::Delete(length) => {
                    let start_line_index = position.line_index + 1;
                    let end_line_index = start_line_index + length.line_count;
                    self.inline_inlays.drain(start_line_index..end_line_index);
                    self.soft_breaks.drain(start_line_index..end_line_index);
                    self.fold_column_index
                        .drain(start_line_index..end_line_index);
                    self.scale.drain(start_line_index..end_line_index);
                    self.summed_heights.truncate(start_line_index);
                }
                OperationInfo::Retain(length) => {
                    position += length;
                }
                OperationInfo::Insert(length) => {
                    let line_index = position.line_index + 1;
                    self.inline_inlays.splice(
                        line_index..line_index,
                        (0..length.line_count).map(|_| Vec::new()),
                    );
                    self.soft_breaks.splice(
                        line_index..line_index,
                        (0..length.line_count).map(|_| Vec::new()),
                    );
                    self.fold_column_index
                        .splice(line_index..line_index, (0..length.line_count).map(|_| 0));
                    self.scale
                        .splice(line_index..line_index, (0..length.line_count).map(|_| 1.0));
                    self.summed_heights.truncate(position.line_index);
                }
            }
        }
        self.update_summed_heights();
    }

    fn update_summed_heights(&mut self) {
        use std::mem;

        let start_line_index = self.summed_heights.len();
        let mut summed_height = if start_line_index == 0 {
            0.0
        } else {
            self.summed_heights[start_line_index - 1]
        };
        let mut summed_heights = mem::take(self.summed_heights);
        for block in self
            .as_view()
            .blocks(start_line_index, self.as_view().line_count())
        {
            summed_height += block.height();
            if let Block::Line {
                is_inlay: false, ..
            } = block
            {
                summed_heights.push(summed_height);
            }
        }
        *self.summed_heights = summed_heights;
    }
}

#[derive(Debug)]
pub struct Lines<'a> {
    text: slice::Iter<'a, String>,
    inline_inlays: slice::Iter<'a, Vec<(usize, InlineInlay)>>,
    soft_breaks: slice::Iter<'a, Vec<usize>>,
    fold_column_index: slice::Iter<'a, usize>,
    scale: slice::Iter<'a, f64>,
}

impl<'a> Clone for Lines<'a> {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Line {
            text: self.text.next()?,
            inline_inlays: self.inline_inlays.next()?,
            soft_breaks: self.soft_breaks.next()?,
            fold_column_index: *self.fold_column_index.next()?,
            scale: *self.scale.next()?,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Line<'a> {
    text: &'a str,
    inline_inlays: &'a [(usize, InlineInlay)],
    soft_breaks: &'a [usize],
    fold_column_index: usize,
    scale: f64,
}

impl<'a> Line<'a> {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn inlines(&self) -> Inlines<'a> {
        Inlines {
            text: self.text,
            inline_inlays: self.inline_inlays.iter(),
            byte_index: 0,
        }
    }

    pub fn wrapped_inlines(&self) -> WrappedInlines<'a> {
        let mut inlines = self.inlines();
        WrappedInlines {
            inline: inlines.next(),
            inlines,
            soft_breaks: self.soft_breaks.iter(),
            byte_index: 0,
        }
    }

    pub fn fold_column_index(&self) -> usize {
        self.fold_column_index
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn column_count(&self, tab_column_count: usize) -> usize {
        let mut max_summed_column_count = 0;
        let mut summed_column_count = 0;
        for wrapped_inline in self.wrapped_inlines() {
            match wrapped_inline {
                WrappedInline::Inline(inline) => {
                    summed_column_count += inline.column_count(tab_column_count);
                }
                WrappedInline::SoftBreak => {
                    max_summed_column_count = max_summed_column_count.max(summed_column_count);
                    summed_column_count = 0;
                }
            }
        }
        max_summed_column_count = max_summed_column_count.max(summed_column_count);
        max_summed_column_count
    }

    pub fn row_count(&self) -> usize {
        self.soft_breaks.len() + 1
    }

    pub fn byte_index_to_inlay_byte_index(&self, byte_index: usize) -> usize {
        let mut inlay_byte_index = byte_index;
        for &(current_byte_index, ref inline_inlay) in self.inline_inlays {
            if current_byte_index > byte_index {
                break;
            }
            match inline_inlay {
                InlineInlay::Text(text) => {
                    inlay_byte_index += text.len();
                }
                _ => {}
            }
        }
        inlay_byte_index
    }

    pub fn is_at_first_row(&self, byte_index: usize) -> bool {
        self.soft_breaks.first().map_or(true, |&soft_break| {
            self.byte_index_to_inlay_byte_index(byte_index) < soft_break
        })
    }

    pub fn is_at_last_row(&self, byte_index: usize) -> bool {
        self.soft_breaks.last().map_or(true, |&soft_break| {
            soft_break <= self.byte_index_to_inlay_byte_index(byte_index)
        })
    }

    pub fn byte_index_to_row_column_index(
        &self,
        byte_index: usize,
        tab_column_count: usize,
    ) -> (usize, usize) {
        use crate::str::StrExt;

        let mut current_byte_index = 0;
        let mut row_index = 0;
        let mut column_index = 0;
        for wrapped_inline in self.wrapped_inlines() {
            match wrapped_inline {
                WrappedInline::Inline(inline) => match inline {
                    Inline::Text {
                        is_inlay: false,
                        text,
                    } => {
                        for grapheme in text.graphemes() {
                            if current_byte_index == byte_index {
                                return (row_index, column_index);
                            }
                            current_byte_index += grapheme.len();
                            column_index += grapheme.column_count(tab_column_count);
                        }
                    }
                    _ => {
                        column_index += inline.column_count(tab_column_count);
                    }
                },
                WrappedInline::SoftBreak => {
                    row_index += 1;
                    column_index = 0;
                }
            }
        }
        (row_index, column_index)
    }

    pub fn row_column_index_to_byte_index(
        &self,
        row_index: usize,
        column_index: usize,
        tab_column_count: usize,
    ) -> usize {
        use crate::str::StrExt;

        let mut byte_index = 0;
        let mut current_row_index = 0;
        let mut current_column_index = 0;
        for wrapped_inline in self.wrapped_inlines() {
            match wrapped_inline {
                WrappedInline::Inline(inline) => match inline {
                    Inline::Text {
                        is_inlay: false,
                        text,
                    } => {
                        for grapheme in text.graphemes() {
                            let next_column_index =
                                current_column_index + grapheme.column_count(tab_column_count);
                            if current_row_index == row_index && next_column_index > column_index {
                                return byte_index;
                            }
                            byte_index += grapheme.len();
                            current_column_index = next_column_index;
                        }
                    }
                    _ => {
                        if current_row_index == row_index && current_column_index > column_index {
                            return byte_index;
                        }
                        current_column_index += inline.column_count(tab_column_count);
                    }
                },
                WrappedInline::SoftBreak => {
                    if current_row_index == row_index {
                        let (byte_index, _) = self.text[..byte_index]
                            .grapheme_indices()
                            .next_back()
                            .unwrap();
                        return byte_index;
                    }
                    current_row_index += 1;
                    current_column_index = 0;
                }
            }
        }
        byte_index
    }

    pub fn width(&self, tab_column_count: usize) -> f64 {
        self.column_index_to_x(self.column_count(tab_column_count))
    }

    pub fn height(&self) -> f64 {
        self.scale * self.row_count() as f64
    }

    pub fn column_index_to_x(&self, column_index: usize) -> f64 {
        let column_count_before_fold_column_index = column_index.min(self.fold_column_index);
        let column_count_after_fold_column_index =
            column_index - column_count_before_fold_column_index;
        column_count_before_fold_column_index as f64
            + self.scale * column_count_after_fold_column_index as f64
    }
}

#[derive(Clone, Debug)]
pub struct Inlines<'a> {
    text: &'a str,
    inline_inlays: slice::Iter<'a, (usize, InlineInlay)>,
    byte_index: usize,
}

impl<'a> Iterator for Inlines<'a> {
    type Item = Inline<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self
            .inline_inlays
            .as_slice()
            .first()
            .map_or(false, |&(byte_index, _)| byte_index == self.byte_index)
        {
            let (_, inline_inlay) = self.inline_inlays.next().unwrap();
            return Some(match inline_inlay {
                InlineInlay::Text(text) => Inline::Text {
                    is_inlay: true,
                    text,
                },
                InlineInlay::Widget(widget) => Inline::Widget(widget),
            });
        }
        if self.text.is_empty() {
            return None;
        }
        let mut byte_count = self.text.len();
        if let Some(&(byte_index, _)) = self.inline_inlays.as_slice().first() {
            byte_count = byte_count.min(byte_index - self.byte_index);
        }
        let (text, remaining_text) = self.text.split_at(byte_count);
        self.text = remaining_text;
        self.byte_index += text.len();
        Some(Inline::Text {
            is_inlay: false,
            text,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Inline<'a> {
    Text { is_inlay: bool, text: &'a str },
    Widget(&'a InlineWidget),
}

impl<'a> Inline<'a> {
    pub fn column_count(&self, tab_column_count: usize) -> usize {
        use crate::str::StrExt;

        match self {
            Self::Text { text, .. } => text.column_count(tab_column_count),
            Self::Widget(widget) => widget.column_count,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WrappedInlines<'a> {
    inline: Option<Inline<'a>>,
    inlines: Inlines<'a>,
    soft_breaks: slice::Iter<'a, usize>,
    byte_index: usize,
}

impl<'a> Iterator for WrappedInlines<'a> {
    type Item = WrappedInline<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self
            .soft_breaks
            .as_slice()
            .first()
            .map_or(false, |&byte_index| byte_index == self.byte_index)
        {
            self.soft_breaks.next().unwrap();
            return Some(WrappedInline::SoftBreak);
        }
        Some(WrappedInline::Inline(match self.inline.take()? {
            Inline::Text { is_inlay, text } => {
                let mut byte_count = text.len();
                if let Some(&byte_index) = self.soft_breaks.as_slice().first() {
                    byte_count = byte_count.min(byte_index - self.byte_index);
                }
                let text = if byte_count < text.len() {
                    let (text, remaining_text) = text.split_at(byte_count);
                    self.inline = Some(Inline::Text {
                        is_inlay,
                        text: remaining_text,
                    });
                    text
                } else {
                    self.inline = self.inlines.next();
                    text
                };
                self.byte_index += text.len();
                Inline::Text { is_inlay, text }
            }
            inline @ Inline::Widget(_) => {
                self.inline = self.inlines.next();
                inline
            }
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WrappedInline<'a> {
    Inline(Inline<'a>),
    SoftBreak,
}

#[derive(Clone, Debug)]
pub struct Blocks<'a> {
    lines: Lines<'a>,
    block_inlays: slice::Iter<'a, (usize, BlockInlay)>,
    line_index: usize,
}

impl<'a> Iterator for Blocks<'a> {
    type Item = Block<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self
            .block_inlays
            .as_slice()
            .first()
            .map_or(false, |&(line, _)| line == self.line_index)
        {
            let (_, block_inlays) = self.block_inlays.next().unwrap();
            return Some(match block_inlays {
                BlockInlay::Line(line) => Block::Line {
                    is_inlay: true,
                    line: line.as_line(),
                },
                BlockInlay::Widget(widget) => Block::Widget(widget),
            });
        }
        let line = self.lines.next()?;
        self.line_index += 1;
        Some(Block::Line {
            is_inlay: false,
            line,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Block<'a> {
    Line { is_inlay: bool, line: Line<'a> },
    Widget(&'a BlockWidget),
}

impl<'a> Block<'a> {
    pub fn width(&self, tab_column_count: usize) -> f64 {
        match self {
            Self::Line { line, .. } => line.width(tab_column_count),
            Self::Widget(widget) => widget.size.width,
        }
    }

    pub fn height(&self) -> f64 {
        match self {
            Self::Line { line, .. } => line.height(),
            Self::Widget(widget) => widget.size.height,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutEvent<'a> {
    pub rect: Rect,
    pub kind: LayoutEventKind<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum LayoutEventKind<'a> {
    Line { is_inlay: bool, line: Line<'a> },
    Grapheme { is_inlay: bool, text: &'a str },
    Break { is_soft: bool },
    Widget { id: usize },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(usize);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum InlineInlay {
    Text(String),
    Widget(InlineWidget),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InlineWidget {
    pub id: usize,
    pub column_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockInlay {
    Line(LineInlay),
    Widget(BlockWidget),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LineInlay {
    text: String,
    inline_inlays: Vec<(usize, InlineInlay)>,
    soft_breaks: Vec<usize>,
    fold_column_index: usize,
    scale: f64,
}

impl LineInlay {
    pub fn new(text: String) -> Self {
        Self {
            text,
            inline_inlays: Vec::new(),
            soft_breaks: Vec::new(),
            fold_column_index: 0,
            scale: 1.0,
        }
    }

    pub fn as_line(&self) -> Line<'_> {
        Line {
            text: &self.text,
            inline_inlays: &self.inline_inlays,
            soft_breaks: &self.soft_breaks,
            fold_column_index: self.fold_column_index,
            scale: self.scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockWidget {
    pub id: usize,
    pub size: Size,
}

#[derive(Clone, Debug)]
struct Session {
    document_id: DocumentId,
    soft_breaks: Vec<Vec<usize>>,
    fold_column_index: Vec<usize>,
    scale: Vec<f64>,
    summed_heights: Vec<f64>,
    selections: Vec<Selection>,
    last_added_selection_index: usize,
    folding_lines: HashSet<usize>,
    unfolding_lines: HashSet<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DocumentId(usize);

#[derive(Clone, Debug)]
struct Document {
    text: Text,
    inline_inlays: Vec<Vec<(usize, InlineInlay)>>,
    block_inlays: Vec<(usize, BlockInlay)>,
}
