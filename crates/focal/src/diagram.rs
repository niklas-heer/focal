//! Diagrams in the languages Markdown files use, drawn to SVG in Focal's
//! palette: Mermaid by `merman`, natively, with no web view and no
//! JavaScript; the others as each language allows (see [`render`]).

use focal_core::blocks::DiagramLanguage;
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

/// Draws `source`, written in `language`, as SVG in `palette`, or says why
/// it cannot.
pub fn render(
    language: DiagramLanguage,
    source: &str,
    palette: &Palette,
) -> Result<String, String> {
    // Renderers written for other uses may panic on input they do not expect.
    let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match language {
        DiagramLanguage::Mermaid => mermaid(source, palette),
        DiagramLanguage::Graphviz => graphviz(source),
        DiagramLanguage::Svgbob => Ok(svgbob::to_svg(source)),
        DiagramLanguage::Pikchr => {
            pikchr::Pikchr::render(source, None, pikchr::PikchrFlags::default())
                .map(|drawn| drawn.rendered().to_owned())
        }
        DiagramLanguage::WaveDrom => {
            let mut svg = Vec::new();
            wavedrom::render_json5(source, &mut svg)
                .map_err(|error| format!("{error:?}"))
                .map(|()| String::from_utf8_lossy(&svg).into_owned())
        }
        DiagramLanguage::GeoJson => crate::maps::geojson(source, palette),
        DiagramLanguage::TopoJson => crate::maps::topojson(source, palette),
        other => Err(format!("{} diagrams are not drawn yet.", other.name())),
    }))
    .unwrap_or_else(|_| Err(format!("{} could not draw this.", language.name())))?;
    if matches!(
        language,
        DiagramLanguage::Mermaid | DiagramLanguage::GeoJson | DiagramLanguage::TopoJson
    ) {
        // Drawn in Focal's palette already.
        Ok(drawn)
    } else {
        Ok(recolor(&with_view_box(&drawn), palette))
    }
}

/// Whether Focal can draw `language` on this Mac; a block it cannot draw
/// stays a code block.
pub const fn can_draw(language: DiagramLanguage) -> bool {
    match language {
        DiagramLanguage::Mermaid
        | DiagramLanguage::Graphviz
        | DiagramLanguage::Svgbob
        | DiagramLanguage::Pikchr
        | DiagramLanguage::WaveDrom
        | DiagramLanguage::GeoJson
        | DiagramLanguage::TopoJson => true,
        DiagramLanguage::PlantUml | DiagramLanguage::D2 => false,
    }
}

/// Graphviz: the `dot` tool when it is installed, which reads all of DOT,
/// otherwise `layout-rs`, which reads most of it.
fn graphviz(source: &str) -> Result<String, String> {
    if let Some(dot) = crate::tools::find("dot") {
        let args = ["-Tsvg", "-Gbgcolor=transparent"];
        return crate::tools::run(&dot, &args, source, crate::tools::TIMEOUT);
    }
    let graph = layout::gv::DotParser::new(source).process()?;
    let mut builder = layout::gv::GraphBuilder::new();
    builder.visit_graph(&graph);
    let mut visual = builder.get();
    let mut svg = layout::backends::svg::SVGWriter::new();
    visual.do_it(false, false, false, &mut svg);
    Ok(svg.finalize())
}

/// Gives an SVG with only a width and height a `viewBox`, so it scales.
fn with_view_box(svg: &str) -> String {
    let Some(tag) = root(svg) else {
        return svg.to_owned();
    };
    if attribute(tag, "viewBox").is_some() {
        return svg.to_owned();
    }
    let number =
        |name| attribute(tag, name).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
    let (Some(width), Some(height)) = (number("width"), number("height")) else {
        return svg.to_owned();
    };
    let at = svg.find("<svg").map_or(0, |at| at + 4);
    format!(
        "{} viewBox=\"0 0 {width} {height}\"{}",
        &svg[..at],
        &svg[at..]
    )
}

/// A diagram drawn black on white, in `palette` instead: black becomes the
/// text color, white the page, text without a color the text color, and
/// fonts Focal's.
fn recolor(svg: &str, palette: &Palette) -> String {
    const BLACK: &[&str] = &[
        "#000000ff",
        "#000000",
        "rgb(0,0,0)",
        "rgb(0, 0, 0)",
        "black",
    ];
    const WHITE: &[&str] = &[
        "#ffffffff",
        "#ffffff",
        "rgb(255,255,255)",
        "rgb(255, 255, 255)",
        "white",
    ];
    let mut out = svg.to_owned();
    for (names, color) in [(BLACK, &palette.text), (WHITE, &palette.canvas)] {
        for name in names {
            out = replace_ignoring_case(&out, name, color);
        }
    }
    // Short hex forms, only as whole attribute or property values.
    for (short, color) in [("#000", &palette.text), ("#fff", &palette.canvas)] {
        for (before, after) in [("\"", "\""), (":", ";"), (":", "\""), (": ", ";")] {
            out = replace_ignoring_case(
                &out,
                &format!("{before}{short}{after}"),
                &format!("{before}{color}{after}"),
            );
        }
    }
    let monospace = out.contains("monospace") || out.contains("Mono");
    let font = if monospace {
        "iA Writer Mono S, Menlo, monospace"
    } else {
        "Helvetica Neue, Helvetica, Arial, sans-serif"
    };
    // Fonts named in class rules outrank the rule below; name ours there.
    for serif in ["Times New Roman", "Times, serif", "Times"] {
        out = out.replace(serif, font);
    }
    let style = format!(
        "<style>text{{fill:{};font-family:{font};}}</style>",
        palette.text
    );
    match out
        .find("<svg")
        .and_then(|at| out[at..].find('>').map(|end| at + end + 1))
    {
        Some(at) => format!("{}{style}{}", &out[..at], &out[at..]),
        None => out,
    }
}

fn replace_ignoring_case(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let from = from.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (found, _) in lower.match_indices(&from) {
        out.push_str(&text[at..found]);
        out.push_str(to);
        at = found + from.len();
    }
    out.push_str(&text[at..]);
    out
}

fn mermaid(source: &str, palette: &Palette) -> Result<String, String> {
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
        let svg = render(
            DiagramLanguage::Mermaid,
            "pie\n  \"a\" : 2\n  \"b\" : 1\n",
            &palette(),
        )
        .unwrap();
        assert!(
            svg.to_lowercase().contains("#4a7fd6"),
            "the first series color is used"
        );
    }

    #[test]
    fn a_flowchart_renders_in_the_palette() {
        let svg = render(
            DiagramLanguage::Mermaid,
            "flowchart TD\n  A[Start] --> B[End]\n",
            &palette(),
        )
        .unwrap();
        assert!(svg.starts_with("<svg"), "{}", &svg[..80.min(svg.len())]);
        assert!(svg.contains("#e6e6e6"), "the text color is used");
        let (width, height) = svg_size(&svg).unwrap();
        assert!(width > 10. && height > 10.);
    }

    #[test]
    fn a_broken_diagram_says_why() {
        let error = render(
            DiagramLanguage::Mermaid,
            "flowchart TD\n  A --> \n",
            &palette(),
        )
        .unwrap_err();
        assert!(error.to_lowercase().contains("parse"), "{error}");
        assert!(render(DiagramLanguage::Mermaid, "just text", &palette()).is_err());
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

    #[test]
    fn recoloring_replaces_black_and_white_and_colors_text() {
        let svg = r##"<svg viewBox="0 0 10 10"><style>.a { stroke: black; fill: white; }</style><rect stroke="#000000ff" fill="#FFFFFF"/><path style="fill:rgb(0,0,0);stroke:#000"/><text>x</text></svg>"##;
        let out = recolor(svg, &palette()).to_lowercase();
        for gone in [
            "black",
            "white",
            "#000000",
            "#ffffff",
            "rgb(0,0,0)",
            "#000\"",
        ] {
            assert!(!out.contains(gone), "{gone} in {out}");
        }
        assert!(out.contains(&palette().text.to_lowercase()), "{out}");
        assert!(
            out.contains("text{fill:"),
            "text gets the text color: {out}"
        );
    }

    #[test]
    fn graphviz_svgbob_and_pikchr_draw() {
        for (language, source) in [
            (
                DiagramLanguage::Graphviz,
                "digraph { rankdir=LR; a -> b -> c; }",
            ),
            (
                DiagramLanguage::Svgbob,
                "+---+    +---+\n| a |--->| b |\n+---+    +---+\n",
            ),
            (
                DiagramLanguage::Pikchr,
                "box \"Hello\"; arrow; circle \"World\"",
            ),
        ] {
            assert!(can_draw(language), "{language:?}");
            let svg = render(language, source, &palette())
                .unwrap_or_else(|e| panic!("{language:?}: {e}"));
            assert!(svg_size(&svg).is_some(), "{language:?} has a size: {svg}");
        }
    }

    #[test]
    fn broken_graphviz_and_pikchr_say_why() {
        assert!(render(DiagramLanguage::Graphviz, "digraph { a -> ", &palette()).is_err());
        assert!(render(DiagramLanguage::Pikchr, "box \"unclosed", &palette()).is_err());
    }

    #[test]
    fn serif_fonts_become_focals() {
        let svg =
            r#"<svg viewBox="0 0 1 1"><style>.a14 { font-family: Times, serif; }</style></svg>"#;
        assert!(!recolor(svg, &palette()).contains("Times"));
    }

    #[test]
    fn wavedrom_and_maps_draw() {
        for (language, source) in [
            (
                DiagramLanguage::WaveDrom,
                r#"{ signal: [ { name: "clk", wave: "p...." }, { name: "data", wave: "x.=.x", data: ["a"] } ] }"#,
            ),
            (
                DiagramLanguage::GeoJson,
                r#"{"type":"Point","coordinates":[1,2]}"#,
            ),
            (
                DiagramLanguage::TopoJson,
                r#"{"type":"Topology","objects":{"a":{"type":"LineString","arcs":[0]}},"arcs":[[[0,0],[1,1]]]}"#,
            ),
        ] {
            assert!(can_draw(language), "{language:?}");
            let svg = render(language, source, &palette())
                .unwrap_or_else(|e| panic!("{language:?}: {e}"));
            assert!(svg_size(&svg).is_some(), "{language:?}: {svg}");
        }
        assert!(render(DiagramLanguage::WaveDrom, "{ signal: [", &palette()).is_err());
    }
}
