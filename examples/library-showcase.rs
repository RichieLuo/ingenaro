//! Regenerate the independently authored built-in library gallery.
//! cargo run --example library-showcase -- [output-directory]
use anyhow::Result;
use rpshop::model::{Page, Project, TextLayout, Widget, WidgetKind};
use rpshop::widget_library::{Category, ITEMS};

fn main() -> Result<()> {
    let directory = std::env::args().nth(1).unwrap_or_else(|| "examples".into());
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory)?;
    let mut project = Project::blank();
    project.name = "RPShop 组件画廊".into();
    project.pages.clear();
    for category in Category::ALL {
        let items: Vec<_> = ITEMS
            .iter()
            .filter(|item| item.category == category)
            .collect();
        if items.is_empty() {
            continue;
        }
        let (columns, column_width, min_row_height, min_height) = match category {
            Category::Icons => (6, 205.0, 108.0, 720.0),
            Category::Patterns => (3, 410.0, 90.0, 950.0),
            _ => (3, 410.0, 330.0, 950.0),
        };
        let mut page = Page {
            id: project.allocate_id(),
            name: category.label().into(),
            width: 1280.0,
            height: min_height,
            canvas: Default::default(),
            background: "#FFFFFF".into(),
            widgets: Vec::new(),
            group_names: Default::default(),
        };
        let mut heading = Widget::new(project.allocate_id(), WidgetKind::Text, 40.0, 28.0);
        heading.text = format!("{} · {} 项", category.label(), items.len());
        heading.width = 1100.0;
        heading.height = 56.0;
        heading.font_size = 30.0;
        heading.text_layout = Some(TextLayout {
            horizontal: 0,
            vertical: 1,
            padding: [0.0; 4],
            wrap: false,
            bold: true,
        });
        page.widgets.push(heading);
        let mut y = 118.0;
        for (row_index, row) in items.chunks(columns).enumerate() {
            let mut next_row = y + min_row_height;
            for (column_index, item) in row.iter().enumerate() {
                let x = 40.0 + column_index as f32 * column_width;
                let mut caption = Widget::new(project.allocate_id(), WidgetKind::Text, x, y);
                caption.text = format!(
                    "{:02}  {}",
                    row_index * columns + column_index + 1,
                    item.label
                );
                caption.width = column_width - 40.0;
                caption.height = 28.0;
                caption.font_size = 14.0;
                caption.text_color = if category == Category::Patterns {
                    "#596681"
                } else {
                    "#858A99"
                }
                .into();
                page.widgets.push(caption);
                let widgets = item.create(&mut project, x, y + 46.0);
                if category == Category::Patterns {
                    next_row = widgets.iter().fold(next_row, |bottom, widget| {
                        bottom.max(widget.y + widget.height + 26.0)
                    });
                }
                page.widgets.extend(widgets);
            }
            y = next_row;
        }
        page.height = (y + 22.0).max(min_height);
        project.pages.push(page);
    }
    project.save(&directory.join("library.rpshop"))?;
    std::fs::write(
        directory.join("library.html"),
        rpshop::export::html(&project)?,
    )?;
    for (index, page) in project.pages.iter().enumerate() {
        let name = if index == 0 {
            "library.svg".into()
        } else {
            format!("library-{}.svg", index + 1)
        };
        std::fs::write(directory.join(name), rpshop::export::svg(page)?)?;
    }
    println!(
        "已生成 {} 个分类、{} 项组件的画廊：{}",
        project.pages.len(),
        ITEMS.len(),
        directory.display()
    );
    Ok(())
}
