//! Focal's palettes, following the system's light or dark appearance.

use focal_core::analysis::Alert;
use gpui_kit::{Hsla, WindowAppearance, rgb, rgba};

pub const PROSE_FONT: &str = "iA Writer Quattro S";
pub const MONO_FONT: &str = "iA Writer Mono S";
/// Bold prose. The static Quattro S Bold files report weight 400 in their
/// OS/2 table, so GPUI cannot select them by weight; Duo S Bold reports 700.
pub const BOLD_PROSE_FONT: &str = "iA Writer Duo S";

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
    pub caret: Hsla,
    pub rule: Hsla,
    pub quote_bar: Hsla,
    pub banner: Hsla,
    pub misspelled: Hsla,
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
            caret: rgb(0x0019_a0e6).into(),
            rule: rgba(0x2121_2130).into(),
            quote_bar: rgba(0x2121_2130).into(),
            banner: rgb(0x00ec_e9e2).into(),
            misspelled: rgb(0x00e0_3b30).into(),
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
            caret: rgb(0x0019_a0e6).into(),
            rule: rgba(0xd9d7_d230).into(),
            quote_bar: rgba(0xd9d7_d238).into(),
            banner: rgb(0x0026_2624).into(),
            misspelled: rgb(0x00ff_5f57).into(),
            alerts: [
                rgb(0x0047_8be6).into(),
                rgb(0x0034_a853).into(),
                rgb(0x008f_6ee6).into(),
                rgb(0x00c6_9026).into(),
                rgb(0x00e5_534b).into(),
            ],
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
