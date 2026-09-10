// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::widget::canvas;
use cosmic::iced::{Color, Point};
use image::{DynamicImage, RgbaImage};
use tiny_skia::{
    LineCap, LineJoin, Paint, Path, PathBuilder, PathSegment, Pixmap, Stroke, Transform,
};

pub fn build_path(f: impl FnOnce(&mut PathBuilder)) -> Option<Path> {
    let mut path_builder = PathBuilder::new();
    f(&mut path_builder);
    path_builder.finish()
}

/// A hand-drawn stroke, smoothed into curves.
///
/// Pointer motion arrives as a chain of short straight hops. Curving through
/// the midpoint of each hop, with the sampled point as the control, turns that
/// chain into something that reads as drawn rather than plotted. Returns
/// `None` for fewer than two points, which is a click and marks nothing.
#[must_use]
pub fn smoothed_path(points: &[Point]) -> Option<Path> {
    if points.len() < 2 {
        return None;
    }

    build_path(|builder| {
        builder.move_to(points[0].x, points[0].y);

        if points.len() == 2 {
            builder.line_to(points[1].x, points[1].y);
            return;
        }

        builder.line_to(
            f32::midpoint(points[0].x, points[1].x),
            f32::midpoint(points[0].y, points[1].y),
        );

        for idx in 1..points.len() - 1 {
            let control = points[idx];
            let next = points[idx + 1];
            builder.quad_to(
                control.x,
                control.y,
                f32::midpoint(control.x, next.x),
                f32::midpoint(control.y, next.y),
            );
        }

        let last = points[points.len() - 1];
        builder.line_to(last.x, last.y);
    })
}

/// The region a stroke covers, as a shape to fill rather than a line to draw.
///
/// Drawing a translucent line darkens it wherever it passes over itself, and a
/// hand-drawn curve passes over itself constantly - the tessellator overlaps
/// its own geometry at every join, so a smooth sweep comes out pebbled with
/// wedges before the stroke ever doubles back. One filled outline covers each
/// pixel once, which is what a highlighter does.
#[must_use]
pub fn stroke_outline(path: &Path, width: f32, line_cap: LineCap) -> Option<Path> {
    path.stroke(
        &Stroke {
            width,
            line_cap,
            line_join: LineJoin::Round,
            ..Stroke::default()
        },
        1.0,
    )
}

/// Hand a tiny-skia path to the canvas.
///
/// The two describe paths the same way, so this is a transcription. It exists
/// because the outline of a stroke is only computed on the rasterizing side.
#[must_use]
pub fn to_canvas_path(path: &Path) -> canvas::Path {
    let at = |p: tiny_skia::Point| Point::new(p.x, p.y);

    canvas::Path::new(|builder| {
        for segment in path.segments() {
            match segment {
                PathSegment::MoveTo(p) => builder.move_to(at(p)),
                PathSegment::LineTo(p) => builder.line_to(at(p)),
                PathSegment::QuadTo(control, to) => {
                    builder.quadratic_curve_to(at(control), at(to));
                }
                PathSegment::CubicTo(first, second, to) => {
                    builder.bezier_curve_to(at(first), at(second), at(to));
                }
                PathSegment::Close => builder.close(),
            }
        }
    })
}

/// The shape a highlighter stroke of this width lays down, ready to fill.
#[must_use]
pub fn highlight_shape(points: &[Point], width: f32) -> Option<canvas::Path> {
    let path = smoothed_path(points)?;
    let outline = stroke_outline(&path, width, LineCap::Square)?;
    Some(to_canvas_path(&outline))
}

// Quantize a 0.0..=1.0 color component to an 8-bit channel. Rounds to nearest
// and clamps so out-of-gamut inputs cannot wrap. `as u8` on a float already
// saturates, but truncates instead of rounding, hence the explicit round.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: value clamped to 0.0..=255.0 then rounded, in-range for u8
fn channel_u8(component: f32) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

// Quantize an already-scaled 0.0..=255.0 intensity to an 8-bit channel.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: value clamped to 0.0..=255.0 then rounded, in-range for u8
const fn intensity_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// Stroke a path onto a `DynamicImage` with the given color, width, and line cap.
///
/// # Panics
///
/// Panics if `image` is not RGBA, or if its dimensions cannot back a pixmap
/// (zero-sized or larger than tiny-skia supports).
pub fn stroke_on_image(
    image: &mut DynamicImage,
    path: &Path,
    color: Color,
    width: f32,
    line_cap: LineCap,
) {
    let rgba = image.as_mut_rgba8().expect("image should be RGBA");
    let (img_width, img_height) = (rgba.width(), rgba.height());

    // Render stroke onto a transparent overlay to avoid premultiplication issues
    let mut overlay =
        Pixmap::new(img_width, img_height).expect("image dimensions should produce a valid pixmap");
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        channel_u8(color.r),
        channel_u8(color.g),
        channel_u8(color.b),
        channel_u8(color.a),
    );

    let stroke = Stroke {
        width,
        line_cap,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };

    overlay.stroke_path(path, &paint, &stroke, Transform::identity(), None);

    // Composite overlay onto the image buffer
    blend_overlay(rgba, &overlay);
}

/// Fill a path onto a `DynamicImage` with the given color.
///
/// # Panics
///
/// Panics if `image` is not RGBA, or if its dimensions cannot back a pixmap
/// (zero-sized or larger than tiny-skia supports).
pub fn fill_on_image(image: &mut DynamicImage, path: &Path, color: Color) {
    let rgba = image.as_mut_rgba8().expect("image should be RGBA");
    let (img_width, img_height) = (rgba.width(), rgba.height());

    let mut overlay =
        Pixmap::new(img_width, img_height).expect("image dimensions should produce a valid pixmap");
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        channel_u8(color.r),
        channel_u8(color.g),
        channel_u8(color.b),
        channel_u8(color.a),
    );

    overlay.fill_path(
        path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    blend_overlay(rgba, &overlay);
}

fn blend_overlay(dst: &mut RgbaImage, overlay: &Pixmap) {
    let overlay_data = overlay.data();
    let dst_data: &mut [u8] = dst.as_mut();

    for (dst_pixel, src_chunk) in dst_data
        .chunks_exact_mut(4)
        .zip(overlay_data.chunks_exact(4))
    {
        let src_alpha = f32::from(src_chunk[3]) / 255.0;
        if src_alpha == 0.0 {
            continue;
        }

        // Un-premultiply the overlay
        let src_red = f32::from(src_chunk[0]) / src_alpha;
        let src_green = f32::from(src_chunk[1]) / src_alpha;
        let src_blue = f32::from(src_chunk[2]) / src_alpha;

        let dst_alpha = f32::from(dst_pixel[3]) / 255.0;
        let dst_red = f32::from(dst_pixel[0]);
        let dst_green = f32::from(dst_pixel[1]);
        let dst_blue = f32::from(dst_pixel[2]);

        // Source-over compositing
        let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

        if out_alpha > 0.0 {
            dst_pixel[0] = intensity_u8(
                (dst_red * dst_alpha).mul_add(1.0 - src_alpha, src_red * src_alpha) / out_alpha,
            );
            dst_pixel[1] = intensity_u8(
                (dst_green * dst_alpha).mul_add(1.0 - src_alpha, src_green * src_alpha) / out_alpha,
            );
            dst_pixel[2] = intensity_u8(
                (dst_blue * dst_alpha).mul_add(1.0 - src_alpha, src_blue * src_alpha) / out_alpha,
            );
            dst_pixel[3] = intensity_u8(out_alpha * 255.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_highlighter_stroke_covers_a_band_the_width_of_its_nib() {
        // Drawn flat across, a 20px nib covers 20px of height, and the square
        // cap hangs half a nib off each end. Stroked, this was a line and the
        // covered area was the blender's problem. Filled, it is a shape with
        // an extent that can be checked.
        let points = [Point::new(10.0, 50.0), Point::new(90.0, 50.0)];
        let path = smoothed_path(&points).expect("two points make a stroke");
        let outline =
            stroke_outline(&path, 20.0, LineCap::Square).expect("a stroke has an outline");

        let bounds = outline.bounds();
        assert!((bounds.top() - 40.0).abs() < 0.5, "top: {}", bounds.top());
        assert!(
            (bounds.bottom() - 60.0).abs() < 0.5,
            "bottom: {}",
            bounds.bottom()
        );
        assert!((bounds.left() - 0.0).abs() < 0.5, "left: {}", bounds.left());
        assert!(
            (bounds.right() - 100.0).abs() < 0.5,
            "right: {}",
            bounds.right()
        );
    }

    #[test]
    fn a_click_marks_nothing() {
        let click = [Point::new(1.0, 1.0)];
        assert!(smoothed_path(&click).is_none());
        assert!(highlight_shape(&click, 12.0).is_none());
        assert!(smoothed_path(&[]).is_none());
    }
}
