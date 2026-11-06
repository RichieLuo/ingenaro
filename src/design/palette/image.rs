use super::{ColorRole, HexColor, Palette, Swatch};
use crate::ai::ImageAttachment;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ImageColors {
    pub suggested: Palette,
    /// A prefix of the suggestions was sampled from the images. Any remaining
    /// rows are neutral colors suggested to make the palette usable in a UI.
    pub sampled: usize,
}

pub fn extract_colors(images: &[ImageAttachment]) -> Result<ImageColors, String> {
    let mut colors = Vec::new();
    for image in images {
        for color in sample(image)? {
            if !colors.iter().any(|other| similar(*other, color)) {
                colors.push(color);
            }
        }
    }
    if colors.is_empty() {
        return Err("图片中没有可提取的可见颜色".into());
    }
    // Keep a distinct, colorful candidate even if its area in a photo is small.
    if colors.len() > 6
        && let Some(accent) = accent_index(&colors, colors[0])
        && accent >= 6
    {
        colors.swap(5, accent);
    }
    colors.truncate(6);
    Ok(suggest_roles(colors))
}

#[derive(Default)]
struct Bucket {
    sum: [u32; 3],
    count: u32,
    flat: u32,
}

fn sample(image: &ImageAttachment) -> Result<Vec<HexColor>, String> {
    let rgba = image::load_from_memory(image.png_bytes())
        .map_err(|_| "无法读取参考图片")?
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    let step = width.max(height).div_ceil(224);
    let mut buckets = BTreeMap::<[u8; 3], Bucket>::new();
    for y in (0..height).step_by(step as usize) {
        for x in (0..width).step_by(step as usize) {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            if a < 128 {
                continue;
            }
            let color = [r, g, b];
            let bucket = buckets
                .entry(color.map(|channel| channel / 16))
                .or_default();
            bucket.count += 1;
            for (sum, channel) in bucket.sum.iter_mut().zip(color) {
                *sum += u32::from(channel);
            }
            // Cards have contiguous, flat interiors. Ignore color noise at the
            // edge of a swatch, and weight these interiors when they dominate.
            let uniform = [(step, 0), (0, step)].iter().all(|&(dx, dy)| {
                let neighbor = rgba.get_pixel((x + dx).min(width - 1), (y + dy).min(height - 1));
                neighbor[3] >= 128
                    && color
                        .iter()
                        .zip(neighbor.0)
                        .all(|(&c, n)| c.abs_diff(n) <= 4)
            });
            bucket.flat += u32::from(uniform);
        }
    }
    let count: u32 = buckets.values().map(|b| b.count).sum();
    let mut flats: Vec<_> = buckets.values().map(|b| b.flat).collect();
    flats.sort_unstable_by(|a, b| b.cmp(a));
    let card = flats.iter().take(8).sum::<u32>() * 100 >= count * 70;
    let mut bins: Vec<_> = buckets
        .into_values()
        .filter(|bucket| bucket.count >= (count / 400).max(1))
        .map(|bucket| {
            let hex = HexColor(
                bucket
                    .sum
                    .map(|sum| ((sum + bucket.count / 2) / bucket.count) as u8),
            );
            (
                hex,
                oklab(hex),
                if card {
                    bucket.count + 3 * bucket.flat
                } else {
                    bucket.count
                },
            )
        })
        .collect();
    bins.sort_by_key(|bin| std::cmp::Reverse(bin.2));
    let mut clusters: Vec<(HexColor, [f32; 3], u32)> = Vec::new();
    for (hex, lab, weight) in bins {
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|cluster| distance(cluster.1, lab) < 0.067)
        {
            cluster.2 += weight;
        } else {
            clusters.push((hex, lab, weight));
        }
    }
    clusters.sort_by_key(|cluster| std::cmp::Reverse(cluster.2));
    let mut ranked: Vec<_> = clusters.iter().map(|cluster| cluster.0).collect();
    if let Some(&dominant) = ranked.first()
        && let Some(accent) = accent_index(&ranked, dominant)
        && accent >= 6
    {
        ranked.swap(5, accent);
    }
    ranked.truncate(6);
    Ok(ranked)
}

fn suggest_roles(colors: Vec<HexColor>) -> ImageColors {
    use ColorRole::*;
    let mut roles = vec![None; colors.len()];
    let primary = colors
        .iter()
        .position(|&hex| chroma(oklab(hex)) >= 0.075)
        .unwrap_or(0);
    roles[primary] = Some(Primary);
    let background = colors.iter().enumerate().position(|(i, &hex)| {
        let lab = oklab(hex);
        i != primary && chroma(lab) < 0.065 && (lab[0] > 0.76 || lab[0] < 0.35)
    });
    if let Some(i) = background {
        roles[i] = Some(Background);
    }
    let surface = background.map_or(HexColor([248, 249, 252]), |i| colors[i]);
    let text = colors.iter().enumerate().position(|(i, &hex)| {
        roles[i].is_none() && chroma(oklab(hex)) < 0.075 && hex.contrast(surface) >= 4.5
    });
    if let Some(i) = text {
        roles[i] = Some(Text);
    }
    let remaining = |i: usize| roles[i].is_none();
    if let Some(i) = (0..colors.len())
        .filter(|&i| remaining(i))
        .max_by(|&a, &b| {
            accent_score(colors[a], colors[primary])
                .total_cmp(&accent_score(colors[b], colors[primary]))
        })
        .filter(|&i| chroma(oklab(colors[i])) >= 0.08)
    {
        roles[i] = Some(Accent);
    }
    if let Some(i) = roles.iter().position(Option::is_none) {
        roles[i] = Some(Secondary);
    }
    let mut swatches: Vec<_> = colors
        .into_iter()
        .zip(roles)
        .map(|(hex, role)| {
            Swatch::new(
                hex,
                role.unwrap_or_else(|| {
                    if chroma(oklab(hex)) < 0.075 {
                        Neutral
                    } else {
                        Accent
                    }
                }),
            )
        })
        .collect();
    let sampled = swatches.len();
    if background.is_none() {
        swatches.push(Swatch::new(surface, Background));
    }
    if text.is_none() {
        let dark = HexColor([22, 28, 39]);
        let text = if dark.contrast(surface) >= 4.5 {
            dark
        } else {
            HexColor([255, 255, 255])
        };
        swatches.push(Swatch::new(text, Text));
    }
    ImageColors {
        suggested: Palette(swatches),
        sampled,
    }
}

fn accent_index(colors: &[HexColor], primary: HexColor) -> Option<usize> {
    colors
        .iter()
        .enumerate()
        .filter(|&(_, &hex)| !similar(hex, primary) && chroma(oklab(hex)) >= 0.1)
        .max_by(|&(_, &a), &(_, &b)| accent_score(a, primary).total_cmp(&accent_score(b, primary)))
        .map(|(i, _)| i)
}

fn accent_score(color: HexColor, primary: HexColor) -> f32 {
    let lab = oklab(color);
    chroma(lab) * (0.15 + distance(lab, oklab(primary)))
}

fn chroma([_, a, b]: [f32; 3]) -> f32 {
    a.hypot(b)
}

fn similar(a: HexColor, b: HexColor) -> bool {
    distance(oklab(a), oklab(b)) < 0.067
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

fn oklab(color: HexColor) -> [f32; 3] {
    let [r, g, b] = color.0.map(|c| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}
