//! ASCII STL blocks (GitHub shows them as 3D models): drawn as a still,
//! shaded picture from an angle above, in Focal's palette.

use std::fmt::Write as _;

use crate::diagram::Palette;

/// Triangles drawn at most; larger models are thinned to this.
const TRIANGLE_BUDGET: usize = 20_000;

/// An ASCII STL model as SVG.
pub fn render(source: &str, palette: &Palette) -> Result<String, String> {
    let triangles = parse(source)?;
    let step = triangles.len().div_ceil(TRIANGLE_BUDGET).max(1);
    // Turned a little and tipped toward the viewer, as a product shot.
    let (yaw, pitch) = (35f32.to_radians(), 30f32.to_radians());
    let turn = |[x, y, z]: Point| {
        let (x, y) = (x * yaw.cos() - y * yaw.sin(), x * yaw.sin() + y * yaw.cos());
        // Screen: x right, y up, depth toward the viewer.
        [
            x,
            z * pitch.cos() - y * pitch.sin(),
            z * pitch.sin() + y * pitch.cos(),
        ]
    };
    let mut faces: Vec<([Point; 3], f32)> = triangles
        .iter()
        .step_by(step)
        .map(|t| {
            let t = t.map(turn);
            let normal = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let length = dot(normal, normal).sqrt().max(f32::EPSILON);
            let light = normalize([0.4, 0.6, 1.0]);
            // Facing the light or away (STL winding is not reliable): lit.
            let shade = (dot(normal, light) / length).abs();
            (t, shade)
        })
        .collect();
    // Painter's order: the farthest first.
    faces.sort_by(|a, b| depth(&a.0).total_cmp(&depth(&b.0)));
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (t, _) in &faces {
        for p in t {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
    }
    let span = [(max[0] - min[0]).max(1e-6), (max[1] - min[1]).max(1e-6)];
    let scale = (480. / span[0]).min(360. / span[1]);
    let pad = 10.;
    let (width, height) = (span[0] * scale + 2. * pad, span[1] * scale + 2. * pad);
    let accent = palette.series.first().unwrap_or(&palette.text);
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.1} {height:.1}">"#
    );
    for (t, shade) in faces {
        let points: Vec<String> = t
            .iter()
            .map(|p| {
                format!(
                    "{:.1},{:.1}",
                    (p[0] - min[0]) * scale + pad,
                    (max[1] - p[1]) * scale + pad
                )
            })
            .collect();
        let fill = crate::diagram::blend(&palette.canvas, accent, 0.25 + 0.75 * shade);
        let _ = write!(
            svg,
            r#"<polygon points="{}" fill="{fill}" stroke="{fill}" stroke-width="0.5"/>"#,
            points.join(" ")
        );
    }
    svg.push_str("</svg>");
    Ok(svg)
}

type Point = [f32; 3];

/// The triangles of an ASCII STL model: every three `vertex` lines.
fn parse(source: &str) -> Result<Vec<[Point; 3]>, String> {
    let mut vertices = Vec::new();
    for line in source.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("vertex") {
            continue;
        }
        let numbers: Vec<f32> = words.filter_map(|w| w.parse().ok()).collect();
        let [x, y, z] = numbers[..] else {
            return Err(format!("This vertex is not three numbers: {}", line.trim()));
        };
        vertices.push([x, y, z]);
    }
    let triangles: Vec<[Point; 3]> = vertices
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]])
        .collect();
    if triangles.is_empty() {
        return Err("This is not an ASCII STL model.".into());
    }
    Ok(triangles)
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: Point) -> Point {
    let length = dot(a, a).sqrt();
    [a[0] / length, a[1] / length, a[2] / length]
}

fn depth(t: &[Point; 3]) -> f32 {
    (t[0][2] + t[1][2] + t[2][2]) / 3.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            canvas: "#ffffff".into(),
            surface: "#eeeeee".into(),
            text: "#222222".into(),
            line: "#888888".into(),
            series: vec!["#4a7fd6".into()],
        }
    }

    fn facet(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> String {
        format!(
            "facet normal 0 0 0\n outer loop\n  vertex {} {} {}\n  vertex {} {} {}\n  vertex {} {} {}\n endloop\nendfacet\n",
            a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]
        )
    }

    fn cube() -> String {
        let p = |x: f32, y: f32, z: f32| [x, y, z];
        let quads = [
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.)],
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
            [p(0., 1., 0.), p(1., 1., 0.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(0., 1., 0.), p(0., 1., 1.), p(0., 0., 1.)],
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
        ];
        let mut stl = String::from("solid cube\n");
        for [a, b, c, d] in quads {
            stl += &facet(a, b, c);
            stl += &facet(a, c, d);
        }
        stl + "endsolid cube\n"
    }

    #[test]
    fn a_cube_draws_shaded_faces() {
        let svg = render(&cube(), &palette()).unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("viewBox"), "{svg}");
        assert_eq!(svg.matches("<polygon").count(), 12, "every triangle");
        let fills: std::collections::HashSet<&str> = svg
            .match_indices("fill=\"")
            .map(|(at, _)| &svg[at + 6..at + 13])
            .collect();
        assert!(fills.len() >= 3, "faces are shaded differently: {fills:?}");
    }

    #[test]
    fn huge_models_are_thinned() {
        let mut stl = String::from("solid big\n");
        for i in 0..30_000 {
            let x = i as f32;
            stl += &facet([x, 0., 0.], [x + 1., 0., 0.], [x, 1., 1.]);
        }
        let svg = render(&stl, &palette()).unwrap();
        assert!(svg.matches("<polygon").count() <= TRIANGLE_BUDGET);
    }

    #[test]
    fn text_that_is_not_stl_says_why() {
        assert!(render("not a model", &palette()).is_err());
        assert!(
            render(
                "solid x\nfacet normal 0 0 0\n outer loop\n  vertex 1 2\n",
                &palette()
            )
            .is_err()
        );
    }
}
