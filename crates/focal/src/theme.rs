//! Focal's palettes, following the system's light or dark appearance.

use crate::settings::{ColumnWidth, ProseFont, Settings, TextSize};
use focal_core::analysis::Alert;
use gpui_kit::{Hsla, WindowAppearance, rgb, rgba};

pub const PROSE_FONT: &str = "iA Writer Quattro S";
pub const MONO_FONT: &str = "iA Writer Mono S";
/// Bold prose. The static Quattro S Bold files report weight 400 in their
/// OS/2 table, so GPUI cannot select them by weight; Duo S Bold reports 700.
pub const BOLD_PROSE_FONT: &str = "iA Writer Duo S";
/// The text size, column width and prose typeface chosen in the settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub size: f32,
    /// The text column's widest width, in points.
    pub column: f32,
    pub prose: &'static str,
}

impl Typography {
    pub const fn new(settings: &Settings) -> Self {
        let size = match settings.text_size {
            TextSize::Small => 16.,
            TextSize::Medium => 18.,
            TextSize::Large => 21.,
            TextSize::Huge => 24.,
        };
        // In ems of the text size: about 60, 70 or 85 characters.
        let column = match settings.column_width {
            ColumnWidth::Narrow => 34.,
            ColumnWidth::Medium => 40.,
            ColumnWidth::Wide => 48.,
        };
        let prose = match settings.prose_font {
            ProseFont::Quattro => PROSE_FONT,
            ProseFont::Duo => BOLD_PROSE_FONT,
            ProseFont::Mono => MONO_FONT,
        };
        Self {
            size,
            column: column * size,
            prose,
        }
    }
}

/// The opacity of text that focus mode dims.
pub const DIMMED: f32 = 0.3;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Hsla,
    pub text: Hsla,
    /// Revealed Markdown syntax and other quiet text.
    pub marker: Hsla,
    pub link: Hsla,
    pub code_background: Hsla,
    /// Inline code. GPUI paints run backgrounds at full line height, which
    /// would join into bars across lines, so inline code is tinted instead.
    pub code_text: Hsla,
    pub highlight: Hsla,
    pub selection: Hsla,
    /// Matches of the find bar's query.
    pub found: Hsla,
    pub caret: Hsla,
    pub rule: Hsla,
    pub quote_bar: Hsla,
    pub banner: Hsla,
    pub misspelled: Hsla,
    pub grammar: Hsla,
    pub checkbox: Hsla,
    alerts: [Hsla; 5],
}

impl Theme {
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
        }
    }

    fn light() -> Self {
        Self {
            background: rgb(0x00f7_f6f3).into(),
            text: rgb(0x0021_2121).into(),
            marker: rgba(0x2121_2166).into(),
            link: rgb(0x0026_6fb5).into(),
            code_background: rgba(0x0000_000a).into(),
            code_text: rgb(0x0086_4a1c).into(),
            highlight: rgba(0xffd8_4a66).into(),
            selection: rgba(0x3d8a_ff40).into(),
            found: rgba(0xf5a6_2366).into(),
            caret: rgb(0x0019_a0e6).into(),
            rule: rgba(0x2121_2130).into(),
            quote_bar: rgba(0x2121_2130).into(),
            banner: rgb(0x00ec_e9e2).into(),
            misspelled: rgb(0x00e0_3b30).into(),
            grammar: rgb(0x0028_9c4a).into(),
            checkbox: rgb(0x0026_6fb5).into(),
            alerts: [
                rgb(0x0009_69da).into(),
                rgb(0x001a_7f37).into(),
                rgb(0x0082_50df).into(),
                rgb(0x009a_6700).into(),
                rgb(0x00cf_222e).into(),
            ],
        }
    }

    fn dark() -> Self {
        Self {
            background: rgb(0x0017_1716).into(),
            text: rgb(0x00d9_d7d2).into(),
            marker: rgba(0xd9d7_d25c).into(),
            link: rgb(0x0068_a9e8).into(),
            code_background: rgba(0xffff_ff0d).into(),
            code_text: rgb(0x00d9_a066).into(),
            highlight: rgba(0xc79a_1a66).into(),
            selection: rgba(0x3d8a_ff4d).into(),
            found: rgba(0xd98e_2a59).into(),
            caret: rgb(0x0019_a0e6).into(),
            rule: rgba(0xd9d7_d230).into(),
            quote_bar: rgba(0xd9d7_d238).into(),
            banner: rgb(0x0026_2624).into(),
            misspelled: rgb(0x00ff_5f57).into(),
            grammar: rgb(0x0045_c463).into(),
            checkbox: rgb(0x0068_a9e8).into(),
            alerts: [
                rgb(0x0047_8be6).into(),
                rgb(0x0034_a853).into(),
                rgb(0x008f_6ee6).into(),
                rgb(0x00c6_9026).into(),
                rgb(0x00e5_534b).into(),
            ],
        }
    }

    /// The colors for text that focus mode dims.
    /// Focus mode's colors around the words kept bright: syntax markers,
    /// bullets, quote bars and rules dim, the words do not.
    pub fn focused(self) -> Self {
        Self {
            marker: self.marker.opacity(DIMMED),
            checkbox: self.checkbox.opacity(DIMMED),
            quote_bar: self.quote_bar.opacity(DIMMED),
            rule: self.rule.opacity(DIMMED),
            ..self
        }
    }

    pub fn faded(self) -> Self {
        Self {
            text: self.text.opacity(DIMMED),
            marker: self.marker.opacity(DIMMED),
            checkbox: self.checkbox.opacity(DIMMED),
            quote_bar: self.quote_bar.opacity(DIMMED),
            ..self
        }
    }

    pub const fn alert(&self, alert: Alert) -> Hsla {
        match alert {
            Alert::Note => self.alerts[0],
            Alert::Tip => self.alerts[1],
            Alert::Important => self.alerts[2],
            Alert::Warning => self.alerts[3],
            Alert::Caution => self.alerts[4],
        }
    }
}

/// A typeface file Focal carries.
pub struct Font {
    pub file: &'static str,
    pub family: &'static str,
    pub bold: bool,
    pub italic: bool,
    pub data: &'static [u8],
}

/// The iA Writer typefaces, loaded at launch and given to printed pages.
pub const FONTS: [Font; 9] = [
    Font {
        file: "iAWriterQuattroS-Regular.ttf",
        family: "iA Writer Quattro S",
        bold: false,
        italic: false,
        data: include_bytes!("../../../assets/fonts/iAWriterQuattroS-Regular.ttf"),
    },
    Font {
        file: "iAWriterQuattroS-Italic.ttf",
        family: "iA Writer Quattro S",
        bold: false,
        italic: true,
        data: include_bytes!("../../../assets/fonts/iAWriterQuattroS-Italic.ttf"),
    },
    Font {
        file: "iAWriterDuoS-Regular.ttf",
        family: "iA Writer Duo S",
        bold: false,
        italic: false,
        data: include_bytes!("../../../assets/fonts/iAWriterDuoS-Regular.ttf"),
    },
    Font {
        file: "iAWriterDuoS-Italic.ttf",
        family: "iA Writer Duo S",
        bold: false,
        italic: true,
        data: include_bytes!("../../../assets/fonts/iAWriterDuoS-Italic.ttf"),
    },
    Font {
        file: "iAWriterDuoS-Bold.ttf",
        family: "iA Writer Duo S",
        bold: true,
        italic: false,
        data: include_bytes!("../../../assets/fonts/iAWriterDuoS-Bold.ttf"),
    },
    Font {
        file: "iAWriterDuoS-BoldItalic.ttf",
        family: "iA Writer Duo S",
        bold: true,
        italic: true,
        data: include_bytes!("../../../assets/fonts/iAWriterDuoS-BoldItalic.ttf"),
    },
    Font {
        file: "iAWriterMonoS-Regular.ttf",
        family: "iA Writer Mono S",
        bold: false,
        italic: false,
        data: include_bytes!("../../../assets/fonts/iAWriterMonoS-Regular.ttf"),
    },
    Font {
        file: "iAWriterMonoS-Bold.ttf",
        family: "iA Writer Mono S",
        bold: true,
        italic: false,
        data: include_bytes!("../../../assets/fonts/iAWriterMonoS-Bold.ttf"),
    },
    Font {
        file: "iAWriterMonoS-Italic.ttf",
        family: "iA Writer Mono S",
        bold: false,
        italic: true,
        data: include_bytes!("../../../assets/fonts/iAWriterMonoS-Italic.ttf"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_mode_dims_syntax_but_not_words() {
        let theme = Theme::light();
        let focused = theme.focused();
        assert!(focused.marker.a < theme.marker.a);
        assert!(focused.quote_bar.a < theme.quote_bar.a);
        assert_eq!(focused.text, theme.text);
    }
}
