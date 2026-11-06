//! Color roles shared by the project preset, editor, and design context.
mod image;
mod suggestion;
#[cfg(test)]
mod tests;

pub use image::{ImageColors, extract_colors};
pub use suggestion::{ColorEdit, PaletteProposal, ReviewedEdits, review_edits};

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_SWATCHES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::try_from(text.trim().to_owned())
    }

    pub fn contrast(self, other: Self) -> f32 {
        let luminance = |color: Self| {
            let [r, g, b] = color.0.map(|channel| {
                let v = f32::from(channel) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            r * 0.2126 + g * 0.7152 + b * 0.0722
        };
        let a = luminance(self);
        let b = luminance(other);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let hex = text.strip_prefix('#').unwrap_or(&text);
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("请输入六位 HEX 色值，例如 #2780D9".into());
        }
        let channel = |i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        Ok(Self([channel(0), channel(2), channel(4)]))
    }
}

impl From<HexColor> for String {
    fn from(color: HexColor) -> Self {
        color.to_string()
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02X}{g:02X}{b:02X}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorRole {
    Primary,
    Secondary,
    Accent,
    Background,
    Text,
    Neutral,
}

impl ColorRole {
    pub const ALL: [Self; 6] = [
        Self::Primary,
        Self::Secondary,
        Self::Accent,
        Self::Background,
        Self::Text,
        Self::Neutral,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "主色",
            Self::Secondary => "辅助色",
            Self::Accent => "强调色",
            Self::Background => "背景",
            Self::Text => "文字",
            Self::Neutral => "中性色",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Accent => "accent",
            Self::Background => "background",
            Self::Text => "text",
            Self::Neutral => "neutral",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swatch {
    pub hex: HexColor,
    pub role: ColorRole,
    pub locked: bool,
}

impl Swatch {
    pub fn new(hex: HexColor, role: ColorRole) -> Self {
        Self {
            hex,
            role,
            locked: false,
        }
    }

    pub fn label(self) -> String {
        format!(
            "{} {}{}",
            self.role.label(),
            self.hex,
            if self.locked { "（锁定）" } else { "" }
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Palette(pub Vec<Swatch>);

impl Palette {
    pub fn validate(&self) -> Result<(), String> {
        if self.0.len() > MAX_SWATCHES {
            return Err(format!("配色最多包含 {MAX_SWATCHES} 个色块"));
        }
        Ok(())
    }

    pub fn first(&self, role: ColorRole) -> Option<HexColor> {
        self.0
            .iter()
            .find(|swatch| swatch.role == role)
            .map(|swatch| swatch.hex)
    }

    /// Reuse the artifact's theme when possible. A native widget's style is literal HEX.
    pub fn design_context(&self, html: bool) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let mut context = format!(
            "\n当前已确认的项目配色（以这里为准，优先于旧会话中的配色记录）：{}。主色用于主要按钮和关键操作，辅助色用于次要元素，强调色用于焦点/高亮，背景色用于画布/页面表面，文字色用于正文；注意文字和按钮标签与各自背景的可读性。同一用途按列表顺序优先使用第一个。锁定表示项目预设中的颜色和用途必须保留。用户本轮针对页面或组件明确指定的颜色仅覆盖对应范围，不改变项目配色。只设计用户指定的范围。\n",
            serde_json::to_string(self).expect("palette serializes")
        );
        if html {
            let mut variables = String::new();
            for role in ColorRole::ALL {
                if let Some(hex) = self.first(role) {
                    variables.push_str(&format!("--rpshop-{}:{hex};", role.token()));
                }
            }
            context.push_str(&format!("HTML 优先复用页面已有的语义 CSS 变量；没有对应变量时，在主题中定义 :root{{{variables}}} 并通过 var() 引用。局部覆盖使用目标页面或组件的作用域。\n"));
        } else {
            context.push_str("原生样式按用途填写实际 HEX 值：页面 background、元件 fill / stroke / text_color。\n");
        }
        context
    }
}
