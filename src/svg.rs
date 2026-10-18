//! SVG serialization of glyph outlines.
//!
//! Font coordinates point up; SVG coordinates point down. Every point is
//! emitted as `(x, -y)` and the `viewBox` is the flipped bounding box.

use crate::font::{BoundingBox, GlyphOutline, PathCommand};

/// Renders a glyph outline as a standalone SVG document.
///
/// Returns `None` for outlines that cannot produce a drawable document: no
/// commands, no bounding box, or a degenerate (zero width or height) box.
pub fn render(outline: &GlyphOutline) -> Option<String> {
    if outline.commands.is_empty() {
        return None;
    }
    let view = ViewBox::from_bbox(outline.bbox?)?;

    let mut path = String::new();
    for command in &outline.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                path.push('M');
                point(&mut path, x, y);
            }
            PathCommand::LineTo { x, y } => {
                path.push('L');
                point(&mut path, x, y);
            }
            PathCommand::QuadTo { x1, y1, x, y } => {
                path.push('Q');
                point(&mut path, x1, y1);
                path.push(' ');
                point(&mut path, x, y);
            }
            PathCommand::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                path.push('C');
                point(&mut path, x1, y1);
                path.push(' ');
                point(&mut path, x2, y2);
                path.push(' ');
                point(&mut path, x, y);
            }
            PathCommand::Close => path.push('Z'),
        }
    }

    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n  <path d=\"{}\" fill=\"currentColor\"/>\n</svg>\n",
        view.x, view.y, view.width, view.height, view.width, view.height, path
    ))
}

struct ViewBox {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl ViewBox {
    fn from_bbox(bbox: BoundingBox) -> Option<Self> {
        if !(bbox.x_min.is_finite()
            && bbox.y_min.is_finite()
            && bbox.x_max.is_finite()
            && bbox.y_max.is_finite())
        {
            return None;
        }

        let width = (bbox.x_max - bbox.x_min).round() as i64;
        let height = (bbox.y_max - bbox.y_min).round() as i64;
        if width <= 0 || height <= 0 {
            return None;
        }

        Some(Self {
            x: bbox.x_min.round() as i64,
            y: (-bbox.y_max).round() as i64,
            width,
            height,
        })
    }
}

fn point(out: &mut String, x: f32, y: f32) {
    out.push_str(&number(x));
    out.push(' ');
    out.push_str(&number(-y));
}

fn number(value: f32) -> String {
    (value.round() as i64).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(commands: Vec<PathCommand>, bbox: BoundingBox) -> GlyphOutline {
        GlyphOutline {
            commands,
            bbox: Some(bbox),
        }
    }

    #[test]
    fn flips_the_y_axis() {
        let svg = render(&outline(
            vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 100.0, y: 200.0 },
                PathCommand::Close,
            ],
            BoundingBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 100.0,
                y_max: 200.0,
            },
        ))
        .expect("drawable outline");

        assert!(svg.contains(r#"viewBox="0 -200 100 200""#), "{svg}");
        assert!(svg.contains(r#"d="M0 0L100 -200Z""#), "{svg}");
        assert!(svg.contains(r#"fill="currentColor""#), "{svg}");
    }

    #[test]
    fn formats_curves() {
        let svg = render(&outline(
            vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::QuadTo {
                    x1: 10.0,
                    y1: 20.0,
                    x: 30.0,
                    y: 0.0,
                },
                PathCommand::CurveTo {
                    x1: 1.0,
                    y1: 2.0,
                    x2: 3.0,
                    y2: 4.0,
                    x: 5.0,
                    y: 6.0,
                },
                PathCommand::Close,
            ],
            BoundingBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 30.0,
                y_max: 20.0,
            },
        ))
        .expect("drawable outline");

        assert!(svg.contains("Q10 -20 30 0"), "{svg}");
        assert!(svg.contains("C1 -2 3 -4 5 -6"), "{svg}");
    }

    #[test]
    fn refuses_empty_outlines() {
        let empty = GlyphOutline {
            commands: Vec::new(),
            bbox: None,
        };
        assert!(render(&empty).is_none());
    }

    #[test]
    fn refuses_degenerate_bounding_boxes() {
        let flat = outline(
            vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 0.0, y: 100.0 },
                PathCommand::Close,
            ],
            BoundingBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 0.0,
                y_max: 100.0,
            },
        );
        assert!(render(&flat).is_none());
    }
}
