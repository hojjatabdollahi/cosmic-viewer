// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    ToolOperation,
    annotate::tool::shapes::normalize_rect,
    renderer::{build_path, fill_on_image},
};
use cosmic::{
    Renderer,
    iced::widget::canvas::{Fill, Frame},
    iced::{Color, Point, Rectangle, Size},
};
use image::DynamicImage;
use std::any::Any;

/// A solid black box over something that must not be seen.
#[derive(Debug, Clone)]
pub struct RedactOperation {
    pub start: Point,
    pub end: Point,
}

impl RedactOperation {
    #[must_use]
    pub const fn new(start: Point, end: Point) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn rect(&self) -> Rectangle {
        normalize_rect(self.start, self.end)
    }
}

impl ToolOperation for RedactOperation {
    fn clone_boxed(&self) -> Box<dyn ToolOperation> {
        Box::new(self.clone())
    }

    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, _scale: f32) {
        let r = self.rect();
        frame.fill_rectangle(r.position(), r.size(), Fill::from(Color::BLACK));
    }

    fn apply(&self, image: &mut DynamicImage) {
        let r = self.rect();
        let Some(rect) = tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height) else {
            return;
        };
        if let Some(path) = build_path(|b| b.push_rect(rect)) {
            fill_on_image(image, &path, Color::BLACK);
        }
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

    fn bounds(&self) -> Option<Rectangle> {
        Some(self.rect())
    }

    fn movable(&self) -> bool {
        true
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        for point in [&mut self.start, &mut self.end] {
            point.x += dx;
            point.y += dy;
        }
    }

    fn transform_crop(&mut self, region: Rectangle) {
        self.translate(-region.x, -region.y);
    }

    fn transform_scale(&mut self, factor: f32) {
        for point in [&mut self.start, &mut self.end] {
            point.x *= factor;
            point.y *= factor;
        }
    }
}
