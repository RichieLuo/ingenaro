//! Sidebar thumbnails on a shared 24-point grid, independent of canvas styling.

use std::sync::Arc;

use super::*;

pub(super) fn magnifier(painter: &egui::Painter, center: Pos2, ink: Color32) {
    let stroke = Stroke::new(1.35, ink);
    painter.circle_stroke(center - vec2(1.6, 1.6), 4.0, stroke);
    painter.line_segment([center + vec2(1.3, 1.3), center + vec2(4.7, 4.7)], stroke);
}

pub(super) fn paint(ui: &egui::Ui, center: Pos2, item: LibraryItem, ink: Color32) {
    let p = ui.painter();
    let origin = center - vec2(12.0, 12.0);
    let stroke = Stroke::new(1.4, ink);
    let tint = ink.gamma_multiply(0.17);
    let at = |[x, y]: [f32; 2]| origin + vec2(x, y);
    let rect = |a, b| Rect::from_min_max(at(a), at(b));
    let outline = |a, b, radius: f32| p.rect_stroke(rect(a, b), radius, stroke, StrokeKind::Middle);
    let fill = |a, b, radius: f32, color| p.rect_filled(rect(a, b), radius, color);
    let segment = |a, b| p.line_segment([at(a), at(b)], stroke);
    let line = |points: &[[f32; 2]]| {
        p.add(egui::Shape::line(
            points.iter().copied().map(at).collect(),
            stroke,
        ))
    };
    let circle = |c, radius| p.circle_stroke(at(c), radius, stroke);
    let dot = |c, radius| p.circle_filled(at(c), radius, ink);

    match item.id {
        "rectangle" => {
            outline([3.5, 5.5], [20.5, 18.5], 0.5);
        }
        "rounded" => {
            outline([3.5, 5.5], [20.5, 18.5], 4.0);
        }
        "box2" => {
            fill([3.5, 5.5], [20.5, 18.5], 0.5, tint);
            outline([3.5, 5.5], [20.5, 18.5], 0.5);
        }
        "box3" => {
            fill([3.5, 5.5], [20.5, 18.5], 0.5, ink.gamma_multiply(0.58));
        }
        "ellipse" => {
            p.add(egui::epaint::EllipseShape::stroke(
                center,
                vec2(9.0, 6.5),
                stroke,
            ));
        }
        "text" => {
            segment([5.5, 6.0], [18.5, 6.0]);
            segment([12.0, 6.0], [12.0, 18.0]);
            segment([9.0, 18.0], [15.0, 18.0]);
        }
        "heading1" | "heading2" | "heading3" => {
            let number = &item.id["heading".len()..];
            p.text(
                center,
                Align2::CENTER_CENTER,
                format!("H{number}"),
                FontId::proportional(13.0),
                ink,
            );
        }
        "paragraph" => {
            segment([4.0, 6.5], [20.0, 6.5]);
            segment([4.0, 12.0], [20.0, 12.0]);
            segment([4.0, 17.5], [14.5, 17.5]);
        }
        "link" => {
            line(&[
                [11.0, 7.5],
                [14.4, 4.1],
                [17.8, 3.6],
                [20.4, 6.2],
                [19.9, 9.6],
                [16.5, 13.0],
            ]);
            line(&[
                [13.0, 16.5],
                [9.6, 19.9],
                [6.2, 20.4],
                [3.6, 17.8],
                [4.1, 14.4],
                [7.5, 11.0],
            ]);
            segment([8.8, 15.2], [15.2, 8.8]);
        }
        "button" | "outline-button" => {
            if item.id == "button" {
                fill([2.5, 7.0], [21.5, 17.0], 3.0, ink);
                p.line_segment(
                    [at([8.0, 12.0]), at([16.0, 12.0])],
                    Stroke::new(1.4, Color32::WHITE),
                );
            } else {
                outline([2.5, 7.0], [21.5, 17.0], 3.0);
                segment([8.0, 12.0], [16.0, 12.0]);
            }
        }
        "line" => {
            segment([3.0, 12.0], [21.0, 12.0]);
        }
        "vertical-line" => {
            segment([12.0, 3.0], [12.0, 21.0]);
        }
        "image" => {
            outline([3.0, 4.5], [21.0, 19.5], 2.0);
            dot([8.0, 9.0], 1.35);
            line(&[
                [3.5, 17.0],
                [9.0, 13.0],
                [12.0, 15.2],
                [16.0, 10.7],
                [20.5, 15.0],
            ]);
        }
        "placeholder" => {
            outline([3.0, 4.5], [21.0, 19.5], 2.0);
            segment([7.0, 8.0], [17.0, 16.0]);
            segment([17.0, 8.0], [7.0, 16.0]);
        }
        "hotspot" => {
            line(&[[3.5, 9.0], [3.5, 4.5], [8.0, 4.5]]);
            line(&[[16.0, 4.5], [20.5, 4.5], [20.5, 9.0]]);
            line(&[[20.5, 15.0], [20.5, 19.5], [16.0, 19.5]]);
            line(&[[8.0, 19.5], [3.5, 19.5], [3.5, 15.0]]);
            circle([12.0, 12.0], 3.1);
            dot([12.0, 12.0], 0.9);
        }
        "container" => {
            outline([3.0, 4.0], [21.0, 20.0], 2.0);
            segment([3.0, 8.5], [21.0, 8.5]);
            outline([7.0, 11.5], [17.0, 16.5], 0.5);
        }
        "input" => {
            outline([2.5, 6.5], [21.5, 17.5], 2.0);
            segment([6.5, 10.0], [6.5, 14.0]);
            segment([10.0, 12.0], [16.5, 12.0]);
        }
        "textarea" => {
            outline([3.0, 4.0], [21.0, 20.0], 2.0);
            segment([6.5, 8.0], [17.5, 8.0]);
            segment([6.5, 12.0], [13.5, 12.0]);
            segment([15.5, 17.5], [18.5, 14.5]);
        }
        "checkbox" => {
            fill([5.0, 5.0], [19.0, 19.0], 2.5, tint);
            outline([5.0, 5.0], [19.0, 19.0], 2.5);
            line(&[[8.3, 12.0], [11.0, 14.7], [15.8, 9.9]]);
        }
        "radio" => {
            circle([12.0, 12.0], 7.0);
            dot([12.0, 12.0], 3.3);
        }
        "droplist" => {
            outline([2.5, 6.5], [21.5, 17.5], 2.0);
            segment([5.5, 12.0], [10.5, 12.0]);
            line(&[[14.0, 11.0], [16.0, 13.0], [18.0, 11.0]]);
        }
        "listbox" => {
            outline([3.5, 4.0], [20.5, 20.0], 2.0);
            fill([5.0, 10.0], [19.0, 14.0], 1.0, tint);
            segment([7.0, 7.8], [14.0, 7.8]);
            segment([7.0, 12.0], [16.5, 12.0]);
            segment([7.0, 16.2], [12.0, 16.2]);
        }
        "table" => {
            fill([3.0, 4.5], [21.0, 9.5], 1.5, tint);
            outline([3.0, 4.5], [21.0, 19.5], 1.5);
            segment([3.0, 9.5], [21.0, 9.5]);
            segment([3.0, 14.5], [21.0, 14.5]);
            segment([12.0, 4.5], [12.0, 19.5]);
        }
        "horizontal-menu" => {
            outline([2.5, 6.5], [21.5, 17.5], 2.0);
            for x in [6.5, 12.0, 17.5] {
                dot([x, 11.5], 1.05);
            }
            segment([4.8, 14.6], [8.2, 14.6]);
        }
        "vertical-menu" => {
            outline([5.5, 3.0], [18.5, 21.0], 2.0);
            fill([6.5, 4.5], [17.5, 9.0], 1.0, tint);
            for y in [7.0, 12.0, 17.0] {
                segment([9.0, y], [15.0, y]);
            }
        }
        "tree" => {
            outline([3.0, 3.0], [7.5, 7.5], 1.0);
            line(&[[5.25, 7.5], [5.25, 18.5], [11.0, 18.5]]);
            segment([5.25, 12.0], [11.0, 12.0]);
            outline([11.0, 10.0], [20.0, 14.0], 1.0);
            outline([11.0, 16.5], [20.0, 20.5], 1.0);
        }
        "sticky1" | "sticky2" | "sticky3" | "sticky4" => {
            let paper = match item.id {
                "sticky1" => Color32::from_rgb(255, 228, 144),
                "sticky2" => Color32::from_rgb(213, 234, 180),
                "sticky3" => Color32::from_rgb(197, 226, 252),
                _ => Color32::from_rgb(252, 211, 231),
            };
            let corners = [
                [5.0, 4.0],
                [19.0, 4.0],
                [19.0, 15.0],
                [14.0, 20.0],
                [5.0, 20.0],
            ];
            p.add(egui::Shape::convex_polygon(
                corners.into_iter().map(at).collect(),
                paper,
                stroke,
            ));
            line(&[[14.0, 20.0], [14.0, 15.0], [19.0, 15.0]]);
            segment([8.0, 8.0], [16.0, 8.0]);
            segment([8.0, 11.5], [13.0, 11.5]);
        }
        "circle-marker" | "drop-marker" => {
            let y = if item.id == "circle-marker" {
                circle([12.0, 12.0], 8.0);
                12.0
            } else {
                line(&[
                    [12.0, 21.0],
                    [5.4, 13.0],
                    [5.0, 8.8],
                    [7.5, 5.0],
                    [12.0, 3.5],
                    [16.5, 5.0],
                    [19.0, 8.8],
                    [18.6, 13.0],
                    [12.0, 21.0],
                ]);
                10.7
            };
            line(&[[10.3, y - 1.4], [12.2, y - 3.0], [12.2, y + 3.0]]);
        }
        "flow-start" => {
            outline([2.5, 7.0], [21.5, 17.0], 5.0);
            dot([12.0, 12.0], 1.35);
        }
        "flow-connector" => {
            circle([12.0, 12.0], 5.3);
        }
        "flow-process" => {
            outline([3.0, 5.5], [21.0, 18.5], 1.0);
            segment([7.5, 12.0], [16.5, 12.0]);
        }
        "pattern-card" => {
            outline([3.0, 3.5], [21.0, 20.5], 2.5);
            fill([6.0, 6.5], [11.5, 11.5], 1.2, tint);
            outline([6.0, 6.5], [11.5, 11.5], 1.0);
            segment([6.0, 15.0], [18.0, 15.0]);
            segment([6.0, 17.5], [13.5, 17.5]);
        }
        "pattern-profile" => {
            outline([2.5, 5.5], [21.5, 18.5], 2.5);
            circle([8.0, 12.0], 2.8);
            segment([13.0, 10.0], [18.0, 10.0]);
            segment([13.0, 14.0], [16.5, 14.0]);
        }
        "pattern-stat" => {
            outline([3.0, 4.0], [21.0, 20.0], 2.5);
            segment([6.0, 8.0], [11.0, 8.0]);
            line(&[[6.0, 16.5], [10.0, 12.5], [13.0, 14.5], [18.0, 9.5]]);
        }
        "pattern-login" => {
            outline([5.0, 2.5], [19.0, 21.5], 2.0);
            segment([8.0, 7.0], [13.5, 7.0]);
            segment([8.0, 10.5], [16.0, 10.5]);
            segment([8.0, 13.5], [16.0, 13.5]);
            fill([8.0, 16.0], [16.0, 18.5], 1.0, ink);
        }
        "pattern-search" => {
            outline([2.5, 7.0], [21.5, 17.0], 2.0);
            fill([14.0, 7.0], [21.5, 17.0], 2.0, tint);
            segment([14.0, 7.0], [14.0, 17.0]);
            circle([7.8, 11.5], 1.9);
            segment([9.2, 12.9], [10.6, 14.3]);
        }
        "pattern-tabs" => {
            outline([3.0, 5.0], [21.0, 19.0], 2.0);
            segment([3.0, 11.0], [21.0, 11.0]);
            fill([4.0, 9.6], [9.0, 11.6], 0.5, ink);
            segment([6.0, 15.0], [16.5, 15.0]);
        }
        "pattern-pagination" => {
            fill([9.0, 8.5], [15.0, 15.5], 1.0, tint);
            outline([9.0, 8.5], [15.0, 15.5], 1.0);
            line(&[[6.0, 9.0], [3.0, 12.0], [6.0, 15.0]]);
            line(&[[18.0, 9.0], [21.0, 12.0], [18.0, 15.0]]);
        }
        "pattern-breadcrumb" => {
            segment([2.5, 12.0], [7.5, 12.0]);
            line(&[[10.5, 9.0], [13.5, 12.0], [10.5, 15.0]]);
            segment([16.5, 12.0], [21.5, 12.0]);
        }
        "pattern-dialog" => {
            outline([2.5, 4.0], [21.5, 20.0], 2.5);
            segment([6.0, 8.5], [13.0, 8.5]);
            segment([6.0, 11.5], [16.0, 11.5]);
            fill([12.5, 15.0], [18.5, 17.5], 0.8, ink);
            fill([6.0, 15.0], [11.0, 17.5], 0.8, tint);
        }
        "pattern-alert" => {
            outline([2.5, 6.0], [21.5, 18.0], 2.5);
            segment([7.5, 9.5], [7.5, 12.3]);
            dot([7.5, 14.8], 0.8);
            segment([12.0, 10.5], [18.0, 10.5]);
            segment([12.0, 13.5], [16.5, 13.5]);
        }
        "pattern-empty" => {
            line(&[
                [4.0, 17.5],
                [4.0, 8.0],
                [7.0, 5.0],
                [17.0, 5.0],
                [20.0, 8.0],
                [20.0, 17.5],
                [4.0, 17.5],
            ]);
            line(&[
                [4.0, 11.5],
                [9.0, 11.5],
                [10.5, 13.5],
                [13.5, 13.5],
                [15.0, 11.5],
                [20.0, 11.5],
            ]);
        }
        "pattern-progress" => {
            segment([3.0, 7.5], [10.0, 7.5]);
            outline([3.0, 11.5], [21.0, 16.5], 2.5);
            fill([3.0, 11.5], [13.0, 16.5], 2.5, ink);
        }
        _ => draw_contours(ui, item, center, stroke),
    }
}

fn draw_contours(ui: &egui::Ui, item: LibraryItem, center: Pos2, stroke: Stroke) {
    // Shared by the gallery and the canvas, with no temporary project or widget.
    let paths = ui.data_mut(|data| {
        Arc::clone(
            data.get_temp_mut_or_insert_with(egui::Id::new(("library-outline", item.id)), || {
                Arc::new(item.preview_contours())
            }),
        )
    });
    debug_assert!(
        !paths.is_empty(),
        "Library thumbnail missing for {}",
        item.id
    );
    let size = match item.id {
        _ if item.category == Category::Icons => vec2(24.0, 24.0),
        "arrow-up" | "flow-actor" => vec2(13.0, 19.0),
        "flow-bracket" => vec2(9.0, 17.0),
        "triangle" | "pentagon" | "hexagon" | "octagon" | "star" | "flow-sum" | "flow-or" => {
            vec2(18.0, 18.0)
        }
        _ => vec2(18.0, 14.0),
    };
    let min = center - size / 2.0;
    for (points, closed) in paths.iter() {
        let points = points
            .iter()
            .map(|[x, y]| min + vec2(x * size.x, y * size.y))
            .collect();
        let shape = if *closed {
            egui::Shape::closed_line(points, stroke)
        } else {
            egui::Shape::line(points, stroke)
        };
        ui.painter().add(shape);
    }
}
