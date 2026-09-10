// SPDX-License-Identifier: GPL-3.0-only

use std::any::Any;

use crate::{
    ToolOperation,
    renderer::{highlight_shape, smoothed_path, stroke_on_image},
    rotate::RotateDirection,
};
use cosmic::{
    Renderer,
    iced::widget::canvas::Frame,
    iced::{Color, Point, Rectangle, Size},
};
use image::DynamicImage;
use tiny_skia::LineCap as SkiaLineCap;

use super::HIGHLIGHT_ALPHA;

#[derive(Debug, Clone)]
pub struct HighlighterOperation {
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
}

impl HighlighterOperation {
    fn highlight_color(&self) -> Color {
        Color::from_rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            self.color.a * HIGHLIGHT_ALPHA,
        )
    }
}

impl ToolOperation for HighlighterOperation {
    fn clone_boxed(&self) -> Box<dyn ToolOperation> {
        Box::new(self.clone())
    }

    fn transform_scale(&mut self, factor: f32) {
        for point in &mut self.points {
            point.x *= factor;
            point.y *= factor;
        }
        self.width *= factor;
    }

    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        // Filled, not stroked: see `renderer::stroke_outline`.
        if let Some(shape) = highlight_shape(&self.points, self.width * scale) {
            frame.fill(&shape, self.highlight_color());
        }
    }

    fn apply(&self, image: &mut DynamicImage) {
        // The same curve the preview draws, so what is saved is what was seen.
        let Some(path) = smoothed_path(&self.points) else {
            return;
        };

        stroke_on_image(
            image,
            &path,
            self.highlight_color(),
            self.width,
            SkiaLineCap::Square,
        );
    }

    fn commit(&self) -> Option<Box<dyn ToolOperation>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn transform_rotate(&mut self, direction: RotateDirection, image_size: Size) {
        let (width, height) = (image_size.width, image_size.height);
        for point in &mut self.points {
            let (x, y) = (point.x, point.y);

            *point = match direction {
                RotateDirection::Left => Point::new(y, width - x),
                RotateDirection::Right => Point::new(height - y, x),
            }
        }
    }

    fn transform_crop(&mut self, region: Rectangle) {
        for point in &mut self.points {
            point.x -= region.x;
            point.y -= region.y;
        }
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        for point in &mut self.points {
            point.x += dx;
            point.y += dy;
        }
    }

    fn movable(&self) -> bool {
        true
    }

    fn bounds(&self) -> Option<Rectangle> {
        crate::stroke_bounds(&self.points, self.width)
    }

    fn hit_test(&self, point: Point) -> bool {
        crate::stroke_hit(&self.points, self.width, point)
    }
}
