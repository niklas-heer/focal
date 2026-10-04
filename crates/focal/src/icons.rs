//! Focal's icons: Lucide's outline icons (ISC license, `assets/icons`),
//! built into the binary and drawn by GPUI in the text color around them.

use std::borrow::Cow;

use gpui_kit::{AssetSource, IntoElement, SharedString, Styled, svg};

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// An icon from `assets/icons`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            /// The icon's path among the assets.
            pub const fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("icons/", $file, ".svg"),)*
                }
            }

            const fn data(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!("../../../assets/icons/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    Bold => "bold",
    ChevronDown => "chevron-down",
    Clock => "clock",
    Code => "code",
    Ellipsis => "ellipsis",
    FileText => "file-text",
    Focus => "focus",
    Folder => "folder",
    Hash => "hash",
    Heading => "heading",
    Highlighter => "highlighter",
    Info => "info",
    Italic => "italic",
    Keyboard => "keyboard",
    Link => "link",
    ListChecks => "list-checks",
    ListOrdered => "list-ordered",
    ListTree => "list-tree",
    List => "list",
    Moon => "moon",
    PanelRight => "panel-right",
    PenLine => "pen-line",
    Search => "search",
    Settings => "settings",
    Shapes => "shapes",
    Sigma => "sigma",
    SquareCode => "square-code",
    Strikethrough => "strikethrough",
    SunMoon => "sun-moon",
    Sun => "sun",
    Table => "table",
    TextQuote => "text-quote",
    Type => "type",
    X => "x",
}

impl Icon {
    /// The icon at `size` points, in the text color it is given.
    pub fn element(self, size: f32) -> impl IntoElement + Styled {
        svg()
            .path(SharedString::new_static(self.path()))
            .size(gpui_kit::px(size))
            .flex_none()
    }
}

/// The assets GPUI loads by path: Focal's icons.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.data())))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Icon::ALL
            .iter()
            .filter(|icon| icon.path().starts_with(path))
            .map(|icon| SharedString::new_static(icon.path()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_as_an_svg() {
        for icon in Icon::ALL {
            let data = Assets.load(icon.path()).unwrap().expect("embedded");
            let svg = std::str::from_utf8(&data).unwrap();
            assert!(svg.contains("<svg"), "{icon:?}");
            assert!(
                svg.contains("currentColor"),
                "{icon:?} takes the text color"
            );
        }
        assert!(Assets.load("icons/missing.svg").unwrap().is_none());
    }
}
