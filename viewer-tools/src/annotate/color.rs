// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::Color;

/// Preset annotation colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnotateColor(pub Color);

impl AnnotateColor {
    pub const WHITE: Self = Self(Color::WHITE);
    pub const RED: Self = Self(Color::from_rgb(1.0, 0.0, 0.0));
    pub const ORANGE: Self = Self(Color::from_rgb(1.0, 0.65, 0.0));
    pub const GREEN: Self = Self(Color::from_rgb(0.0, 1.0, 0.0));
    pub const BLUE: Self = Self(Color::from_rgb(0.0, 0.0, 1.0));
    pub const BLACK: Self = Self(Color::BLACK);

    /// The palette, in the order the swatches are shown.
    pub const PRESETS: [Self; 6] = [
        Self::WHITE,
        Self::RED,
        Self::ORANGE,
        Self::GREEN,
        Self::BLUE,
        Self::BLACK,
    ];

    #[must_use]
    pub fn presets() -> Vec<Self> {
        Self::PRESETS.to_vec()
    }
}

impl Default for AnnotateColor {
    fn default() -> Self {
        Self::BLACK
    }
}

impl From<AnnotateColor> for Color {
    fn from(c: AnnotateColor) -> Self {
        c.0
    }
}

impl From<Color> for AnnotateColor {
    fn from(c: Color) -> Self {
        Self(c)
    }
}
