//! Mermaid diagrams, laid out and drawn to SVG by `merman`, natively: no
//! web view and no JavaScript.

use merman::render::{HeadlessRenderer, HostThemeOutput, HostThemeProfile, HostThemeRoles};

/// The colors a diagram is drawn in, as `#rrggbb`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    pub canvas: String,
    pub surface: String,
    pub text: String,
    pub line: String,
    /// Colors for chart series, such as pie slices.
    pub series: Vec<String>,
}

/// Draws `source` as SVG in `palette`, or says why it cannot.
pub fn render(source: &str, palette: &Palette) -> Result<String, String> {
    let profile = HostThemeProfile::builder()
        .roles(HostThemeRoles {
            canvas: Some(palette.canvas.clone()),
            surface: Some(palette.surface.clone()),
            surface_alt: Some(palette.surface.clone()),
            text: Some(palette.text.clone()),
            border: Some(palette.line.clone()),
            line: Some(palette.line.clone()),
            note_background: Some(palette.surface.clone()),
            note_border: Some(palette.line.clone()),
            note_text: Some(palette.text.clone()),
            ..HostThemeRoles::default()
        })
        .series_palette(palette.series.iter().cloned())
        .output(HostThemeOutput::resvg_safe_editor())
        .build();
    HeadlessRenderer::new()
        .with_host_theme(&profile)
        .with_vendored_text_measurer()
        .with_diagram_id("focal")
        .render_svg_sync(source)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "This is not a Mermaid diagram.".to_owned())
}

/// The root element `<svg …>` of `svg`.
fn root(svg: &str) -> Option<&str> {
    let start = svg.find("<svg")?;
    let end = start + svg[start..].find('>')?;
    Some(&svg[start..end])
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

/// The diagram's natural size, from its `viewBox`.
pub fn svg_size(svg: &str) -> Option<(f32, f32)> {
    let view = attribute(root(svg)?, "viewBox")?;
    let numbers: Vec<f32> = view
        .split([' ', ','])
        .filter(|n| !n.is_empty())
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    match numbers[..] {
        [_, _, width, height] if width > 0. && height > 0. => Some((width, height)),
        _ => None,
    }
}

/// `svg` with its root's size set to `width` × `height`, replacing a
/// relative size such as `width="100%"`.
pub fn with_size(svg: &str, width: f32, height: f32) -> String {
    let Some(start) = svg.find("<svg") else {
        return svg.to_owned();
    };
    let end = svg[start..].find('>').map_or(svg.len(), |end| start + end);
    let mut tag = svg[start + 4..end].to_owned();
    for name in ["width", "height"] {
        if let Some(value) = attribute(&tag, name) {
            let old = format!(" {name}=\"{value}\"");
            tag = tag.replacen(&old, "", 1);
        }
    }
    format!(
        "{}<svg width=\"{width}\" height=\"{height}\"{tag}{}",
        &svg[..start],
        &svg[end..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            canvas: "#1b1b1b".into(),
            surface: "#262626".into(),
            text: "#e6e6e6".into(),
            line: "#9a9a9a".into(),
            series: vec!["#4a7fd6".into(), "#3f9a5a".into(), "#c9952e".into()],
        }
    }

    #[test]
    fn charts_use_the_series_colors() {
        let svg = render("pie\n  \"a\" : 2\n  \"b\" : 1\n", &palette()).unwrap();
        assert!(
            svg.to_lowercase().contains("#4a7fd6"),
            "the first series color is used"
        );
    }

    #[test]
    fn a_flowchart_renders_in_the_palette() {
        let svg = render("flowchart TD\n  A[Start] --> B[End]\n", &palette()).unwrap();
        assert!(svg.starts_with("<svg"), "{}", &svg[..80.min(svg.len())]);
        assert!(svg.contains("#e6e6e6"), "the text color is used");
        let (width, height) = svg_size(&svg).unwrap();
        assert!(width > 10. && height > 10.);
    }

    #[test]
    fn a_broken_diagram_says_why() {
        let error = render("flowchart TD\n  A --> \n", &palette()).unwrap_err();
        assert!(error.to_lowercase().contains("parse"), "{error}");
        assert!(render("just text", &palette()).is_err());
    }

    #[test]
    fn the_size_is_set_on_the_root() {
        let svg = r#"<svg id="d" width="100%" style="max-width: 120px;" viewBox="0 0 120 80" xmlns="http://www.w3.org/2000/svg"><rect width="5" height="5"/></svg>"#;
        assert_eq!(svg_size(svg), Some((120., 80.)));
        let sized = with_size(svg, 240., 160.);
        assert!(
            sized.starts_with(r#"<svg width="240" height="160" id="d""#),
            "{sized}"
        );
        assert!(
            sized.contains(r#"<rect width="5" height="5"/>"#),
            "inner sizes stay"
        );
    }
}
