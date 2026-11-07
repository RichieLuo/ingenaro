use super::*;

pub(crate) fn valid_color(value: &str) -> bool {
    matches!(value.len(), 7 | 9)
        && value.starts_with('#')
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn validate_page_tree(tree: &[PageTreeNode], page_ids: &HashSet<u64>) -> Result<()> {
    if tree.is_empty() {
        return Ok(());
    }
    fn visit(
        nodes: &[PageTreeNode],
        page_ids: &HashSet<u64>,
        seen_pages: &mut HashSet<u64>,
        seen_folders: &mut HashSet<String>,
        count: &mut usize,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 64, "页面目录嵌套超过 64 层");
        for node in nodes {
            *count += 1;
            ensure!(*count <= 4096, "页面目录最多包含 4096 个节点");
            match node {
                PageTreeNode::Folder { id, name, .. } => {
                    ensure!(
                        !id.is_empty() && id.len() <= 1024 && seen_folders.insert(id.clone()),
                        "页面文件夹 ID 为空、过长或重复"
                    );
                    ensure!(
                        !name.trim().is_empty() && name.len() <= 1024,
                        "页面文件夹名称不能为空或超过 1024 字节"
                    );
                }
                PageTreeNode::Page { page_id, .. } => {
                    ensure!(
                        page_ids.contains(page_id) && seen_pages.insert(*page_id),
                        "页面目录引用不存在或重复的页面：{page_id}"
                    );
                }
            }
            visit(
                node.children(),
                page_ids,
                seen_pages,
                seen_folders,
                count,
                depth + 1,
            )?;
        }
        Ok(())
    }
    let mut seen_pages = HashSet::new();
    visit(
        tree,
        page_ids,
        &mut seen_pages,
        &mut HashSet::new(),
        &mut 0,
        0,
    )?;
    ensure!(&seen_pages == page_ids, "页面目录没有包含所有页面");
    Ok(())
}

pub(crate) fn validate_page(page: &Page) -> Result<()> {
    ensure!(page.id > 0, "页面 ID 不能为零");
    ensure!(
        !page.name.trim().is_empty() && page.name.len() <= 1024,
        "页面名称不能为空且不能超过 1024 字节"
    );
    ensure!(
        bounded(page.width, 64.0, 20_000.0) && bounded(page.height, 64.0, 20_000.0),
        "页面尺寸应在 64–20000 之间"
    );
    ensure!(
        page.responsive_min_width.is_none_or(|width| {
            bounded(width, 64.0, 20_000.0)
                && (page.canvas != CanvasMode::ResponsiveWidth || width <= page.width)
        }),
        "最小宽度应在 64–20000 之间，且在响应式页面中不能超过设计宽度"
    );
    ensure!(valid_color(&page.background), "页面背景颜色无效");
    ensure!(page.widgets.len() <= 5_000, "单页最多包含 5000 个元件");
    let mut ids = HashSet::new();
    for widget in &page.widgets {
        ensure!(widget.id > 0 && ids.insert(widget.id), "元件 ID 重复或为零");
        ensure!(
            widget.groups.len() <= MAX_GROUP_DEPTH,
            "元件组合不能超过 32 层"
        );
        let mut ancestors = HashSet::new();
        ensure!(
            widget
                .groups
                .iter()
                .all(|group| *group > 0 && ancestors.insert(*group)),
            "组合 ID 不能为零或在同一路径中重复"
        );
        ensure!(
            widget.name.len() <= 1024
                && widget
                    .radio_group
                    .as_ref()
                    .is_none_or(|group| group.len() <= 1024)
                && widget.text.len() <= 100_000
                && widget.interaction.value.len() <= 100_000,
            "元件文本过长"
        );
        ensure!(
            bounded(widget.x, -100_000.0, 100_000.0) && bounded(widget.y, -100_000.0, 100_000.0),
            "元件坐标必须是有限数字"
        );
        ensure!(
            bounded(widget.width, 1.0, 20_000.0) && bounded(widget.height, 1.0, 20_000.0),
            "元件尺寸应在 1–20000 之间"
        );
        let layout = widget.horizontal_layout;
        ensure!(
            bounded(layout.right, 0.0, 100.0) && bounded(layout.left, 0.0, layout.right),
            "水平布局比例应在 0–100 之间，且左边缘比例不能大于右边缘"
        );
        ensure!(bounded(widget.font_size, 4.0, 512.0), "字号应在 4–512 之间");
        if let Some(layout) = &widget.text_layout {
            ensure!(
                layout.horizontal <= 2
                    && layout.vertical <= 2
                    && layout
                        .padding
                        .iter()
                        .all(|value| bounded(*value, 0.0, 20_000.0)),
                "文字对齐或内边距无效"
            );
        }
        ensure!(
            bounded(widget.rotation, -36_000.0, 36_000.0),
            "旋转角度必须为 -36000–36000 之间的有限数字"
        );
        ensure!(
            bounded(widget.radius, 0.0, 20_000.0) && bounded(widget.opacity, 0.0, 1.0),
            "圆角或透明度无效"
        );
        ensure!(
            valid_color(&widget.fill)
                && valid_color(&widget.stroke)
                && valid_color(&widget.text_color),
            "元件颜色应为 #RRGGBB 或 #RRGGBBAA"
        );
        if let Some(data) = &widget.image_data {
            validate_image(data).with_context(|| format!("元件「{}」的图片无效", widget.name))?;
        }
        if let Some(points) = &widget.vector_points {
            crate::vector_edit::validate_points(points)
                .with_context(|| format!("元件「{}」的矢量路径无效", widget.name))?;
        }
    }
    if !page.group_names.is_empty() {
        let groups = page
            .widgets
            .iter()
            .flat_map(|w| &w.groups)
            .collect::<HashSet<_>>();
        for (id, name) in &page.group_names {
            ensure!(groups.contains(id), "当前页面不存在分组 {id}");
            crate::grouping::validate_group_name(name)?;
        }
    }
    Ok(())
}

pub(super) fn bounded(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

pub(super) fn validate_image(data: &str) -> Result<()> {
    ensure!(
        data.len() <= MAX_IMAGE_BYTES * 4 / 3 + 64,
        "图片超过 12 MB 限制"
    );
    let (header, payload) = data.split_once(',').context("图片必须为 base64 data URI")?;
    let expected = match header {
        "data:image/png;base64" => image::ImageFormat::Png,
        "data:image/jpeg;base64" => image::ImageFormat::Jpeg,
        "data:image/gif;base64" => image::ImageFormat::Gif,
        "data:image/webp;base64" => image::ImageFormat::WebP,
        _ => bail!("只支持 PNG、JPEG、GIF 和 WebP 图片"),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .context("图片 base64 无效")?;
    ensure!(bytes.len() <= MAX_IMAGE_BYTES, "图片超过 12 MB 限制");
    ensure!(
        image::guess_format(&bytes).context("无法识别图片格式")? == expected,
        "图片内容与 MIME 类型不匹配"
    );
    let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let (width, height) = reader.into_dimensions().context("无法读取图片尺寸")?;
    ensure!(
        width > 0
            && height > 0
            && width <= 8192
            && height <= 8192
            && u64::from(width) * u64::from(height) <= 32_000_000,
        "图片尺寸过大"
    );
    Ok(())
}
