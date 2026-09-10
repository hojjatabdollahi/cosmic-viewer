// SPDX-License-Identifier: GPL-3.0-only

use std::any::Any;

use super::{HIGHLIGHT_ALPHA, HighlighterOperation};
use crate::ToolOperation;
use crate::renderer::highlight_shape;
use cosmic::{
    Renderer,
    iced::widget::canvas::Frame,
    iced::{Color, Point, Size, mouse},
};
use image::DynamicImage;

#[derive(Debug, Clone)]
pub struct HighlighterPreview {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl HighlighterPreview {
    #[must_use]
    pub const fn new(color: Color, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }

    fn highlight_color(&self) -> Color {
        Color::from_rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            self.color.a * HIGHLIGHT_ALPHA,
        )
    }
}

impl ToolOperation for HighlighterPreview {
    fn clone_boxed(&self) -> Box<dyn ToolOperation> {
        Box::new(self.clone())
    }

    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        // Filled, not stroked: see `renderer::stroke_outline`.
        if let Some(shape) = highlight_shape(&self.points, self.width * scale) {
            frame.fill(&shape, self.highlight_color());
        }
    }

    fn apply(&self, _image: &mut DynamicImage) {
        // Never modifies pixels
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        if self.points.len() >= 2 {
            Some(Box::new(HighlighterOperation {
                points: self.points.clone(),
                color: self.color,
                width: self.width,
            }))
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn on_press(&mut self, point: Point, _image_size: Size) -> mouse::Interaction {
        self.points.clear();
        self.points.push(point);
        mouse::Interaction::Crosshair
    }

    fn on_drag(&mut self, point: Point, _image_size: Size) {
        if let Some(last) = self.points.last() {
            let dx = point.x - last.x;
            let dy = point.y - last.y;
            let min_dist = self.width * 0.5;

            if dy.mul_add(dy, dx * dx) < min_dist * min_dist {
                // Skip to reduce join overlap
                return;
            }
        }
        self.points.push(point);
    }

    fn on_release(&mut self, _point: Point, _image_size: Size) {
        // Points already captured during drag; nothing to finalize.
    }
}
