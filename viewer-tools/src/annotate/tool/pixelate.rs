// SPDX-License-Identifier: GPL-3.0-only

use crate::{SampleSource, ToolOperation, annotate::tool::shapes::normalize_rect};
use cosmic::{
    Renderer,
    iced::widget::canvas::{Fill, Frame, Path, Stroke},
    iced::{Color, Point, Rectangle, Size},
};
use image::{DynamicImage, RgbaImage};
use std::any::Any;

/// Replaces a region with the average color of each block inside it.
#[derive(Debug, Clone)]
pub struct PixelateOperation {
    pub start: Point,
    pub end: Point,
    /// Block edge, in operation units.
    pub block_size: f32,
}

impl PixelateOperation {
    #[must_use]
    pub const fn new(start: Point, end: Point, block_size: f32) -> Self {
        Self {
            start,
            end,
            block_size,
        }
    }

    #[must_use]
    pub fn rect(&self) -> Rectangle {
        normalize_rect(self.start, self.end)
    }

    /// Visit each block as `(block, average color)`.
    // reason: the loops step by at least one unit per pass, so they terminate.
    #[allow(clippy::while_float)]
    fn blocks(&self, source: &SampleSource<'_>, mut visit: impl FnMut(Rectangle, Color)) {
        let area = self.rect();
        let area_px = {
            let p0 = source.to_pixels(area.position());
            let p1 = source.to_pixels(Point::new(area.x + area.width, area.y + area.height));
            Rectangle::new(p0, Size::new(p1.x - p0.x, p1.y - p0.y))
        };
        let (picture, offset) = source.rendered(area_px);
        let local = |p: Point| Point::new(p.x - offset.x, p.y - offset.y);
        let step = self.block_size.max(1.0);
        let mut y = area.y;
        while y < area.y + area.height {
            let height = step.min(area.y + area.height - y);
            let mut x = area.x;
            while x < area.x + area.width {
                let width = step.min(area.x + area.width - x);
                let p0 = local(source.to_pixels(Point::new(x, y)));
                let p1 = local(source.to_pixels(Point::new(x + width, y + height)));
                if let Some([red, green, blue, alpha]) = average_rgba(&picture, p0, p1) {
                    visit(
                        Rectangle::new(Point::new(x, y), Size::new(width, height)),
                        Color::from_rgba8(red, green, blue, f32::from(alpha) / 255.0),
                    );
                }
                x += width;
            }
            y += height;
        }
    }
}

/// Average of the pixels in `[p0, p1)`, clamped to the image.
// reason: pixel coordinates are non-negative and clamped before the cast,
// and image dimensions are far below f32's exact integer range.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn average_rgba(image: &RgbaImage, p0: Point, p1: Point) -> Option<[u8; 4]> {
    let (w, h) = image.dimensions();
    let x0 = p0.x.round().clamp(0.0, w as f32) as u32;
    let y0 = p0.y.round().clamp(0.0, h as f32) as u32;
    let x1 = p1.x.round().clamp(0.0, w as f32) as u32;
    let y1 = p1.y.round().clamp(0.0, h as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut sum = [0u64; 4];
    for y in y0..y1 {
        for x in x0..x1 {
            for (acc, channel) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                *acc += u64::from(channel);
            }
        }
    }
    let count = u64::from(x1 - x0) * u64::from(y1 - y0);
    Some(sum.map(|total| (total / count) as u8))
}

impl ToolOperation for PixelateOperation {
    fn clone_boxed(&self) -> Box<dyn ToolOperation> {
        Box::new(self.clone())
    }

    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        // Nothing to sample from: mark the area instead.
        let r = self.rect();
        frame.fill_rectangle(
            r.position(),
            r.size(),
            Fill::from(Color::from_rgba(0.5, 0.5, 0.5, 0.6)),
        );
        frame.stroke(
            &Path::rectangle(r.position(), r.size()),
            Stroke::default().with_color(Color::WHITE).with_width(scale),
        );
    }

    fn draw_sampled(
        &self,
        frame: &mut Frame<Renderer>,
        image_size: Size,
        scale: f32,
        source: Option<&SampleSource<'_>>,
    ) {
        let Some(source) = source else {
            return self.draw(frame, image_size, scale);
        };
        self.blocks(source, |block, color| {
            frame.fill_rectangle(block.position(), block.size(), Fill::from(color));
        });
    }

    // reason: block bounds are clamped to the image before the cast, and image
    // dimensions are far below f32's exact integer range.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn apply(&self, image: &mut DynamicImage) {
        let Some(rgba) = image.as_mut_rgba8() else {
            return;
        };
        let snapshot = rgba.clone();
        let source = SampleSource::identity(&snapshot);
        let (w, h) = rgba.dimensions();
        self.blocks(&source, |block, color| {
            let x0 = block.x.round().clamp(0.0, w as f32) as u32;
            let y0 = block.y.round().clamp(0.0, h as f32) as u32;
            let x1 = (block.x + block.width).round().clamp(0.0, w as f32) as u32;
            let y1 = (block.y + block.height).round().clamp(0.0, h as f32) as u32;
            let pixel = image::Rgba(color.into_rgba8());
            for y in y0..y1 {
                for x in x0..x1 {
                    rgba.put_pixel(x, y, pixel);
                }
            }
        });
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
        self.block_size *= factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_take_the_average() {
        // Left half red, right half blue, one block per half.
        let img = RgbaImage::from_fn(8, 4, |x, _| {
            if x < 4 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let mut image = DynamicImage::ImageRgba8(img);
        PixelateOperation::new(Point::ORIGIN, Point::new(8.0, 4.0), 4.0).apply(&mut image);
        let rgba = image.as_rgba8().unwrap();
        assert_eq!(rgba.get_pixel(1, 1).0, [255, 0, 0, 255]);
        assert_eq!(rgba.get_pixel(6, 2).0, [0, 0, 255, 255]);
    }

    #[test]
    fn a_block_spanning_both_halves_mixes_them() {
        let img = RgbaImage::from_fn(8, 4, |x, _| {
            if x < 4 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let mut image = DynamicImage::ImageRgba8(img);
        PixelateOperation::new(Point::ORIGIN, Point::new(8.0, 4.0), 8.0).apply(&mut image);
        assert_eq!(
            image.as_rgba8().unwrap().get_pixel(0, 0).0,
            [127, 0, 127, 255]
        );
    }
}
