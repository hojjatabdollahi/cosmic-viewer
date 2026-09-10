// SPDX-License-Identifier: GPL-3.0-only

use crate::ToolOperation;
use cosmic::iced::Point;

/// Committed operations with undo/redo, plus the active tool's live preview.
#[derive(Debug, Default)]
pub struct OperationStack {
    committed: Vec<Box<dyn ToolOperation>>,
    undone: Vec<Box<dyn ToolOperation>>,
    preview: Option<Box<dyn ToolOperation>>,
}

impl Clone for OperationStack {
    fn clone(&self) -> Self {
        Self {
            committed: self.committed.iter().map(|op| op.clone_boxed()).collect(),
            undone: self.undone.iter().map(|op| op.clone_boxed()).collect(),
            preview: self.preview.as_ref().map(|op| op.clone_boxed()),
        }
    }
}

impl OperationStack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Committed operations, in draw order.
    #[must_use]
    pub fn operations(&self) -> &[Box<dyn ToolOperation>] {
        &self.committed
    }

    pub fn operations_mut(&mut self) -> &mut Vec<Box<dyn ToolOperation>> {
        &mut self.committed
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.committed.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.committed.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&(dyn ToolOperation + 'static)> {
        self.committed.get(index).map(AsRef::as_ref)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut (dyn ToolOperation + 'static)> {
        self.committed.get_mut(index).map(AsMut::as_mut)
    }

    /// Commit an operation. Discards anything undone.
    pub fn commit(&mut self, op: Box<dyn ToolOperation>) {
        self.committed.push(op);
        self.undone.clear();
    }

    pub fn undo(&mut self) -> Option<&(dyn ToolOperation + 'static)> {
        let op = self.committed.pop()?;
        self.undone.push(op);
        self.undone.last().map(AsRef::as_ref)
    }

    pub fn redo(&mut self) -> Option<&(dyn ToolOperation + 'static)> {
        let op = self.undone.pop()?;
        self.committed.push(op);
        self.committed.last().map(AsRef::as_ref)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.committed.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Remove one committed operation. Discards anything undone, since it was
    /// undone from a list this operation was part of.
    pub fn remove(&mut self, index: usize) -> Option<Box<dyn ToolOperation>> {
        if index >= self.committed.len() {
            return None;
        }
        self.undone.clear();
        Some(self.committed.remove(index))
    }

    /// Keep only the committed operations matching `keep`. Discards anything undone.
    pub fn retain(&mut self, keep: impl FnMut(&dyn ToolOperation) -> bool) {
        let mut keep = keep;
        self.committed.retain(|op| keep(op.as_ref()));
        self.undone.clear();
    }

    /// Drop everything, history and preview included.
    pub fn clear(&mut self) {
        self.committed.clear();
        self.undone.clear();
        self.preview = None;
    }

    pub fn set_preview(&mut self, preview: Option<Box<dyn ToolOperation>>) {
        self.preview = preview;
    }

    #[must_use]
    pub fn preview(&self) -> Option<&(dyn ToolOperation + 'static)> {
        self.preview.as_deref()
    }

    pub fn preview_mut(&mut self) -> Option<&mut (dyn ToolOperation + 'static)> {
        self.preview.as_deref_mut()
    }

    pub fn take_preview(&mut self) -> Option<Box<dyn ToolOperation>> {
        self.preview.take()
    }

    /// Commit the preview through its own `commit`. Returns whether it produced
    /// an operation. The preview is cleared either way.
    pub fn commit_preview(&mut self) -> bool {
        let Some(op) = self.preview.as_ref().and_then(|preview| preview.commit()) else {
            return false;
        };
        self.preview = None;
        self.commit(op);
        true
    }

    /// Drop the committed and undone operations, leaving the preview.
    pub fn clear_history(&mut self) {
        self.committed.clear();
        self.undone.clear();
    }

    /// The topmost movable operation under `point`, by each operation's own
    /// hit test, so a stroke is picked along its line rather than anywhere in
    /// its box.
    #[must_use]
    pub fn hit(&self, point: Point) -> Option<usize> {
        self.committed
            .iter()
            .rposition(|op| op.movable() && op.hit_test(point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::{ShapeKind, ShapeOperation};
    use cosmic::iced::Color;

    fn shape(x: f32) -> Box<dyn ToolOperation> {
        Box::new(ShapeOperation::new(
            ShapeKind::Rectangle,
            Point::new(x, 0.0),
            Point::new(x + 10.0, 10.0),
            Color::BLACK,
            2.0,
        ))
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut stack = OperationStack::new();
        stack.commit(shape(0.0));
        stack.commit(shape(20.0));
        assert!(stack.undo().is_some());
        assert_eq!(stack.len(), 1);
        assert!(stack.can_redo());
        assert!(stack.redo().is_some());
        assert_eq!(stack.len(), 2);
        assert!(!stack.can_redo());
    }

    #[test]
    fn commit_discards_undone() {
        let mut stack = OperationStack::new();
        stack.commit(shape(0.0));
        stack.undo();
        stack.commit(shape(20.0));
        assert!(!stack.can_redo());
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn hit_prefers_topmost() {
        let mut stack = OperationStack::new();
        stack.commit(shape(0.0));
        stack.commit(shape(5.0));
        assert_eq!(stack.hit(Point::new(7.0, 5.0)), Some(1));
        // Shapes pick with a 4 px pad, so stay clear of the upper one.
        assert_eq!(stack.hit(Point::new(-2.0, 5.0)), Some(0));
        assert_eq!(stack.hit(Point::new(40.0, 5.0)), None);
    }

    #[test]
    fn clone_is_independent() {
        let mut stack = OperationStack::new();
        stack.commit(shape(0.0));
        let copy = stack.clone();
        stack.clear();
        assert_eq!(copy.len(), 1);
    }
}
