// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    SampleSource, ToolOperation,
    renderer::{build_path, stroke_on_image},
};
use cosmic::{
    Renderer,
    iced::advanced::image::{FilterMethod, Handle, Image},
    iced::widget::canvas::{Frame, Path, Stroke},
    iced::{Color, Point, Radians, Rectangle, Size},
};
use image::{DynamicImage, RgbaImage};
use std::any::Any;
use tiny_skia::LineCap;

/// Smallest loupe worth drawing, in operation units.
pub const MIN_RADIUS: f32 = 12.0;

/// A circular loupe showing the pixels beneath it enlarged.
#[derive(Debug, Clone)]
pub struct MagnifierOperation {
    pub start: Point,
    pub end: Point,
    pub magnification: f32,
    pub color: Color,
    pub ring_width: f32,
}

impl MagnifierOperation {
    #[must_use]
    pub const fn new(start: Point, end: Point, magnification: f32, color: Color) -> Self {
        Self {
            start,
            end,
            magnification,
            color,
            ring_width: 3.0,
        }
    }

    #[must_use]
    pub const fn center(&self) -> Point {
        Point::new(
            f32::midpoint(self.start.x, self.end.x),
            f32::midpoint(self.start.y, self.end.y),
        )
    }

    /// Half the average extent of the dragged box, so the loupe is always a
    /// circle.
    #[must_use]
    pub fn radius(&self) -> f32 {
        (((self.end.x - self.start.x).abs() + (self.end.y - self.start.y).abs()) * 0.25).max(1.0)
    }

    pub fn set_geometry(&mut self, center: Point, radius: f32) {
        let r = radius.max(MIN_RADIUS);
        self.start = Point::new(center.x - r, center.y - r);
        self.end = Point::new(center.x + r, center.y + r);
    }

    /// The enlarged pixels, and the operation-space box they fill.
    // reason: the loupe is a few hundred pixels across, so every cast is in range.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn loupe(&self, source: &SampleSource<'_>) -> Option<(RgbaImage, Rectangle)> {
        let center = self.center();
        let radius = self.radius();
        if radius < 1.0 {
            return None;
        }
        let center_px = source.to_pixels(center);
        let radius_px = radius * source.scale;
        let size = (radius_px * 2.0).ceil().max(1.0) as u32;
        let magnification = self.magnification.max(1.0);

        // The source pixels the loupe reaches, with a pixel of slack for the filter.
        let reach = radius_px / magnification + 1.0;
        let (picture, offset) = source.rendered(Rectangle::new(
            Point::new(center_px.x - reach, center_px.y - reach),
            Size::new(reach * 2.0, reach * 2.0),
        ));

        let mut out = RgbaImage::new(size, size);
        for py in 0..size {
            for px in 0..size {
                let dx = px as f32 + 0.5 - radius_px;
                let dy = py as f32 + 0.5 - radius_px;
                if dx.mul_add(dx, dy * dy) > radius_px * radius_px {
                    continue;
                }
                let sample = bilinear(
                    &picture,
                    center_px.x + dx / magnification - offset.x,
                    center_px.y + dy / magnification - offset.y,
                );
                out.put_pixel(px, py, sample);
            }
        }
        let bounds = Rectangle::new(
            Point::new(center.x - radius, center.y - radius),
            Size::new(radius * 2.0, radius * 2.0),
        );
        Some((out, bounds))
    }

    fn draw_ring(&self, frame: &mut Frame<Renderer>, scale: f32) {
        let circle = Path::circle(self.center(), self.radius());
        frame.stroke(
            &circle,
            Stroke::default()
                .with_color(Color::from_rgba(0.0, 0.0, 0.0, 0.85))
                .with_width((self.ring_width + 2.0) * scale),
        );
        frame.stroke(
            &circle,
            Stroke::default()
                .with_color(self.color)
                .with_width(self.ring_width * scale),
        );
    }
}

/// Sample `image` at a fractional position, clamped to its edges.
// reason: coordinates are clamped to the image before the cast, and image
// dimensions are far below f32's exact integer range.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn bilinear(image: &RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return image::Rgba([0, 0, 0, 255]);
    }
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;

    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = f32::from(p00[c]).mul_add(1.0 - fx, f32::from(p10[c]) * fx);
        let bottom = f32::from(p01[c]).mul_add(1.0 - fx, f32::from(p11[c]) * fx);
        out[c] = top.mul_add(1.0 - fy, bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    image::Rgba(out)
}

impl ToolOperation for MagnifierOperation {
    fn clone_boxed(&self) -> Box<dyn ToolOperation> {
        Box::new(self.clone())
    }

    fn draw(&self, frame: &mut Frame<Renderer>, _image_size: Size, scale: f32) {
        self.draw_ring(frame, scale);
    }

    fn draw_sampled(
        &self,
        frame: &mut Frame<Renderer>,
        _image_size: Size,
        scale: f32,
        source: Option<&SampleSource<'_>>,
    ) {
        if let Some((pixels, bounds)) = source.and_then(|source| self.loupe(source)) {
            let (w, h) = pixels.dimensions();
            frame.draw_image(
                bounds,
                Image {
                    handle: Handle::from_rgba(w, h, pixels.into_raw()),
                    filter_method: FilterMethod::Linear,
                    rotation: Radians(0.0),
                    border_radius: self.radius().into(),
                    opacity: 1.0,
                    snap: false,
                },
            );
        }
        self.draw_ring(frame, scale);
    }

    // reason: the loupe box is clamped to the image before the cast, and image
    // dimensions are far below f32's exact integer range.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn apply(&self, image: &mut DynamicImage) {
        let loupe = image
            .as_rgba8()
            .and_then(|rgba| self.loupe(&SampleSource::identity(rgba)));
        if let Some((pixels, bounds)) = loupe
            && let Some(rgba) = image.as_mut_rgba8()
        {
            let (w, h) = rgba.dimensions();
            let radius = bounds.width / 2.0;
            let x0 = bounds.x.floor();
            let y0 = bounds.y.floor();
            for (px, py, pixel) in pixels.enumerate_pixels() {
                let dx = px as f32 + 0.5 - radius;
                let dy = py as f32 + 0.5 - radius;
                if dx.mul_add(dx, dy * dy) > radius * radius {
                    continue;
                }
                let x = x0 + px as f32;
                let y = y0 + py as f32;
                if x >= 0.0 && y >= 0.0 && (x as u32) < w && (y as u32) < h {
                    rgba.put_pixel(x as u32, y as u32, *pixel);
                }
            }
        }

        let center = self.center();
        let Some(circle) = build_path(|b| b.push_circle(center.x, center.y, self.radius())) else {
            return;
        };
        stroke_on_image(
            image,
            &circle,
            Color::from_rgba(0.0, 0.0, 0.0, 0.85),
            self.ring_width + 2.0,
            LineCap::Round,
        );
        stroke_on_image(image, &circle, self.color, self.ring_width, LineCap::Round);
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
        let center = self.center();
        let radius = self.radius();
        Some(Rectangle::new(
            Point::new(center.x - radius, center.y - radius),
            Size::new(radius * 2.0, radius * 2.0),
        ))
    }

    fn hit_test(&self, point: Point) -> bool {
        point.distance(self.center()) <= self.radius()
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
        self.ring_width *= factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red channel encodes the x coordinate.
    #[allow(clippy::cast_possible_truncation)] // reason: test images are under 256 wide
    fn coord_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, _| {
            image::Rgba([x as u8, 0, 0, 255])
        }))
    }

    #[test]
    fn zooms_toward_center() {
        let mut image = coord_image(21, 21);
        // Center (10, 10), radius 10, 2x.
        MagnifierOperation::new(
            Point::ORIGIN,
            Point::new(20.0, 20.0),
            2.0,
            Color::TRANSPARENT,
        )
        .apply(&mut image);
        let rgba = image.as_rgba8().unwrap();
        // 4.5 right of center samples the source 2.25 right of center: x = 12.
        assert_eq!(rgba.get_pixel(14, 10)[0], 12);
        assert_eq!(rgba.get_pixel(10, 10)[0], 10);
    }

    #[test]
    fn leaves_outside_untouched() {
        let mut image = coord_image(21, 21);
        MagnifierOperation::new(
            Point::ORIGIN,
            Point::new(20.0, 20.0),
            2.0,
            Color::TRANSPARENT,
        )
        .apply(&mut image);
        assert_eq!(image.as_rgba8().unwrap().get_pixel(0, 0)[0], 0);
    }
}
