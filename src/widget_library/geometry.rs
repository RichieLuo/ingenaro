pub(super) fn paths(contours: &[Vec<[f32; 2]>], closed: &[bool]) -> serde_json::Value {
    serde_json::Value::Array(contours.iter().zip(closed).map(|(c,closed)| serde_json::Value::Array(c.iter().enumerate().map(|(i,p)|serde_json::json!({"Path":{"X":p[0],"Y":p[1]},"HigherSegmentExists":*closed||i+1<c.len()})).collect())).collect())
}

pub(super) fn regular(n: usize, inner: Option<f32>) -> Vec<[f32; 2]> {
    let count = if inner.is_some() { n * 2 } else { n };
    (0..count)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / count as f32 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 1 {
                inner.unwrap_or(0.5)
            } else {
                0.5
            };
            [0.5 + a.cos() * r, 0.5 + a.sin() * r]
        })
        .collect()
}

pub(super) fn shape(name: &str) -> serde_json::Value {
    let rect = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let arc = |cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32| {
        (0..=24)
            .map(|i| {
                let a = start + (end - start) * i as f32 / 24.0;
                [cx + rx * a.cos(), cy + ry * a.sin()]
            })
            .collect::<Vec<_>>()
    };
    let pi = std::f32::consts::PI;
    let points = match name {
        "triangle" => vec![[0.5, 0.0], [1.0, 1.0], [0.0, 1.0]],
        "diamond" | "decision" => vec![[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]],
        "pentagon" => regular(5, None),
        "hexagon" => regular(6, None),
        "octagon" => regular(8, None),
        "star" => regular(5, Some(0.22)),
        "arrow" | "left-arrow" | "up-arrow" => {
            let mut p = vec![
                [0.0, 0.25],
                [0.62, 0.25],
                [0.62, 0.0],
                [1.0, 0.5],
                [0.62, 1.0],
                [0.62, 0.75],
                [0.0, 0.75],
            ];
            if name == "left-arrow" {
                for v in &mut p {
                    v[0] = 1.0 - v[0];
                }
            }
            if name == "up-arrow" {
                for v in &mut p {
                    *v = [v[1], 1.0 - v[0]];
                }
            }
            p
        }
        "double-arrow" => vec![
            [0.0, 0.5],
            [0.3, 0.0],
            [0.3, 0.25],
            [0.7, 0.25],
            [0.7, 0.0],
            [1.0, 0.5],
            [0.7, 1.0],
            [0.7, 0.75],
            [0.3, 0.75],
            [0.3, 1.0],
        ],
        "data" => vec![[0.2, 0.0], [1.0, 0.0], [0.8, 1.0], [0.0, 1.0]],
        "manual" => vec![[0.0, 0.2], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        "preparation" => vec![
            [0.2, 0.0],
            [0.8, 0.0],
            [1.0, 0.5],
            [0.8, 1.0],
            [0.2, 1.0],
            [0.0, 0.5],
        ],
        "beveled" => vec![
            [0.1, 0.0],
            [0.9, 0.0],
            [1.0, 0.15],
            [1.0, 0.85],
            [0.9, 1.0],
            [0.1, 1.0],
            [0.0, 0.85],
            [0.0, 0.15],
        ],
        "offpage" => vec![[0.0, 0.0], [1.0, 0.0], [1.0, 0.65], [0.5, 1.0], [0.0, 0.65]],
        "offline" => vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
        "card" => vec![[0.15, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.2]],
        "keying" => vec![[0.0, 0.0], [1.0, 0.0], [0.8, 1.0], [0.2, 1.0]],
        "trapezoid" => vec![[0.2, 0.0], [0.8, 0.0], [1.0, 1.0], [0.0, 1.0]],
        "bracket" => {
            return paths(
                &[vec![[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]],
                &[false],
            );
        }
        "collate" => {
            return paths(
                &[
                    vec![[0.0, 0.0], [1.0, 0.0], [0.5, 0.5]],
                    vec![[0.5, 0.5], [1.0, 1.0], [0.0, 1.0]],
                ],
                &[true, true],
            );
        }
        "delay" => {
            let mut p = vec![[0.0, 0.0]];
            p.extend(arc(0.5, 0.5, 0.5, 0.5, -pi / 2.0, pi / 2.0));
            p.push([0.0, 1.0]);
            p
        }
        "display" => {
            let mut p = vec![[0.0, 0.5], [0.2, 0.0]];
            p.extend(arc(0.75, 0.5, 0.25, 0.5, -pi / 2.0, pi / 2.0));
            p.push([0.2, 1.0]);
            p
        }
        "semicircle" => arc(0.5, 1.0, 0.5, 1.0, pi, 2.0 * pi),
        "file" => {
            return paths(
                &[
                    vec![[0.0, 0.0], [0.75, 0.0], [1.0, 0.25], [1.0, 1.0], [0.0, 1.0]],
                    vec![[0.75, 0.0], [0.75, 0.25], [1.0, 0.25]],
                ],
                &[true, false],
            );
        }
        "ram" => {
            return paths(
                &[
                    rect,
                    vec![[0.12, 0.0], [0.12, 1.0]],
                    vec![[0.0, 0.18], [1.0, 0.18]],
                ],
                &[true, false, false],
            );
        }
        "sort" => {
            return paths(
                &[
                    vec![[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]],
                    vec![[0.0, 0.5], [1.0, 0.5]],
                ],
                &[true, false],
            );
        }
        "or" | "sum" => {
            let circle = arc(0.5, 0.5, 0.5, 0.5, 0.0, 2.0 * pi);
            let lines = if name == "or" {
                vec![
                    vec![[0.15, 0.15], [0.85, 0.85]],
                    vec![[0.85, 0.15], [0.15, 0.85]],
                ]
            } else {
                vec![vec![[0.0, 0.5], [1.0, 0.5]], vec![[0.5, 0.0], [0.5, 1.0]]]
            };
            return paths(
                &[circle, lines[0].clone(), lines[1].clone()],
                &[false, false, false],
            );
        }
        "tape" => {
            return paths(
                &[
                    arc(0.45, 0.45, 0.45, 0.45, 0.0, 2.0 * pi),
                    vec![[0.45, 0.9], [1.0, 0.9]],
                ],
                &[false, false],
            );
        }
        "actor" => {
            return paths(
                &[
                    arc(0.5, 0.16, 0.18, 0.14, 0.0, 2.0 * pi),
                    vec![[0.5, 0.3], [0.5, 0.65]],
                    vec![[0.05, 0.4], [0.95, 0.4]],
                    vec![[0.1, 1.0], [0.5, 0.65], [0.9, 1.0]],
                ],
                &[false, false, false, false],
            );
        }
        "punched" => {
            let mut p = Vec::new();
            for i in 0..=20 {
                let x = i as f32 / 20.0;
                p.push([x, 0.1 + (x * 2.0 * pi).sin() * 0.08]);
            }
            for i in 0..=20 {
                let x = 1.0 - i as f32 / 20.0;
                p.push([x, 0.9 + (x * 2.0 * pi).sin() * 0.08]);
            }
            p
        }
        "stack-rect" | "stack-rounded" | "stack-file" | "multidoc" => {
            let mut front = if name == "multidoc" {
                let mut p = vec![[0.0, 0.15], [0.85, 0.15], [0.85, 0.85]];
                for i in 1..=20 {
                    let x = 0.85 - i as f32 / 20.0 * 0.85;
                    p.push([x, 0.85 + (x * 2.0 * pi / 0.85).sin() * 0.08]);
                }
                p
            } else if name == "stack-file" {
                vec![
                    [0.0, 0.15],
                    [0.65, 0.15],
                    [0.85, 0.35],
                    [0.85, 1.0],
                    [0.0, 1.0],
                ]
            } else {
                vec![[0.0, 0.15], [0.85, 0.15], [0.85, 1.0], [0.0, 1.0]]
            };
            if name == "stack-rounded" {
                front = vec![
                    [0.05, 0.15],
                    [0.8, 0.15],
                    [0.85, 0.2],
                    [0.85, 0.95],
                    [0.8, 1.0],
                    [0.05, 1.0],
                    [0.0, 0.95],
                    [0.0, 0.2],
                ];
            }
            return paths(
                &[
                    vec![
                        [0.15, 0.08],
                        [0.15, 0.0],
                        [1.0, 0.0],
                        [1.0, 0.85],
                        [0.93, 0.85],
                    ],
                    vec![
                        [0.07, 0.15],
                        [0.07, 0.08],
                        [0.93, 0.08],
                        [0.93, 0.93],
                        [0.85, 0.93],
                    ],
                    front,
                ],
                &[false, false, true],
            );
        }
        "stored" => {
            let mut p = arc(0.85, 0.5, 0.15, 0.5, -pi / 2.0, pi / 2.0);
            p.extend(arc(0.0, 0.5, 0.15, 0.5, pi / 2.0, -pi / 2.0));
            p
        }
        "document" => {
            let mut p = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 0.85]];
            for i in 1..=20 {
                let x = 1.0 - i as f32 / 20.0;
                p.push([x, 0.85 + (x * std::f32::consts::TAU).sin() * 0.1]);
            }
            p
        }
        "database" => {
            let mut p = Vec::new();
            for i in 0..=20 {
                let a = std::f32::consts::PI + i as f32 * std::f32::consts::PI / 20.0;
                p.push([0.5 + a.cos() * 0.5, 0.13 + a.sin() * 0.13]);
            }
            for i in 0..=20 {
                let a = i as f32 * std::f32::consts::PI / 20.0;
                p.push([0.5 + a.cos() * 0.5, 0.87 + a.sin() * 0.13]);
            }
            return paths(
                &[
                    p,
                    (0..=32)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::TAU / 32.0;
                            [0.5 + a.cos() * 0.5, 0.13 + a.sin() * 0.13]
                        })
                        .collect(),
                ],
                &[true, false],
            );
        }
        "predefined" => {
            return paths(
                &[
                    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    vec![[0.12, 0.0], [0.12, 1.0]],
                    vec![[0.88, 0.0], [0.88, 1.0]],
                ],
                &[true, false, false],
            );
        }
        _ => vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    };
    crate::shape_geometry::polygon(&points)
}
