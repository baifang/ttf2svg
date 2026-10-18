//! Minimal SVG reader: extracts filled shapes, applies transforms, and flattens
//! path data into drawing commands.
//!
//! Supported elements: `<path>`, `<rect>` (including rounded corners),
//! `<circle>`, `<ellipse>`, `<polygon>`, `<polyline>` and `<line>`.
//! Supported path commands: `M L H V C S Q T A Z` in absolute and relative
//! form. `<defs>`, `<clipPath>`, `<mask>`, `<pattern>` and `<marker>` content is
//! ignored. `<use>`, CSS and `fill-rule` are not interpreted.

use std::fmt;

/// A point in SVG user space.
pub type Point = [f64; 2];

/// One drawing command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        ctrl: Point,
        to: Point,
    },
    CubicTo {
        ctrl1: Point,
        ctrl2: Point,
        to: Point,
    },
    Close,
}

/// A malformed or unsupported SVG document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgError(pub String);

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SvgError {}

/// Shape elements that can be converted to filled contours.
const SHAPE_TAGS: [&str; 7] = [
    "path", "rect", "circle", "ellipse", "line", "polygon", "polyline",
];

/// Reads every visible, filled shape of the document, in document order.
pub fn parse_document(text: &str) -> Result<Vec<Segment>, SvgError> {
    let document = roxmltree::Document::parse(text)
        .map_err(|error| SvgError(format!("invalid SVG: {error}")))?;

    let mut segments = Vec::new();
    for node in document
        .descendants()
        .filter(|node| SHAPE_TAGS.iter().any(|tag| node.has_tag_name(*tag)))
    {
        if is_hidden(node) || !is_filled(node) {
            continue;
        }
        let transform = composed_transform(node)?;
        match node.tag_name().name() {
            "path" => {
                if let Some(data) = node.attribute("d") {
                    segments.extend(parse_path(data, transform)?);
                }
            }
            "rect" => segments.extend(parse_rect(node, transform)?),
            "circle" => segments.extend(parse_ellipse(node, transform, true)?),
            "ellipse" => segments.extend(parse_ellipse(node, transform, false)?),
            "polygon" => segments.extend(parse_polygon(node, transform, true)?),
            "polyline" => segments.extend(parse_polygon(node, transform, false)?),
            "line" => segments.extend(parse_line(node, transform)?),
            _ => {}
        }
    }
    Ok(segments)
}

fn is_hidden(node: roxmltree::Node<'_, '_>) -> bool {
    node.ancestors().any(|ancestor| {
        ["defs", "symbol", "clipPath", "mask", "pattern", "marker"]
            .iter()
            .any(|name| ancestor.has_tag_name(*name))
    })
}

/// Whether the element is filled. The nearest `fill` attribute wins; SVG's
/// default is black, so a missing attribute means filled.
fn is_filled(node: roxmltree::Node<'_, '_>) -> bool {
    for ancestor in node.ancestors() {
        let Some(fill) = ancestor.attribute("fill") else {
            continue;
        };
        let fill = fill.trim();
        return !(fill.eq_ignore_ascii_case("none") || fill.eq_ignore_ascii_case("transparent"));
    }
    true
}

fn composed_transform(node: roxmltree::Node<'_, '_>) -> Result<Transform, SvgError> {
    let mut chain: Vec<_> = node.ancestors().collect();
    chain.reverse();

    let mut transform = Transform::IDENTITY;
    for ancestor in chain {
        if let Some(value) = ancestor.attribute("transform") {
            transform = transform * parse_transform(value)?;
        }
    }
    Ok(transform)
}

fn number_attribute(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<f64>, SvgError> {
    let Some(raw) = node.attribute(name) else {
        return Ok(None);
    };
    raw.trim().parse::<f64>().map(Some).map_err(|_| {
        SvgError(format!(
            "invalid {name} attribute \"{raw}\" on <{}>",
            node.tag_name().name()
        ))
    })
}

fn number_or(node: roxmltree::Node<'_, '_>, name: &str, default: f64) -> Result<f64, SvgError> {
    Ok(number_attribute(node, name)?.unwrap_or(default))
}

fn push_move(out: &mut Vec<Segment>, transform: Transform, point: Point) {
    out.push(Segment::MoveTo(transform.apply(point)));
}

fn push_line(out: &mut Vec<Segment>, transform: Transform, point: Point) {
    out.push(Segment::LineTo(transform.apply(point)));
}

#[allow(clippy::too_many_arguments)]
fn push_arc(
    out: &mut Vec<Segment>,
    transform: Transform,
    from: Point,
    rx: f64,
    ry: f64,
    to: Point,
    sweep: bool,
) {
    let arc = Arc {
        rx,
        ry,
        rotation: 0.0,
        large_arc: false,
        sweep,
        to,
    };
    for (ctrl1, ctrl2, end) in arc_to_cubics(from, arc) {
        out.push(Segment::CubicTo {
            ctrl1: transform.apply(ctrl1),
            ctrl2: transform.apply(ctrl2),
            to: transform.apply(end),
        });
    }
}

fn parse_rect(
    node: roxmltree::Node<'_, '_>,
    transform: Transform,
) -> Result<Vec<Segment>, SvgError> {
    let x = number_or(node, "x", 0.0)?;
    let y = number_or(node, "y", 0.0)?;
    let width = number_or(node, "width", 0.0)?;
    let height = number_or(node, "height", 0.0)?;
    if width <= 0.0 || height <= 0.0 {
        return Ok(Vec::new());
    }

    let (mut rx, mut ry) = match (number_attribute(node, "rx")?, number_attribute(node, "ry")?) {
        (Some(rx), Some(ry)) => (rx.abs(), ry.abs()),
        (Some(rx), None) => (rx.abs(), rx.abs()),
        (None, Some(ry)) => (ry.abs(), ry.abs()),
        (None, None) => (0.0, 0.0),
    };
    rx = rx.min(width * 0.5);
    ry = ry.min(height * 0.5);

    let mut out = Vec::new();
    if rx == 0.0 || ry == 0.0 {
        push_move(&mut out, transform, [x, y]);
        push_line(&mut out, transform, [x + width, y]);
        push_line(&mut out, transform, [x + width, y + height]);
        push_line(&mut out, transform, [x, y + height]);
        out.push(Segment::Close);
        return Ok(out);
    }

    push_move(&mut out, transform, [x + rx, y]);
    push_line(&mut out, transform, [x + width - rx, y]);
    push_arc(
        &mut out,
        transform,
        [x + width - rx, y],
        rx,
        ry,
        [x + width, y + ry],
        true,
    );
    push_line(&mut out, transform, [x + width, y + height - ry]);
    push_arc(
        &mut out,
        transform,
        [x + width, y + height - ry],
        rx,
        ry,
        [x + width - rx, y + height],
        true,
    );
    push_line(&mut out, transform, [x + rx, y + height]);
    push_arc(
        &mut out,
        transform,
        [x + rx, y + height],
        rx,
        ry,
        [x, y + height - ry],
        true,
    );
    push_line(&mut out, transform, [x, y + ry]);
    push_arc(&mut out, transform, [x, y + ry], rx, ry, [x + rx, y], true);
    out.push(Segment::Close);
    Ok(out)
}

fn parse_ellipse(
    node: roxmltree::Node<'_, '_>,
    transform: Transform,
    circle: bool,
) -> Result<Vec<Segment>, SvgError> {
    let cx = number_or(node, "cx", 0.0)?;
    let cy = number_or(node, "cy", 0.0)?;
    let (rx, ry) = if circle {
        let radius = number_or(node, "r", 0.0)?;
        (radius, radius)
    } else {
        (number_or(node, "rx", 0.0)?, number_or(node, "ry", 0.0)?)
    };
    if rx <= 0.0 || ry <= 0.0 {
        return Ok(Vec::new());
    }

    let right = [cx + rx, cy];
    let bottom = [cx, cy + ry];
    let left = [cx - rx, cy];
    let top = [cx, cy - ry];

    let mut out = Vec::new();
    push_move(&mut out, transform, right);
    push_arc(&mut out, transform, right, rx, ry, bottom, true);
    push_arc(&mut out, transform, bottom, rx, ry, left, true);
    push_arc(&mut out, transform, left, rx, ry, top, true);
    push_arc(&mut out, transform, top, rx, ry, right, true);
    out.push(Segment::Close);
    Ok(out)
}

fn parse_polygon(
    node: roxmltree::Node<'_, '_>,
    transform: Transform,
    close: bool,
) -> Result<Vec<Segment>, SvgError> {
    let Some(raw) = node.attribute("points") else {
        return Ok(Vec::new());
    };
    let points = parse_points(raw)?;
    if points.len() < 2 {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(points.len() + 1);
    push_move(&mut out, transform, points[0]);
    for point in &points[1..] {
        push_line(&mut out, transform, *point);
    }
    if close {
        out.push(Segment::Close);
    }
    Ok(out)
}

fn parse_line(
    node: roxmltree::Node<'_, '_>,
    transform: Transform,
) -> Result<Vec<Segment>, SvgError> {
    let x1 = number_or(node, "x1", 0.0)?;
    let y1 = number_or(node, "y1", 0.0)?;
    let x2 = number_or(node, "x2", 0.0)?;
    let y2 = number_or(node, "y2", 0.0)?;

    Ok(vec![
        Segment::MoveTo(transform.apply([x1, y1])),
        Segment::LineTo(transform.apply([x2, y2])),
    ])
}

fn parse_points(raw: &str) -> Result<Vec<Point>, SvgError> {
    let mut cursor = Cursor::new(raw.as_bytes());
    let mut values = Vec::new();
    loop {
        cursor.skip_separators();
        if cursor.peek().is_none() {
            break;
        }
        values.push(cursor.number()?);
    }

    if values.len() % 2 != 0 {
        return Err(SvgError(
            "points attribute has an odd number of coordinates".to_owned(),
        ));
    }

    Ok(values
        .chunks_exact(2)
        .map(|pair| [pair[0], pair[1]])
        .collect())
}

/// Parses a `d` attribute, applying `transform` to every emitted point.
pub fn parse_path(data: &str, transform: Transform) -> Result<Vec<Segment>, SvgError> {
    let mut cursor = Cursor::new(data.as_bytes());
    let mut segments = Vec::new();

    let mut current = [0.0, 0.0];
    let mut start = [0.0, 0.0];
    let mut command: Option<u8> = None;
    let mut last_command: Option<u8> = None;
    let mut last_cubic_ctrl: Option<Point> = None;
    let mut last_quad_ctrl: Option<Point> = None;

    loop {
        cursor.skip_separators();
        let Some(byte) = cursor.peek() else {
            break;
        };

        if byte.is_ascii_alphabetic() {
            cursor.advance(1);
            command = Some(byte);
        } else if command.is_none() {
            return Err(cursor.error("path data must begin with a command"));
        }

        let Some(letter) = command else {
            return Err(cursor.error("unexpected number in path data"));
        };
        let relative = letter.is_ascii_lowercase();
        let upper = letter.to_ascii_uppercase();

        if segments.is_empty() && upper != b'M' {
            return Err(cursor.error("path data must begin with a moveto command"));
        }

        match upper {
            b'M' => {
                let to = cursor.point(relative, current)?;
                segments.push(Segment::MoveTo(transform.apply(to)));
                current = to;
                start = to;
                command = Some(if relative { b'l' } else { b'L' });
            }
            b'L' => {
                let to = cursor.point(relative, current)?;
                segments.push(Segment::LineTo(transform.apply(to)));
                current = to;
            }
            b'H' => {
                let x = cursor.number()?;
                let to = [if relative { current[0] + x } else { x }, current[1]];
                segments.push(Segment::LineTo(transform.apply(to)));
                current = to;
            }
            b'V' => {
                let y = cursor.number()?;
                let to = [current[0], if relative { current[1] + y } else { y }];
                segments.push(Segment::LineTo(transform.apply(to)));
                current = to;
            }
            b'C' => {
                let ctrl1 = cursor.point(relative, current)?;
                let ctrl2 = cursor.point(relative, current)?;
                let to = cursor.point(relative, current)?;
                segments.push(Segment::CubicTo {
                    ctrl1: transform.apply(ctrl1),
                    ctrl2: transform.apply(ctrl2),
                    to: transform.apply(to),
                });
                last_cubic_ctrl = Some(ctrl2);
                current = to;
            }
            b'S' => {
                let ctrl1 = match (last_command, last_cubic_ctrl) {
                    (Some(b'C' | b'S'), Some(previous)) => [
                        2.0 * current[0] - previous[0],
                        2.0 * current[1] - previous[1],
                    ],
                    _ => current,
                };
                let ctrl2 = cursor.point(relative, current)?;
                let to = cursor.point(relative, current)?;
                segments.push(Segment::CubicTo {
                    ctrl1: transform.apply(ctrl1),
                    ctrl2: transform.apply(ctrl2),
                    to: transform.apply(to),
                });
                last_cubic_ctrl = Some(ctrl2);
                current = to;
            }
            b'Q' => {
                let ctrl = cursor.point(relative, current)?;
                let to = cursor.point(relative, current)?;
                segments.push(Segment::QuadTo {
                    ctrl: transform.apply(ctrl),
                    to: transform.apply(to),
                });
                last_quad_ctrl = Some(ctrl);
                current = to;
            }
            b'T' => {
                let ctrl = match (last_command, last_quad_ctrl) {
                    (Some(b'Q' | b'T'), Some(previous)) => [
                        2.0 * current[0] - previous[0],
                        2.0 * current[1] - previous[1],
                    ],
                    _ => current,
                };
                let to = cursor.point(relative, current)?;
                segments.push(Segment::QuadTo {
                    ctrl: transform.apply(ctrl),
                    to: transform.apply(to),
                });
                last_quad_ctrl = Some(ctrl);
                current = to;
            }
            b'A' => {
                let rx = cursor.number()?;
                let ry = cursor.number()?;
                let rotation = cursor.number()?;
                let large_arc = cursor.flag()?;
                let sweep = cursor.flag()?;
                let to = cursor.point(relative, current)?;
                let arc = Arc {
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    to,
                };
                for (ctrl1, ctrl2, end) in arc_to_cubics(current, arc) {
                    segments.push(Segment::CubicTo {
                        ctrl1: transform.apply(ctrl1),
                        ctrl2: transform.apply(ctrl2),
                        to: transform.apply(end),
                    });
                }
                current = to;
            }
            b'Z' => {
                segments.push(Segment::Close);
                current = start;
                command = None;
            }
            _ => return Err(cursor.error(&format!("unsupported path command '{letter}'"))),
        }

        last_command = Some(letter);
        if upper != b'C' && upper != b'S' {
            last_cubic_ctrl = if upper == b'C' || upper == b'S' {
                last_cubic_ctrl
            } else {
                None
            };
        }
        if upper != b'Q' && upper != b'T' {
            last_quad_ctrl = if upper == b'Q' || upper == b'T' {
                last_quad_ctrl
            } else {
                None
            };
        }
    }

    Ok(segments)
}

/// A 2D affine transform: `[a c e; b d f; 0 0 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Applies the transform to a point.
    pub fn apply(self, point: Point) -> Point {
        [
            self.a * point[0] + self.c * point[1] + self.e,
            self.b * point[0] + self.d * point[1] + self.f,
        ]
    }
}

impl std::ops::Mul for Transform {
    type Output = Self;

    /// Matrix product: `other` is applied first.
    fn mul(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
}

fn parse_transform(value: &str) -> Result<Transform, SvgError> {
    let mut cursor = Cursor::new(value.as_bytes());
    let mut total = Transform::IDENTITY;

    loop {
        cursor.skip_separators();
        if cursor.peek().is_none() {
            break;
        }

        let name = cursor.ident()?;
        cursor.skip_separators();
        if !cursor.eat(b'(') {
            return Err(cursor.error("expected '(' in transform"));
        }

        let mut arguments = Vec::new();
        loop {
            cursor.skip_separators();
            if cursor.eat(b')') {
                break;
            }
            arguments.push(cursor.number()?);
        }

        let transform = match (name, arguments.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Transform {
                a: *a,
                b: *b,
                c: *c,
                d: *d,
                e: *e,
                f: *f,
            },
            ("translate", [x]) => Transform {
                e: *x,
                ..Transform::IDENTITY
            },
            ("translate", [x, y]) => Transform {
                e: *x,
                f: *y,
                ..Transform::IDENTITY
            },
            ("scale", [x]) => Transform {
                a: *x,
                d: *x,
                ..Transform::IDENTITY
            },
            ("scale", [x, y]) => Transform {
                a: *x,
                d: *y,
                ..Transform::IDENTITY
            },
            ("rotate", [angle]) => rotation(*angle),
            ("rotate", [angle, cx, cy]) => {
                Transform {
                    e: *cx,
                    f: *cy,
                    ..Transform::IDENTITY
                } * rotation(*angle)
                    * Transform {
                        e: -*cx,
                        f: -*cy,
                        ..Transform::IDENTITY
                    }
            }
            ("skewX", [angle]) => Transform {
                c: angle.to_radians().tan(),
                ..Transform::IDENTITY
            },
            ("skewY", [angle]) => Transform {
                b: angle.to_radians().tan(),
                ..Transform::IDENTITY
            },
            _ => {
                return Err(SvgError(format!(
                    "unsupported transform \"{name}({})\"",
                    arguments
                        .iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                )))
            }
        };

        total = total * transform;
    }

    Ok(total)
}

fn rotation(angle_degrees: f64) -> Transform {
    let (sin, cos) = angle_degrees.to_radians().sin_cos();
    Transform {
        a: cos,
        b: sin,
        c: -sin,
        d: cos,
        e: 0.0,
        f: 0.0,
    }
}

struct Arc {
    rx: f64,
    ry: f64,
    rotation: f64,
    large_arc: bool,
    sweep: bool,
    to: Point,
}

/// Converts an SVG elliptical arc into cubic Bezier segments (SVG 1.1 F.6).
fn arc_to_cubics(from: Point, arc: Arc) -> Vec<(Point, Point, Point)> {
    let Arc {
        rx,
        ry,
        rotation,
        large_arc,
        sweep,
        to,
    } = arc;

    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx == 0.0 || ry == 0.0 || from == to {
        return Vec::new();
    }

    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from[0] - to[0]) / 2.0;
    let dy = (from[1] - to[1]) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;

    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let mut coefficient = if denominator == 0.0 {
        0.0
    } else {
        (numerator / denominator).max(0.0).sqrt()
    };
    if large_arc == sweep {
        coefficient = -coefficient;
    }

    let cxp = coefficient * (rx * y1 / ry);
    let cyp = coefficient * (-ry * x1 / rx);
    let cx = cos_phi * cxp - sin_phi * cyp + (from[0] + to[0]) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (from[1] + to[1]) / 2.0;

    let ux = (x1 - cxp) / rx;
    let uy = (y1 - cyp) / ry;
    let vx = (-x1 - cxp) / rx;
    let vy = (-y1 - cyp) / ry;

    let start_angle = uy.atan2(ux);
    let mut delta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }

    let count = ((delta.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = delta / count as f64;
    let mut cubics = Vec::with_capacity(count);
    let mut angle = start_angle;

    for _ in 0..count {
        let next_angle = angle + step;
        let alpha = 4.0 / 3.0 * (step / 4.0).tan();

        let (sin_a, cos_a) = angle.sin_cos();
        let (sin_b, cos_b) = next_angle.sin_cos();

        let p0 = [
            cx + rx * cos_a * cos_phi - ry * sin_a * sin_phi,
            cy + rx * cos_a * sin_phi + ry * sin_a * cos_phi,
        ];
        let p3 = [
            cx + rx * cos_b * cos_phi - ry * sin_b * sin_phi,
            cy + rx * cos_b * sin_phi + ry * sin_b * cos_phi,
        ];
        let d0 = [
            -rx * sin_a * cos_phi - ry * cos_a * sin_phi,
            -rx * sin_a * sin_phi + ry * cos_a * cos_phi,
        ];
        let d1 = [
            -rx * sin_b * cos_phi - ry * cos_b * sin_phi,
            -rx * sin_b * sin_phi + ry * cos_b * cos_phi,
        ];

        cubics.push((
            [p0[0] + alpha * d0[0], p0[1] + alpha * d0[1]],
            [p3[0] - alpha * d1[0], p3[1] - alpha * d1[1]],
            p3,
        ));
        angle = next_angle;
    }

    cubics
}

struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.position).copied()
    }

    fn advance(&mut self, count: usize) {
        self.position += count;
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn skip_separators(&mut self) {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() || byte == b',' {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn error(&self, message: &str) -> SvgError {
        SvgError(format!("{message} (at byte {})", self.position))
    }

    fn ident(&mut self) -> Result<&'a str, SvgError> {
        let start = self.position;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphabetic() {
                self.position += 1;
            } else {
                break;
            }
        }
        if self.position == start {
            return Err(self.error("expected a name"));
        }
        std::str::from_utf8(&self.data[start..self.position])
            .map_err(|_| self.error("invalid UTF-8 in transform"))
    }

    fn flag(&mut self) -> Result<bool, SvgError> {
        self.skip_separators();
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(self.error("expected an arc flag (0 or 1)")),
        }
    }

    fn point(&mut self, relative: bool, current: Point) -> Result<Point, SvgError> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(if relative {
            [current[0] + x, current[1] + y]
        } else {
            [x, y]
        })
    }

    fn number(&mut self) -> Result<f64, SvgError> {
        self.skip_separators();
        let start = self.position;

        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }

        let mut digits = 0;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.position += 1;
            digits += 1;
        }
        if self.eat(b'.') {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(self.error("expected a number"));
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            let exponent_start = self.position;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let mut exponent_digits = 0;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
                exponent_digits += 1;
            }
            if exponent_digits == 0 {
                self.position = exponent_start;
            }
        }

        let text = std::str::from_utf8(&self.data[start..self.position])
            .map_err(|_| self.error("invalid UTF-8 in path data"))?;
        text.parse::<f64>()
            .map_err(|_| self.error("invalid number in path data"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(data: &str) -> Vec<Segment> {
        parse_path(data, Transform::IDENTITY).expect("valid path")
    }

    #[test]
    fn parses_absolute_lines() {
        assert_eq!(
            path("M0 0 L10 0 L10 10 Z"),
            vec![
                Segment::MoveTo([0.0, 0.0]),
                Segment::LineTo([10.0, 0.0]),
                Segment::LineTo([10.0, 10.0]),
                Segment::Close,
            ]
        );
    }

    #[test]
    fn parses_relative_commands() {
        assert_eq!(
            path("m1 1 l2 0 v2 h-2 z"),
            vec![
                Segment::MoveTo([1.0, 1.0]),
                Segment::LineTo([3.0, 1.0]),
                Segment::LineTo([3.0, 3.0]),
                Segment::LineTo([1.0, 3.0]),
                Segment::Close,
            ]
        );
    }

    #[test]
    fn implicit_lineto_after_moveto() {
        assert_eq!(
            path("M0 0 10 0 10 10"),
            vec![
                Segment::MoveTo([0.0, 0.0]),
                Segment::LineTo([10.0, 0.0]),
                Segment::LineTo([10.0, 10.0]),
            ]
        );
    }

    #[test]
    fn parses_curves_and_shorthand() {
        assert_eq!(
            path("M0 0 Q5 10 10 0 T20 0"),
            vec![
                Segment::MoveTo([0.0, 0.0]),
                Segment::QuadTo {
                    ctrl: [5.0, 10.0],
                    to: [10.0, 0.0]
                },
                Segment::QuadTo {
                    ctrl: [15.0, -10.0],
                    to: [20.0, 0.0]
                },
            ]
        );
    }

    #[test]
    fn applies_transforms() {
        let transform = Transform {
            a: 2.0,
            d: 3.0,
            e: 1.0,
            f: -1.0,
            ..Transform::IDENTITY
        };
        let segments = parse_path("M1 1 L2 2", transform).expect("valid");
        assert_eq!(segments[0], Segment::MoveTo([3.0, 2.0]));
        assert_eq!(segments[1], Segment::LineTo([5.0, 5.0]));
    }

    #[test]
    fn parses_transform_lists() {
        let transform = parse_transform("translate(10 20) scale(2)").expect("valid");
        assert_eq!(transform.apply([1.0, 1.0]), [12.0, 22.0]);
    }

    #[test]
    fn parses_documents_with_transformed_paths() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(5 5)"><path d="M0 0 L1 0 Z"/></g>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");
        assert_eq!(segments[0], Segment::MoveTo([5.0, 5.0]));
    }

    #[test]
    fn converts_arcs_to_curves_ending_at_the_target() {
        let segments = path("M0 0 A10 10 0 0 1 10 10");
        assert_eq!(segments[0], Segment::MoveTo([0.0, 0.0]));
        let Segment::CubicTo { to, .. } = segments[segments.len() - 1] else {
            panic!("arc should end with a cubic");
        };
        assert!((to[0] - 10.0).abs() < 1e-9, "{to:?}");
        assert!((to[1] - 10.0).abs() < 1e-9, "{to:?}");
    }

    #[test]
    fn skips_unfilled_paths() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M0 0 H24 V24 H0 Z" fill="none"/>
            <path d="M1 1 H2 V2 H1 Z"/>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");

        assert_eq!(segments[0], Segment::MoveTo([1.0, 1.0]));
        assert_eq!(segments.len(), 5);
    }

    #[test]
    fn inherits_fill_none_from_ancestors() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <g fill="none"><path d="M0 0 H1 V1 H0 Z"/></g>
        </svg>"#;
        assert!(parse_document(svg).expect("valid svg").is_empty());
    }

    #[test]
    fn nearest_fill_attribute_wins() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <g fill="none"><path fill="#000" d="M0 0 H1 V1 H0 Z"/></g>
        </svg>"##;
        assert!(!parse_document(svg).expect("valid svg").is_empty());
    }

    #[test]
    fn parses_rectangles() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <rect x="1" y="2" width="3" height="4"/>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");

        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], Segment::MoveTo([1.0, 2.0]));
        assert_eq!(segments[1], Segment::LineTo([4.0, 2.0]));
        assert_eq!(segments[2], Segment::LineTo([4.0, 6.0]));
        assert_eq!(segments[3], Segment::LineTo([1.0, 6.0]));
        assert_eq!(segments[4], Segment::Close);
    }

    #[test]
    fn parses_rounded_rectangles() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="10" height="10" rx="2"/>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");

        assert!(segments.len() > 5);
        assert_eq!(segments[0], Segment::MoveTo([2.0, 0.0]));
        assert!(segments
            .iter()
            .any(|segment| matches!(segment, Segment::CubicTo { .. })));
    }

    #[test]
    fn parses_circles_and_ellipses() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <circle cx="5" cy="5" r="5"/>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");

        assert_eq!(segments.len(), 6);
        assert_eq!(segments[0], Segment::MoveTo([10.0, 5.0]));
        assert_eq!(segments[5], Segment::Close);
        for segment in &segments[1..5] {
            let Segment::CubicTo { to, .. } = segment else {
                panic!("quarter arcs should become cubics");
            };
            let dx = to[0] - 5.0;
            let dy = to[1] - 5.0;
            assert!((dx * dx + dy * dy - 25.0).abs() < 1e-6, "{to:?}");
        }
    }

    #[test]
    fn parses_polygons_polylines_and_lines() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <polygon points="0,0 10,0 10,10"/>
            <polyline points="1-2 3-4"/>
            <line x1="0" y1="0" x2="5" y2="5"/>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");

        assert_eq!(segments.len(), 8);
        assert_eq!(segments[0], Segment::MoveTo([0.0, 0.0]));
        assert_eq!(segments[3], Segment::Close);
        assert_eq!(segments[4], Segment::MoveTo([1.0, -2.0]));
        assert_eq!(segments[5], Segment::LineTo([3.0, -4.0]));
        assert_eq!(segments[6], Segment::MoveTo([0.0, 0.0]));
        assert_eq!(segments[7], Segment::LineTo([5.0, 5.0]));
    }

    #[test]
    fn transforms_shapes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(1 2)"><rect width="1" height="1"/></g>
        </svg>"#;
        let segments = parse_document(svg).expect("valid svg");
        assert_eq!(segments[0], Segment::MoveTo([1.0, 2.0]));
    }

    #[test]
    fn skips_unfilled_shapes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <rect fill="none" width="10" height="10"/>
            <circle fill="transparent" cx="5" cy="5" r="4"/>
        </svg>"#;
        assert!(parse_document(svg).expect("valid svg").is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_path("L10 10", Transform::IDENTITY).is_err());
        assert!(parse_path("M", Transform::IDENTITY).is_err());
        assert!(parse_path("M0 0 K1 1", Transform::IDENTITY).is_err());
    }
}
