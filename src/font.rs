//! Font loading and glyph outline extraction.

use std::path::Path;

use ttf_parser::{Face, OutlineBuilder, Rect};

use crate::error::AppError;

/// An axis-aligned bounding box in font units (y axis pointing up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

impl From<Rect> for BoundingBox {
    fn from(rect: Rect) -> Self {
        Self {
            x_min: f32::from(rect.x_min),
            y_min: f32::from(rect.y_min),
            x_max: f32::from(rect.x_max),
            y_max: f32::from(rect.y_max),
        }
    }
}

/// One drawing command of a glyph outline, in font units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        x1: f32,
        y1: f32,
        x: f32,
        y: f32,
    },
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    Close,
}

/// The outline of a single glyph.
#[derive(Debug, Clone, Default)]
pub struct GlyphOutline {
    pub commands: Vec<PathCommand>,
    pub bbox: Option<BoundingBox>,
}

impl GlyphOutline {
    /// A glyph without contours, such as a space.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// A parsed font face.
pub struct Font<'a> {
    face: Face<'a>,
}

impl<'a> Font<'a> {
    /// Parses the first face of `data`, rejecting unsupported containers.
    pub fn parse(data: &'a [u8], path: &Path) -> Result<Self, AppError> {
        if data.starts_with(b"wOFF") || data.starts_with(b"wOF2") {
            return Err(AppError::WoffInput {
                path: path.to_owned(),
            });
        }
        if data.starts_with(b"ttcf") {
            return Err(AppError::CollectionInput {
                path: path.to_owned(),
            });
        }

        let face = Face::parse(data, 0).map_err(|source| AppError::ParseFont {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self { face })
    }

    /// Looks up `code_point` and returns its outline.
    ///
    /// Returns `None` when the font has no mapping for the code point. A
    /// mapped glyph without contours (for example a space) is returned as an
    /// empty outline.
    pub fn outline(&self, code_point: u32) -> Option<GlyphOutline> {
        let character = char::from_u32(code_point)?;
        let glyph_id = self.face.glyph_index(character)?;

        let mut collector = Collector::default();
        let bbox = self
            .face
            .outline_glyph(glyph_id, &mut collector)
            .map(BoundingBox::from);

        Some(GlyphOutline {
            commands: collector.commands,
            bbox,
        })
    }
}

#[derive(Default)]
struct Collector {
    commands: Vec<PathCommand>,
}

impl OutlineBuilder for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(PathCommand::QuadTo { x1, y1, x, y });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(PathCommand::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }
}
