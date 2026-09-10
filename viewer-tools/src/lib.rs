// SPDX-License-Identifier: GPL-3.0-only

pub mod annotate;
pub mod crop;
pub mod renderer;
pub mod rotate;
pub mod stack;

pub use stack::OperationStack;

// Re-exports
pub use crate::annotate::FONT_SIZE_PRESETS_PT;

use crate::rotate::RotateDirection;
use cosmic::{
    Renderer,
    iced::widget::canvas::Frame,
    iced::{Point, Rectangle, Size, mouse},
};
use image::{DynamicImage, RgbaImage};
use std::{any::Any, fmt::Debug};

/// The pixels beneath an operation: an image covering operation space from
/// `origin`, at `scale` image pixels per operation unit.
#[derive(Clone, Copy, Debug)]
pub struct SampleSource<'a> {
    pub image: &'a RgbaImage,
    pub origin: Point,
    pub scale: f32,
    /// The operations under the one sampling. They are rendered into what it
    /// samples, so a live loupe shows the annotated picture the export will.
    pub below: &'a [Box<dyn ToolOperation>],
}

impl SampleSource<'_> {
    /// An image whose pixel grid is operation space.
    #[must_use]
    pub const fn identity(image: &RgbaImage) -> SampleSource<'_> {
        SampleSource {
            image,
            origin: Point::ORIGIN,
            scale: 1.0,
            below: &[],
        }
    }

    /// The pixels of `area`, in image pixel coordinates, with [`Self::below`]
    /// rendered onto them, and the offset of the returned image's top-left
    /// corner in image pixels. Without operations below, the image itself.
    // reason: the area is clamped to the image before the casts.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[must_use]
    pub fn rendered(&self, area: Rectangle) -> (std::borrow::Cow<'_, RgbaImage>, Point) {
        use std::borrow::Cow;
        if self.below.is_empty() {
            return (Cow::Borrowed(self.image), Point::ORIGIN);
        }
        let (w, h) = self.image.dimensions();
        let x0 = area.x.floor().clamp(0.0, w as f32) as u32;
        let y0 = area.y.floor().clamp(0.0, h as f32) as u32;
        let x1 = (area.x + area.width).ceil().clamp(0.0, w as f32) as u32;
        let y1 = (area.y + area.height).ceil().clamp(0.0, h as f32) as u32;
        if x1 <= x0 || y1 <= y0 {
            return (Cow::Borrowed(self.image), Point::ORIGIN);
        }
        let mut crop = DynamicImage::ImageRgba8(
            image::imageops::crop_imm(self.image, x0, y0, x1 - x0, y1 - y0).to_image(),
        );
        // The operation-space rectangle this crop covers.
        let region = Rectangle::new(
            Point::new(
                self.origin.x + x0 as f32 / self.scale,
                self.origin.y + y0 as f32 / self.scale,
            ),
            Size::new((x1 - x0) as f32 / self.scale, (y1 - y0) as f32 / self.scale),
        );
        apply_all(self.below, &mut crop, region, self.scale);
        (
            Cow::Owned(crop.into_rgba8()),
            Point::new(x0 as f32, y0 as f32),
        )
    }

    /// Operation-space point to image pixel coordinates.
    #[must_use]
    pub fn to_pixels(&self, point: Point) -> Point {
        Point::new(
            (point.x - self.origin.x) * self.scale,
            (point.y - self.origin.y) * self.scale,
        )
    }
}

/// How far from a stroke's center line a press still picks it, beyond its
/// own half-width.
pub const STROKE_PICK_SLACK: f32 = 6.0;

/// The box around a freehand stroke of `width`, or `None` for no points.
#[must_use]
pub fn stroke_bounds(points: &[Point], width: f32) -> Option<Rectangle> {
    let first = points.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let half = width / 2.0;
    Some(Rectangle::new(
        Point::new(x0 - half, y0 - half),
        Size::new(x1 - x0 + width, y1 - y0 + width),
    ))
}

/// Whether `point` lies on a freehand stroke of `width`, with some slack.
#[must_use]
pub fn stroke_hit(points: &[Point], width: f32, point: Point) -> bool {
    let reach = width / 2.0 + STROKE_PICK_SLACK;
    points.iter().any(|p| point.distance(*p) <= reach)
}

/// Rasterize `ops` onto `image`, which covers `region` of operation space at
/// `scale` image pixels per operation unit.
///
/// # Panics
///
/// Panics if `image` is not RGBA or cannot back a pixmap. See the operations'
/// `apply`.
pub fn apply_all(
    ops: &[Box<dyn ToolOperation>],
    image: &mut DynamicImage,
    region: Rectangle,
    scale: f32,
) {
    let region = Rectangle::new(
        Point::new(region.x * scale, region.y * scale),
        Size::new(region.width * scale, region.height * scale),
    );
    for op in ops {
        let mut op = op.clone_boxed();
        op.transform_scale(scale);
        op.transform_crop(region);
        op.apply(image);
    }
}

/// A tool operation that can be draw as an overlay and applied to an image.
///
/// Committed operations live in the undo/redo stack.
/// Active tool previews (like `CropSelection` during drag) implement this
/// trait for rendering but are never committed to the stack; they are "transparent"
/// operations.
pub trait ToolOperation: Debug + Send {
    /// Draw the operation's overlay onto the frame.
    /// The frame is already translated/scaled to image coordinates.
    fn draw(&self, frame: &mut Frame<Renderer>, image_size: Size, scale: f32);

    /// Draw with the pixels beneath the operation available. Only tools that
    /// sample them override this. Everything else falls through to `draw`.
    fn draw_sampled(
        &self,
        frame: &mut Frame<Renderer>,
        image_size: Size,
        scale: f32,
        source: Option<&SampleSource<'_>>,
    ) {
        let _ = source;
        self.draw(frame, image_size, scale);
    }

    /// A boxed copy, for rasterizing a transformed version without touching
    /// the original.
    fn clone_boxed(&self) -> Box<dyn ToolOperation>;

    /// Apply the operation destructively to the image pixels.
    /// Called at save time when flattening all committed operations.
    fn apply(&self, image: &mut DynamicImage);

    /// Produce the committed operation from this preview, if applicable.
    /// Returns None if this operation is already committed or has no result.
    fn commit(&self) -> Option<Box<dyn ToolOperation>>;

    /// Downcast support for tool-specific config.
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast support for tool-specific config.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Called on left mouse press.
    fn on_press(&mut self, point: Point, image_size: Size) -> mouse::Interaction {
        let _ = (point, image_size);
        mouse::Interaction::default()
    }

    /// Called on mouse drag while pressed.
    fn on_drag(&mut self, point: Point, image_size: Size) {
        let _ = (point, image_size);
    }

    /// Called on mouse release.
    fn on_release(&mut self, point: Point, image_size: Size) {
        let _ = (point, image_size);
    }

    /// Returns the cursor to show when hovering at this point.
    fn cursor_at(&self, point: Point) -> mouse::Interaction {
        let _ = point;
        mouse::Interaction::default()
    }

    /// Called when the viewport zoom level changes.
    /// Tools can adjust their coordinates to maintain visual stability.
    fn on_zoom_changed(&mut self, old_zoom: f32, new_zoom: f32, image_size: Size) {
        let _ = (old_zoom, new_zoom, image_size);
    }

    /// Transform this operation's coordinates for a rotation.
    fn transform_rotate(&mut self, _direction: RotateDirection, _image_size: Size) {}

    /// Transform this operation's coordinates for a crop.
    fn transform_crop(&mut self, _region: Rectangle) {}

    /// Scale every coordinate and size by `factor`.
    fn transform_scale(&mut self, _factor: f32) {}

    fn bounds(&self) -> Option<Rectangle> {
        None
    }

    fn movable(&self) -> bool {
        false
    }

    fn hit_test(&self, point: Point) -> bool {
        self.bounds().is_some_and(|b| b.contains(point))
    }

    fn translate(&mut self, _dx: f32, _dy: f32) {}
}
