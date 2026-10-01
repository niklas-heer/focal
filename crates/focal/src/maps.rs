//! GeoJSON and TopoJSON blocks (GitHub draws them as maps): their outlines
//! projected onto a plane and drawn as SVG, with no map tiles and no network.

use std::fmt::Write as _;

use serde_json::Value;

use crate::diagram::Palette;

/// Points drawn at most; larger shapes are thinned to this.
const POINT_BUDGET: usize = 20_000;

/// A GeoJSON document as an outline map.
pub fn geojson(source: &str, palette: &Palette) -> Result<String, String> {
    let value: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
    let mut shapes = Vec::new();
    geo_shapes(&value, &mut shapes);
    draw(&shapes, palette)
}

/// A TopoJSON topology as an outline map.
pub fn topojson(source: &str, palette: &Palette) -> Result<String, String> {
    let value: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
    let topology = Topology::read(&value).ok_or("This is not a TopoJSON topology.")?;
    let mut shapes = Vec::new();
    for object in value["objects"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
    {
        topology.shapes(object, &mut shapes);
    }
    draw(&shapes, palette)
}

type Point = [f64; 2];

/// Something to draw: an area's rings (outer ring and holes), a line, or a
/// point.
enum Shape {
    Area(Vec<Vec<Point>>),
    Line(Vec<Point>),
    Point(Point),
}

fn point(value: &Value) -> Option<Point> {
    Some([value.get(0)?.as_f64()?, value.get(1)?.as_f64()?])
}

fn points(value: &Value) -> Vec<Point> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(point)
        .collect()
}

fn list(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn geo_shapes(value: &Value, shapes: &mut Vec<Shape>) {
    let coordinates = &value["coordinates"];
    match value["type"].as_str().unwrap_or_default() {
        "FeatureCollection" => list(&value["features"]).for_each(|f| geo_shapes(f, shapes)),
        "Feature" => geo_shapes(&value["geometry"], shapes),
        "GeometryCollection" => list(&value["geometries"]).for_each(|g| geo_shapes(g, shapes)),
        "Point" => shapes.extend(point(coordinates).map(Shape::Point)),
        "MultiPoint" => shapes.extend(points(coordinates).into_iter().map(Shape::Point)),
        "LineString" => shapes.push(Shape::Line(points(coordinates))),
        "MultiLineString" => shapes.extend(list(coordinates).map(|l| Shape::Line(points(l)))),
        "Polygon" => shapes.push(Shape::Area(list(coordinates).map(points).collect())),
        "MultiPolygon" => shapes.extend(
            list(coordinates).map(|polygon| Shape::Area(list(polygon).map(points).collect())),
        ),
        _ => {}
    }
}

/// A TopoJSON topology's arcs, decoded, and how to place quantized points.
struct Topology {
    arcs: Vec<Vec<Point>>,
    scale: Point,
    translate: Point,
}

impl Topology {
    fn read(value: &Value) -> Option<Self> {
        let transform = &value["transform"];
        let quantized = transform.is_object();
        let scale = point(&transform["scale"]).unwrap_or([1.0, 1.0]);
        let translate = point(&transform["translate"]).unwrap_or([0.0, 0.0]);
        let arcs = value["arcs"]
            .as_array()?
            .iter()
            .map(|arc| {
                let mut at = [0.0, 0.0];
                points(arc)
                    .into_iter()
                    .map(|p| {
                        if quantized {
                            // Quantized arcs store each position as a step from the last.
                            at = [at[0] + p[0], at[1] + p[1]];
                            [
                                at[0] * scale[0] + translate[0],
                                at[1] * scale[1] + translate[1],
                            ]
                        } else {
                            p
                        }
                    })
                    .collect()
            })
            .collect();
        Some(Self {
            arcs,
            scale: if quantized { scale } else { [1.0, 1.0] },
            translate: if quantized { translate } else { [0.0, 0.0] },
        })
    }

    /// The points of a line or ring made of the arcs `indices` (a negative
    /// index is an arc reversed).
    fn join(&self, indices: &Value) -> Vec<Point> {
        let mut joined: Vec<Point> = Vec::new();
        for index in list(indices).filter_map(Value::as_i64) {
            let (arc, reversed) = if index < 0 {
                (!index, true)
            } else {
                (index, false)
            };
            let Some(arc) = usize::try_from(arc).ok().and_then(|a| self.arcs.get(a)) else {
                continue;
            };
            let mut arc = arc.clone();
            if reversed {
                arc.reverse();
            }
            // Arcs share their end points; keep one of each.
            let skip = usize::from(!joined.is_empty());
            joined.extend(arc.into_iter().skip(skip));
        }
        joined
    }

    fn place(&self, p: Point) -> Point {
        [
            p[0] * self.scale[0] + self.translate[0],
            p[1] * self.scale[1] + self.translate[1],
        ]
    }

    fn shapes(&self, value: &Value, shapes: &mut Vec<Shape>) {
        let arcs = &value["arcs"];
        match value["type"].as_str().unwrap_or_default() {
            "GeometryCollection" => list(&value["geometries"]).for_each(|g| self.shapes(g, shapes)),
            "Point" => {
                shapes.extend(point(&value["coordinates"]).map(|p| Shape::Point(self.place(p))));
            }
            "MultiPoint" => shapes.extend(
                points(&value["coordinates"])
                    .into_iter()
                    .map(|p| Shape::Point(self.place(p))),
            ),
            "LineString" => shapes.push(Shape::Line(self.join(arcs))),
            "MultiLineString" => shapes.extend(list(arcs).map(|l| Shape::Line(self.join(l)))),
            "Polygon" => shapes.push(Shape::Area(list(arcs).map(|r| self.join(r)).collect())),
            "MultiPolygon" => shapes.extend(
                list(arcs)
                    .map(|polygon| Shape::Area(list(polygon).map(|r| self.join(r)).collect())),
            ),
            _ => {}
        }
    }
}

/// The shapes as SVG, projected so a degree of longitude keeps its width at
/// the map's middle latitude, and thinned to the point budget.
fn draw(shapes: &[Shape], palette: &Palette) -> Result<String, String> {
    let all: Vec<Point> = shapes
        .iter()
        .flat_map(|shape| match shape {
            Shape::Area(rings) => rings.iter().flatten().copied().collect::<Vec<_>>(),
            Shape::Line(line) => line.clone(),
            Shape::Point(p) => vec![*p],
        })
        .collect();
    if all.is_empty() {
        return Err("There is nothing to draw.".into());
    }
    let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &all {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    let middle = f64::midpoint(min[1], max[1]).to_radians();
    let squeeze = middle.cos().clamp(0.1, 1.0);
    let span = [
        ((max[0] - min[0]) * squeeze).max(1e-9),
        (max[1] - min[1]).max(1e-9),
    ];
    let scale = (600.0 / span[0]).min(400.0 / span[1]);
    let pad = 10.0;
    let width = span[0] * scale + 2.0 * pad;
    let height = span[1] * scale + 2.0 * pad;
    let project = |p: &Point| {
        [
            (p[0] - min[0]) * squeeze * scale + pad,
            (max[1] - p[1]) * scale + pad,
        ]
    };
    let step = all.len().div_ceil(POINT_BUDGET).max(1);
    let path = |points: &[Point], close: bool| {
        let mut d = String::new();
        let last = points.len().saturating_sub(1);
        for (ix, p) in points.iter().enumerate() {
            if ix % step != 0 && ix != last {
                continue;
            }
            let [x, y] = project(p);
            let _ = write!(d, "{}{x:.1} {y:.1} ", if d.is_empty() { 'M' } else { 'L' });
        }
        if close && !d.is_empty() {
            d.push('Z');
        }
        d
    };
    let accent = palette.series.first().unwrap_or(&palette.text);
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.1} {height:.1}">"#
    );
    for shape in shapes {
        match shape {
            Shape::Area(rings) => {
                let d: String = rings.iter().map(|ring| path(ring, true)).collect();
                let _ = write!(
                    svg,
                    r#"<path d="{d}" fill="{}" fill-rule="evenodd" stroke="{}" stroke-width="1"/>"#,
                    palette.surface, palette.line
                );
            }
            Shape::Line(line) => {
                let _ = write!(
                    svg,
                    r#"<path d="{}" fill="none" stroke="{accent}" stroke-width="2"/>"#,
                    path(line, false)
                );
            }
            Shape::Point(p) => {
                let [x, y] = project(p);
                let _ = write!(
                    svg,
                    r#"<circle cx="{x:.1}" cy="{y:.1}" r="4" fill="{accent}"/>"#
                );
            }
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
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

    fn points_in(svg: &str) -> usize {
        svg.matches(['L', 'M']).count()
    }

    #[test]
    fn a_geojson_feature_collection_draws_its_shapes_and_points() {
        let source = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[5,5]}}]}"#;
        let svg = geojson(source, &palette()).unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("viewBox"), "{svg}");
        assert!(svg.contains("<path") && svg.contains("<circle"), "{svg}");
    }

    #[test]
    fn a_bare_geometry_is_enough() {
        let svg = geojson(
            r#"{"type":"LineString","coordinates":[[0,0],[1,1],[2,0]]}"#,
            &palette(),
        )
        .unwrap();
        assert!(svg.contains("<path"), "{svg}");
    }

    #[test]
    fn a_topojson_topology_decodes_its_arcs() {
        let source = r#"{"type":"Topology",
            "transform":{"scale":[1,1],"translate":[0,0]},
            "objects":{"shape":{"type":"GeometryCollection","geometries":[{"type":"Polygon","arcs":[[0]]}]}},
            "arcs":[[[0,0],[10,0],[0,10],[-10,0],[0,-10]]]}"#;
        let svg = topojson(source, &palette()).unwrap();
        assert!(svg.contains("<path"), "{svg}");
        assert_eq!(points_in(&svg), 5, "{svg}");
    }

    #[test]
    fn huge_shapes_are_thinned() {
        let ring: Vec<String> = (0..100_000)
            .map(|i| {
                let a = f64::from(i) / 100_000.0 * std::f64::consts::TAU;
                format!("[{},{}]", a.cos(), a.sin())
            })
            .collect();
        let source = format!(
            r#"{{"type":"Polygon","coordinates":[[{}]]}}"#,
            ring.join(",")
        );
        let svg = geojson(&source, &palette()).unwrap();
        assert!(
            points_in(&svg) <= POINT_BUDGET + 10,
            "{} points",
            points_in(&svg)
        );
    }

    #[test]
    fn broken_json_says_why() {
        assert!(geojson("{ not json", &palette()).is_err());
        assert!(topojson(r#"{"type":"Topology"}"#, &palette()).is_err());
    }
}
