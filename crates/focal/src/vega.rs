//! Vega and Vega-Lite charts, drawn to SVG by Vega itself running in an
//! embedded `QuickJS` engine (no web view), as math is (see `math.rs`). One
//! worker thread owns the engine, loaded on first use.

use std::sync::OnceLock;
use std::sync::mpsc;

use rquickjs::{CatchResultExt as _, Context, Function, Promise, Runtime};

/// Vega and Vega-Lite, bundled by `scripts/vega`.
const VEGA: &str = include_str!("../../../assets/vega/vega.js");

/// What `QuickJS` lacks and Vega uses: `structuredClone` (for plain data,
/// through JSON), and `setTimeout` with a queue the worker runs after the
/// promise jobs.
const POLYFILLS: &str = r"
globalThis.structuredClone = (v) => (v === undefined ? v : JSON.parse(JSON.stringify(v)));
globalThis.__focalTimers = [];
globalThis.setTimeout = (f, ms, ...args) => { __focalTimers.push(() => f(...args)); return __focalTimers.length; };
globalThis.clearTimeout = () => {};
globalThis.__focalRunTimers = () => { const due = __focalTimers.splice(0); due.forEach((f) => f()); return due.length; };
";

type Reply = mpsc::Sender<Result<String, String>>;

struct Engine {
    _runtime: Runtime,
    context: Context,
}

impl Engine {
    fn new() -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_max_stack_size(8 * 1024 * 1024);
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        context.with(|ctx| {
            ctx.eval::<(), _>(POLYFILLS).map_err(|e| e.to_string())?;
            let measure = Function::new(ctx.clone(), |text: String, size: f64, bold: bool| {
                text_width(&text, size, bold)
            })
            .map_err(|e| e.to_string())?;
            ctx.globals()
                .set("focalTextWidth", measure)
                .map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(VEGA)
                .catch(&ctx)
                .map_err(|e| e.to_string())
        })?;
        Ok(Self {
            _runtime: runtime,
            context,
        })
    }

    fn draw(&self, spec: &str, lite: bool) -> Result<String, String> {
        self.context.with(|ctx| {
            let draw: Function = ctx.globals().get("focalVega").map_err(|e| e.to_string())?;
            let run_timers: Function = ctx
                .globals()
                .get("__focalRunTimers")
                .map_err(|e| e.to_string())?;
            let promise: Promise = draw
                .call((spec, lite))
                .catch(&ctx)
                .map_err(|e| e.to_string())?;
            // Promise jobs, then timers, until the drawing is done.
            for _ in 0..1_000 {
                match promise.finish::<String>() {
                    Err(rquickjs::Error::WouldBlock) => {
                        let ran: usize = run_timers.call(()).map_err(|e| e.to_string())?;
                        if ran == 0 {
                            return Err("Vega stopped before finishing the chart.".into());
                        }
                    }
                    result => return result.catch(&ctx).map_err(|e| e.to_string()),
                }
            }
            Err("Vega took too long".into())
        })
    }
}

/// A font file's bytes and the face's index in it.
type FontData = (Vec<u8>, u32);

/// Helvetica Neue's font data, regular and bold, the font charts are drawn
/// in (see `diagram::recolor`); `None` when this Mac lacks it.
fn fonts() -> &'static [Option<FontData>; 2] {
    static FONTS: OnceLock<[Option<FontData>; 2]> = OnceLock::new();
    FONTS.get_or_init(|| {
        use resvg::usvg::fontdb;
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        [fontdb::Weight::NORMAL, fontdb::Weight::BOLD].map(|weight| {
            let query = fontdb::Query {
                families: &[
                    fontdb::Family::Name("Helvetica Neue"),
                    fontdb::Family::SansSerif,
                ],
                weight,
                ..fontdb::Query::default()
            };
            let id = db.query(&query)?;
            db.with_face_data(id, |data, index| (data.to_vec(), index))
        })
    })
}

/// How wide `text` is at `size` pixels, from the font's own advances; an
/// estimate when the font is missing.
pub fn text_width(text: &str, size: f64, bold: bool) -> f64 {
    let estimate = || text.chars().count() as f64 * size * 0.55;
    let Some((data, index)) = &fonts()[usize::from(bold)] else {
        return estimate();
    };
    let Ok(face) = ttf_parser::Face::parse(data, *index) else {
        return estimate();
    };
    let units = f64::from(face.units_per_em());
    let advance: f64 = text
        .chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .map_or(units * 0.55, f64::from)
        })
        .sum();
    advance * size / units
}

fn worker() -> &'static mpsc::Sender<(String, bool, Reply)> {
    static WORKER: OnceLock<mpsc::Sender<(String, bool, Reply)>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (jobs, queue) = mpsc::channel::<(String, bool, Reply)>();
        let spawned = std::thread::Builder::new()
            .name("focal-vega".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let engine = Engine::new();
                for (spec, lite, reply) in queue {
                    let result = match &engine {
                        Ok(engine) => engine.draw(&spec, lite),
                        Err(error) => Err(error.clone()),
                    };
                    let _ = reply.send(result);
                }
            });
        if let Err(error) = spawned {
            eprintln!("focal: could not start the chart renderer: {error}");
        }
        jobs
    })
}

/// The SVG of a Vega (or, with `lite`, Vega-Lite) spec, or why it cannot be
/// drawn; blocks until it is done, so call it from a background task.
pub fn draw(spec: &str, lite: bool) -> Result<String, String> {
    let (reply, result) = mpsc::channel();
    let _ = worker().send((spec.to_owned(), lite, reply));
    result
        .recv()
        .unwrap_or_else(|_| Err("the chart renderer stopped".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vega_lite_bar_chart_draws() {
        let spec = r#"{"data":{"values":[{"a":"A","b":28},{"a":"B","b":55}]},"mark":"bar","encoding":{"x":{"field":"a","type":"nominal"},"y":{"field":"b","type":"quantitative"}}}"#;
        let svg = draw(spec, true).unwrap();
        assert!(svg.contains("<svg") && svg.contains("<path"), "{svg}");
    }

    #[test]
    fn a_vega_spec_draws_and_a_broken_one_says_why() {
        let spec = r#"{"width":100,"height":50,"marks":[{"type":"rect","encode":{"enter":{"x":{"value":0},"y":{"value":0},"width":{"value":50},"height":{"value":20},"fill":{"value":"steelblue"}}}}]}"#;
        assert!(draw(spec, false).unwrap().contains("<svg"));
        assert!(draw("{ not json", true).is_err());
    }

    #[test]
    fn text_is_measured_from_the_font() {
        let narrow = text_width("iiii", 12., false);
        let wide = text_width("MMMM", 12., false);
        assert!(narrow < wide, "{narrow} < {wide}");
        assert!(
            (text_width("Hello", 24., false) - 2. * text_width("Hello", 12., false)).abs() < 0.01
        );
        assert!(text_width("Hello", 12., true) >= text_width("Hello", 12., false));
    }
}
