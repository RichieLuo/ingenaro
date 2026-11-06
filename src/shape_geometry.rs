//! Shared geometry for authored library shapes. Paths stay editable and scale
//! with the widget; no vendor artwork or raster previews are embedded.
use crate::model::Widget;
use serde_json::{Value, json};
use std::fmt::Write;

pub fn polygon(points: &[[f32; 2]]) -> Value {
    json!([points
        .iter()
        .map(|p| json!({"Path":{"X":p[0],"Y":p[1]},"HigherSegmentExists":true}))
        .collect::<Vec<_>>()])
}

fn anchor(value: &Value, field: &str) -> Option<[f32; 2]> {
    let p = value.get(field)?;
    Some([p.get("X")?.as_f64()? as f32, p.get("Y")?.as_f64()? as f32])
}

/// Exact Bézier SVG commands in widget-local pixel coordinates.
pub fn svg_path(widget: &Widget) -> Option<String> {
    svg_paths(widget).map(|paths| paths.concat())
}

/// One path per contour, preserving the canvas's independent fill and stroke
/// order. Overlapping contours must not cancel through a compound fill rule.
pub fn svg_paths(widget: &Widget) -> Option<Vec<String>> {
    let points = widget.vector_points.as_ref()?;
    crate::vector_edit::validate_points(points).ok()?;
    let mut paths = Vec::new();
    let scale = |p: [f32; 2]| [p[0] * widget.width, p[1] * widget.height];
    for contour in points.as_array()? {
        let contour = contour.as_array()?;
        if contour.len() < 2 {
            continue;
        }
        let mut path = String::new();
        for (i, point) in contour.iter().enumerate() {
            let a = scale(anchor(point, "Path")?);
            if i == 0
                || !contour[(i + contour.len() - 1) % contour.len()]
                    .get("HigherSegmentExists")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            {
                write!(path, "M{} {} ", a[0], a[1]).ok()?;
            }
            if !point
                .get("HigherSegmentExists")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                continue;
            }
            let next = &contour[(i + 1) % contour.len()];
            let b = scale(anchor(next, "Path")?);
            if anchor(point, "HigherHandle").is_some() || anchor(next, "LowerHandle").is_some() {
                let c = scale(anchor(point, "HigherHandle").unwrap_or(anchor(point, "Path")?));
                let d = scale(anchor(next, "LowerHandle").unwrap_or(anchor(next, "Path")?));
                write!(
                    path,
                    "C{} {} {} {} {} {} ",
                    c[0], c[1], d[0], d[1], b[0], b[1]
                )
                .ok()?;
            } else {
                write!(path, "L{} {} ", b[0], b[1]).ok()?;
            }
        }
        if contour.iter().all(|p| {
            p.get("HigherSegmentExists")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        }) {
            path.push_str("Z ");
        }
        if !path.is_empty() {
            paths.push(path);
        }
    }
    (!paths.is_empty()).then_some(paths)
}

/// Bounded flattened contours for the canvas. Existing imported image widgets
/// continue to use their retained image, including complex multi-contour icons.
pub fn contours(widget: &Widget) -> Vec<(Vec<[f32; 2]>, bool)> {
    widget
        .vector_points
        .as_ref()
        .map_or_else(Vec::new, contours_from_points)
}

pub(crate) fn contours_from_points(value: &Value) -> Vec<(Vec<[f32; 2]>, bool)> {
    if crate::vector_edit::validate_points(value).is_err() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for contour in value.as_array().unwrap() {
        let points = contour.as_array().unwrap();
        if points.len() < 2 || points.len() > 256 {
            continue;
        }
        let closed = points.iter().all(|p| {
            p.get("HigherSegmentExists")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        });
        let mut flattened = Vec::new();
        for (i, p) in points.iter().enumerate() {
            let a = anchor(p, "Path").unwrap();
            if i == 0 {
                flattened.push(a);
            }
            if !p
                .get("HigherSegmentExists")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                continue;
            }
            let next = &points[(i + 1) % points.len()];
            let b = anchor(next, "Path").unwrap();
            if anchor(p, "HigherHandle").is_some() || anchor(next, "LowerHandle").is_some() {
                let c = anchor(p, "HigherHandle").unwrap_or(a);
                let d = anchor(next, "LowerHandle").unwrap_or(b);
                for step in 1..=12 {
                    let t = step as f32 / 12.0;
                    let s = 1.0 - t;
                    flattened.push([
                        s * s * s * a[0]
                            + 3.0 * s * s * t * c[0]
                            + 3.0 * s * t * t * d[0]
                            + t * t * t * b[0],
                        s * s * s * a[1]
                            + 3.0 * s * s * t * c[1]
                            + 3.0 * s * t * t * d[1]
                            + t * t * t * b[1],
                    ]);
                }
            } else {
                flattened.push(b);
            }
        }
        if closed {
            flattened.pop();
        }
        result.push((flattened, closed));
    }
    result
}

/// Ear clipping handles concave stars/arrows without filling their cutouts.
pub fn triangles(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    if points.len() < 3 || points.len() > 512 {
        return Vec::new();
    }
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut indices: Vec<_> = (0..points.len()).collect();
    // A closed sampled arc can repeat its first point. Remove duplicates and
    // collinear vertices before clipping, retaining the original mesh indices.
    let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 0.000001;
    loop {
        if indices.len() < 3 {
            return Vec::new();
        }
        let redundant = (0..indices.len()).find(|&i| {
            let a = points[indices[(i + indices.len() - 1) % indices.len()]];
            let b = points[indices[i]];
            let c = points[indices[(i + 1) % indices.len()]];
            near(a, b) || cross(a, b, c).abs() < 0.000001
        });
        if let Some(i) = redundant {
            indices.remove(i);
        } else {
            break;
        }
    }
    let mut result = Vec::new();
    while indices.len() > 2 {
        let Some(i) = (0..indices.len()).find(|&i| {
            let a = indices[(i + indices.len() - 1) % indices.len()];
            let b = indices[i];
            let c = indices[(i + 1) % indices.len()];
            cross(points[a], points[b], points[c]) * sign > 0.000001
                && !indices.iter().any(|&p| {
                    p != a
                        && p != b
                        && p != c
                        && cross(points[a], points[b], points[p]) * sign >= -0.000001
                        && cross(points[b], points[c], points[p]) * sign >= -0.000001
                        && cross(points[c], points[a], points[p]) * sign >= -0.000001
                })
        }) else {
            return Vec::new();
        };
        result.push([
            indices[(i + indices.len() - 1) % indices.len()],
            indices[i],
            indices[(i + 1) % indices.len()],
        ]);
        indices.remove(i);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_contours_keep_separate_paths_and_the_merged_api() {
        let outer = polygon(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let inner = polygon(&[[0.2, 0.2], [0.2, 0.8], [0.8, 0.8], [0.8, 0.2]]);
        let mut widget = Widget::new(1, crate::model::WidgetKind::Rectangle, 0.0, 0.0);
        widget.vector_points = Some(json!([outer[0], inner[0]]));
        let paths = svg_paths(&widget).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.matches('M').count() == 1));
        assert_eq!(svg_path(&widget).unwrap(), paths.concat());
    }

    #[test]
    fn concave_shape_area_is_preserved_and_path_scales() {
        let p = [[0.0, 0.0], [1.0, 0.0], [0.4, 0.4], [1.0, 1.0], [0.0, 1.0]];
        let tris = triangles(&p);
        assert_eq!(tris.len(), 3);
        let area: f32 = tris
            .iter()
            .map(|t| {
                let a = p[t[0]];
                let b = p[t[1]];
                let c = p[t[2]];
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum();
        assert!((area - 0.7).abs() < 0.0001);
        let mut w = Widget::new(1, crate::model::WidgetKind::Rectangle, 0.0, 0.0);
        w.width = 100.0;
        w.height = 50.0;
        w.vector_points = Some(polygon(&p));
        assert!(svg_path(&w).unwrap().contains("L40 20"));
    }
}
