//! Conversion of SVG outlines into triangle meshes.
//!
//! The pipeline is: flatten curves, drop degenerate contours, determine which
//! contours are holes by containment, normalize winding, then triangulate each
//! outer contour together with its direct holes using ear clipping.

use thiserror::Error;

use crate::svgpath::{Point, Segment};

/// A filled 2D mesh.
#[derive(Debug, Clone)]
pub struct Mesh2D {
    pub vertices: Vec<Point>,
    pub indices: Vec<u32>,
    pub bounds: [f64; 4],
}

/// An extruded 3D mesh with one normal per triangle.
#[derive(Debug, Clone)]
pub struct Mesh3D {
    pub vertices: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
    /// Three components per triangle; length is `indices.len()`.
    pub normals: Vec<[f64; 3]>,
    pub bounds: [f64; 4],
}

/// A mesh in either supported dimensionality.
#[derive(Debug, Clone)]
pub enum Mesh {
    Two(Mesh2D),
    Three(Mesh3D),
}

impl Mesh {
    pub fn dimensions(&self) -> u8 {
        match self {
            Self::Two(_) => 2,
            Self::Three(_) => 3,
        }
    }
}

#[derive(Debug, Error)]
pub enum MeshError {
    #[error("the outline has no fillable contours")]
    NoContours,

    #[error("triangulation failed: {0}")]
    Triangulation(String),

    #[error("extrusion depth must be positive, got {0}")]
    InvalidDepth(f64),

    #[error("the mesh needs {0} vertices; at most 65535 are supported")]
    TooManyVertices(usize),
}

/// Flattens `segments` and triangulates them into a 2D mesh.
pub fn build_2d(segments: &[Segment], tolerance: f64) -> Result<Mesh2D, MeshError> {
    let prepared = prepare(segments, tolerance)?;
    let triangulated = triangulate(&prepared)?;

    if triangulated.vertices.len() > usize::from(u16::MAX) {
        return Err(MeshError::TooManyVertices(triangulated.vertices.len()));
    }

    Ok(Mesh2D {
        bounds: bounds_of(&triangulated.vertices),
        vertices: triangulated.vertices,
        indices: triangulated.indices,
    })
}

/// Flattens `segments`, triangulates them, and extrudes the result along +Z.
pub fn build_3d(segments: &[Segment], tolerance: f64, depth: f64) -> Result<Mesh3D, MeshError> {
    if !(depth.is_finite() && depth > 0.0) {
        return Err(MeshError::InvalidDepth(depth));
    }

    let prepared = prepare(segments, tolerance)?;
    let triangulated = triangulate(&prepared)?;
    let count = triangulated.vertices.len();
    if count * 2 > usize::from(u16::MAX) {
        return Err(MeshError::TooManyVertices(count * 2));
    }

    let mut vertices = Vec::with_capacity(count * 2);
    for point in &triangulated.vertices {
        vertices.push([point[0], point[1], 0.0]);
    }
    for point in &triangulated.vertices {
        vertices.push([point[0], point[1], depth]);
    }

    let offset = count as u32;
    let mut indices = Vec::new();

    // Caps: the front face points to -Z, the back face to +Z.
    for triangle in triangulated.indices.chunks_exact(3) {
        let (a, b, c) = (triangle[0], triangle[1], triangle[2]);
        if cross_z(&triangulated.vertices, a, b, c) > 0.0 {
            indices.extend_from_slice(&[a, c, b, a + offset, b + offset, c + offset]);
        } else {
            indices.extend_from_slice(&[a, b, c, a + offset, c + offset, b + offset]);
        }
    }

    // Side walls: outer contours are counter-clockwise, holes clockwise, so
    // this winding points away from the material for both.
    for (ring_index, ring) in prepared.rings.iter().enumerate() {
        let start = triangulated.ring_offsets[ring_index] as u32;
        let length = ring.len() as u32;
        for corner in 0..length {
            let current = start + corner;
            let next = start + (corner + 1) % length;
            let (front_a, front_b) = (current, next);
            let (back_a, back_b) = (current + offset, next + offset);
            indices.extend_from_slice(&[front_a, front_b, back_b]);
            indices.extend_from_slice(&[front_a, back_b, back_a]);
        }
    }

    let mut normals = Vec::with_capacity(indices.len() / 3);
    for triangle in indices.chunks_exact(3) {
        normals.push(triangle_normal(
            vertices[triangle[0] as usize],
            vertices[triangle[1] as usize],
            vertices[triangle[2] as usize],
        ));
    }

    Ok(Mesh3D {
        bounds: bounds_of(&triangulated.vertices),
        vertices,
        indices,
        normals,
    })
}

struct Prepared {
    rings: Vec<Vec<Point>>,
    parents: Vec<Option<usize>>,
    depths: Vec<usize>,
}

fn prepare(segments: &[Segment], tolerance: f64) -> Result<Prepared, MeshError> {
    let mut contours: Vec<Vec<Point>> = flatten(segments, tolerance)
        .into_iter()
        .filter(|contour| contour.len() >= 3)
        .collect();
    contours.retain(|contour| signed_area(contour).abs() > 1e-9);

    if contours.is_empty() {
        return Err(MeshError::NoContours);
    }

    let areas: Vec<f64> = contours
        .iter()
        .map(|contour| signed_area(contour))
        .collect();
    let mut parents = vec![None; contours.len()];

    for (index, contour) in contours.iter().enumerate() {
        let probe = contour[0];
        let mut best: Option<(usize, f64)> = None;
        for (other, candidate) in contours.iter().enumerate() {
            if other == index || !point_in_polygon(probe, candidate) {
                continue;
            }
            let area = areas[other].abs();
            if best.is_none_or(|(_, best_area)| area < best_area) {
                best = Some((other, area));
            }
        }
        parents[index] = best.map(|(parent, _)| parent);
    }

    let mut depths = vec![0_usize; contours.len()];
    for (index, mut current) in parents.iter().copied().enumerate() {
        let mut depth = 0;
        while let Some(parent) = current {
            depth += 1;
            current = parents[parent];
            if depth > parents.len() {
                break;
            }
        }
        depths[index] = depth;
    }

    // Even depth is solid, odd depth is a hole. This also makes the side-wall
    // winding in `build_3d` point outwards.
    for (index, ring) in contours.iter_mut().enumerate() {
        let want_counter_clockwise = depths[index] % 2 == 0;
        if (signed_area(ring) > 0.0) != want_counter_clockwise {
            ring.reverse();
        }
    }

    Ok(Prepared {
        rings: contours,
        parents,
        depths,
    })
}

struct Triangulated {
    vertices: Vec<Point>,
    indices: Vec<u32>,
    ring_offsets: Vec<usize>,
}

fn triangulate(prepared: &Prepared) -> Result<Triangulated, MeshError> {
    let mut vertices = Vec::new();
    let mut ring_offsets = Vec::with_capacity(prepared.rings.len());
    for ring in &prepared.rings {
        ring_offsets.push(vertices.len());
        vertices.extend_from_slice(ring);
    }

    let mut indices: Vec<u32> = Vec::new();

    for (index, ring) in prepared.rings.iter().enumerate() {
        if prepared.depths[index] % 2 != 0 {
            continue;
        }

        let holes: Vec<usize> = (0..prepared.rings.len())
            .filter(|&other| prepared.parents[other] == Some(index))
            .collect();

        let mut local = ring.clone();
        let mut hole_starts = Vec::with_capacity(holes.len());
        for &hole in &holes {
            hole_starts.push(local.len());
            local.extend_from_slice(&prepared.rings[hole]);
        }

        let mut flat = Vec::with_capacity(local.len() * 2);
        for point in &local {
            flat.push(point[0]);
            flat.push(point[1]);
        }

        let triangles = earcutr::earcut(&flat, &hole_starts, 2)
            .map_err(|error| MeshError::Triangulation(error.to_string()))?;

        let mut mapping = Vec::with_capacity(local.len());
        mapping.extend(ring_offsets[index]..ring_offsets[index] + ring.len());
        for &hole in &holes {
            mapping.extend(ring_offsets[hole]..ring_offsets[hole] + prepared.rings[hole].len());
        }

        for triangle in triangles {
            let Some(&global) = mapping.get(triangle) else {
                return Err(MeshError::Triangulation(
                    "earcut returned an out-of-range index".to_owned(),
                ));
            };
            indices.push(global as u32);
        }
    }

    if indices.is_empty() {
        return Err(MeshError::NoContours);
    }

    Ok(Triangulated {
        vertices,
        indices,
        ring_offsets,
    })
}

fn flatten(segments: &[Segment], tolerance: f64) -> Vec<Vec<Point>> {
    let tolerance = tolerance.max(1e-6);
    let mut contours = Vec::new();
    let mut contour: Vec<Point> = Vec::new();
    let mut current = [0.0, 0.0];
    let mut start = [0.0, 0.0];

    for segment in segments {
        match *segment {
            Segment::MoveTo(point) => {
                finish(&mut contours, &mut contour);
                contour.push(point);
                current = point;
                start = point;
            }
            Segment::LineTo(point) => {
                if contour.is_empty() {
                    contour.push(current);
                }
                contour.push(point);
                current = point;
            }
            Segment::QuadTo { ctrl, to } => {
                if contour.is_empty() {
                    contour.push(current);
                }
                flatten_quad(current, ctrl, to, tolerance, &mut contour, 0);
                current = to;
            }
            Segment::CubicTo { ctrl1, ctrl2, to } => {
                if contour.is_empty() {
                    contour.push(current);
                }
                flatten_cubic(current, ctrl1, ctrl2, to, tolerance, &mut contour, 0);
                current = to;
            }
            Segment::Close => {
                finish(&mut contours, &mut contour);
                current = start;
            }
        }
    }

    finish(&mut contours, &mut contour);
    contours
}

fn finish(contours: &mut Vec<Vec<Point>>, contour: &mut Vec<Point>) {
    if contour.len() >= 2 && points_close(contour[0], contour[contour.len() - 1]) {
        contour.pop();
    }
    if contour.len() >= 3 {
        contours.push(std::mem::take(contour));
    } else {
        contour.clear();
    }
}

fn flatten_quad(p0: Point, p1: Point, p2: Point, tolerance: f64, out: &mut Vec<Point>, depth: u32) {
    if depth >= 16 || point_line_distance(p1, p0, p2) <= tolerance {
        out.push(p2);
        return;
    }

    let a = midpoint(p0, p1);
    let b = midpoint(p1, p2);
    let middle = midpoint(a, b);
    flatten_quad(p0, a, middle, tolerance, out, depth + 1);
    flatten_quad(middle, b, p2, tolerance, out, depth + 1);
}

fn flatten_cubic(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tolerance: f64,
    out: &mut Vec<Point>,
    depth: u32,
) {
    let flatness = point_line_distance(p1, p0, p3).max(point_line_distance(p2, p0, p3));
    if depth >= 16 || flatness <= tolerance {
        out.push(p3);
        return;
    }

    let a = midpoint(p0, p1);
    let b = midpoint(p1, p2);
    let c = midpoint(p2, p3);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let middle = midpoint(d, e);
    flatten_cubic(p0, a, d, middle, tolerance, out, depth + 1);
    flatten_cubic(middle, e, c, p3, tolerance, out, depth + 1);
}

fn midpoint(a: Point, b: Point) -> Point {
    [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])]
}

fn points_close(a: Point, b: Point) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

fn point_line_distance(point: Point, start: Point, end: Point) -> f64 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-12 {
        let px = point[0] - start[0];
        let py = point[1] - start[1];
        return (px * px + py * py).sqrt();
    }
    ((point[0] - start[0]) * dy - (point[1] - start[1]) * dx).abs() / length
}

fn signed_area(points: &[Point]) -> f64 {
    let mut sum = 0.0;
    let mut previous = points[points.len() - 1];
    for &current in points {
        sum += previous[0] * current[1] - current[0] * previous[1];
        previous = current;
    }
    sum * 0.5
}

fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        if (current[1] > point[1]) != (previous[1] > point[1]) {
            let crossing = (previous[0] - current[0]) * (point[1] - current[1])
                / (previous[1] - current[1])
                + current[0];
            if point[0] < crossing {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn cross_z(vertices: &[Point], a: u32, b: u32, c: u32) -> f64 {
    let pa = vertices[a as usize];
    let pb = vertices[b as usize];
    let pc = vertices[c as usize];
    (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0])
}

fn triangle_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        [normal[0] / length, normal[1] / length, normal[2] / length]
    }
}

fn bounds_of(vertices: &[Point]) -> [f64; 4] {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in vertices {
        bounds[0] = bounds[0].min(point[0]);
        bounds[1] = bounds[1].min(point[1]);
        bounds[2] = bounds[2].max(point[0]);
        bounds[3] = bounds[3].max(point[1]);
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svgpath::{parse_path, Transform};

    fn segments(data: &str) -> Vec<Segment> {
        parse_path(data, Transform::IDENTITY).expect("valid path")
    }

    fn triangle_area_sum(mesh: &Mesh2D) -> f64 {
        mesh.indices
            .chunks_exact(3)
            .map(|triangle| {
                let a = mesh.vertices[triangle[0] as usize];
                let b = mesh.vertices[triangle[1] as usize];
                let c = mesh.vertices[triangle[2] as usize];
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn triangulates_a_square() {
        let mesh = build_2d(&segments("M0 0 H10 V10 H0 Z"), 0.5).expect("mesh");
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert!((triangle_area_sum(&mesh) - 100.0).abs() < 1e-6);
        assert_eq!(mesh.bounds, [0.0, 0.0, 10.0, 10.0]);
    }

    #[test]
    fn keeps_holes_empty() {
        let mesh = build_2d(&segments("M0 0 H10 V10 H0 Z M2 2 V8 H8 V2 Z"), 0.5).expect("mesh");

        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.indices.len(), 24);
        assert!((triangle_area_sum(&mesh) - 64.0).abs() < 1e-6);
    }

    #[test]
    fn handles_disjoint_outer_contours() {
        let mesh = build_2d(&segments("M0 0 H4 V4 H0 Z M10 0 H14 V4 H10 Z"), 0.5).expect("mesh");
        assert_eq!(mesh.vertices.len(), 8);
        assert!((triangle_area_sum(&mesh) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn extrudes_a_square() {
        let mesh = build_3d(&segments("M0 0 H10 V10 H0 Z"), 0.5, 5.0).expect("mesh");

        assert_eq!(mesh.vertices.len(), 8);
        // Caps: 2 triangles, each emitted front and back. Walls: 4 edges * 2.
        assert_eq!(mesh.indices.len(), 36);
        assert_eq!(mesh.normals.len(), 12);

        let front_faces = mesh.normals.iter().filter(|normal| normal[2] < 0.0).count();
        assert_eq!(front_faces, 2, "{:?}", mesh.normals);
    }

    #[test]
    fn rejects_empty_outlines() {
        assert!(matches!(
            build_2d(&segments("M0 0 L1 1"), 0.5),
            Err(MeshError::NoContours)
        ));
    }

    #[test]
    fn rejects_non_positive_depth() {
        assert!(matches!(
            build_3d(&segments("M0 0 H1 V1 H0 Z"), 0.5, 0.0),
            Err(MeshError::InvalidDepth(_))
        ));
    }

    #[test]
    fn flattens_curves() {
        let mesh = build_2d(&segments("M0 0 Q5 10 10 0 L10 -10 L0 -10 Z"), 0.25).expect("mesh");
        assert!(mesh.vertices.len() > 4);
    }
}
