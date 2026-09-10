// SPDX-License-Identifier: GPL-3.0-only

mod operation;
mod preview;

pub use operation::HighlighterOperation;
pub use preview::HighlighterPreview;

/// Highlighter transparency factor
pub const HIGHLIGHT_ALPHA: f32 = 0.35;
