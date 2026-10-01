//! Display math, typeset by `MathJax` 3 (TeX in, SVG out) running in an
//! embedded `QuickJS` engine: no web view. One worker thread owns the engine,
//! loaded on first use (about 70 ms); each formula then takes a few
//! milliseconds.

use std::sync::OnceLock;
use std::sync::mpsc;

use rquickjs::{Context, Function, Runtime};

/// `MathJax`'s TeX input and SVG output, bundled by `scripts/mathjax`.
const MATHJAX: &str = include_str!("../../../assets/mathjax/mathjax.js");

/// The height of an "x" in `MathJax`'s TeX fonts, in ems; its SVGs measure in
/// these units.
pub const EX: f32 = 0.442;

/// Rasterize at this multiple of the drawn size, so formulas stay sharp on
/// Retina displays (GPUI rasterizes SVG images at scale 1).
const OVERSAMPLE: f32 = 3.;

type Reply = mpsc::Sender<Result<String, String>>;

struct Engine {
    _runtime: Runtime,
    context: Context,
}

impl Engine {
    fn new() -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_max_stack_size(4 * 1024 * 1024);
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        context.with(|ctx| ctx.eval::<(), _>(MATHJAX).map_err(|e| e.to_string()))?;
        Ok(Self {
            _runtime: runtime,
            context,
        })
    }

    fn typeset(&self, tex: &str) -> Result<String, String> {
        self.context.with(|ctx| {
            let render: Function = ctx.globals().get("focalMath").map_err(|e| e.to_string())?;
            render
                .call::<_, String>((tex, true))
                .map_err(|error| match error {
                    // MathJax throws its own error objects, which carry a message.
                    rquickjs::Error::Exception => ctx
                        .catch()
                        .as_object()
                        .and_then(|object| object.get::<_, String>("message").ok())
                        .unwrap_or_else(|| "MathJax could not typeset this".to_owned()),
                    other => other.to_string(),
                })
        })
    }
}

fn worker() -> &'static mpsc::Sender<(String, Reply)> {
    static WORKER: OnceLock<mpsc::Sender<(String, Reply)>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (jobs, queue) = mpsc::channel::<(String, Reply)>();
        let spawned = std::thread::Builder::new()
            .name("focal-math".into())
            .spawn(move || {
                let engine = Engine::new();
                for (tex, reply) in queue {
                    let result = match &engine {
                        Ok(engine) => engine.typeset(&tex),
                        Err(error) => Err(error.clone()),
                    };
                    let _ = reply.send(result);
                }
            });
        if let Err(error) = spawned {
            eprintln!("focal: could not start the math typesetter: {error}");
        }
        jobs
    })
}

/// The SVG of `tex`, or `MathJax`'s error message, typeset on the worker
/// thread; blocks until it is done, so call it from a background task. The
/// worker never touches GPUI, which keeps UI tests deterministic.
pub fn typeset(tex: &str) -> Result<String, String> {
    let (reply, result) = mpsc::channel();
    let _ = worker().send((tex.to_owned(), reply));
    result
        .recv()
        .unwrap_or_else(|_| Err("the typesetter stopped".into()))
}

/// An SVG ready to draw: in the given color and rasterized larger than its
/// drawn `width` × `height`.
pub struct Sized {
    pub svg: Vec<u8>,
    pub width: f32,
    pub height: f32,
}

/// Colors `MathJax`'s SVG with `color` (`#rrggbb`) and sizes it for text of
/// `font_size`. `MathJax` measures in `ex`, which image renderers do not know.
pub fn sized(svg: &str, color: &str, font_size: f32) -> Option<Sized> {
    let ex = font_size * EX;
    let attribute = |name: &str| -> Option<(std::ops::Range<usize>, f32)> {
        let start = svg.find(&format!(" {name}=\""))? + name.len() + 3;
        let end = start + svg[start..].find('"')?;
        let value = svg[start..end].strip_suffix("ex")?.parse::<f32>().ok()?;
        Some((start..end, value * ex))
    };
    let (width_at, width) = attribute("width")?;
    let (height_at, height) = attribute("height")?;
    let mut out = String::with_capacity(svg.len());
    let (first, second) = if width_at.start < height_at.start {
        ((width_at, width), (height_at, height))
    } else {
        ((height_at, height), (width_at, width))
    };
    out.push_str(&svg[..first.0.start]);
    out.push_str(&(first.1 * OVERSAMPLE).to_string());
    out.push_str(&svg[first.0.end..second.0.start]);
    out.push_str(&(second.1 * OVERSAMPLE).to_string());
    out.push_str(&svg[second.0.end..]);
    Some(Sized {
        svg: out.replace("currentColor", color).into_bytes(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_becomes_svg_and_errors_say_why() {
        let svg = typeset(r"E = mc^2").unwrap();
        assert!(
            svg.starts_with("<svg") && svg.contains("currentColor"),
            "{svg}"
        );
        let error = typeset(r"\frac{").unwrap_err();
        assert!(error.contains("close brace"), "{error}");
    }

    #[test]
    fn svgs_are_colored_and_sized_for_the_text() {
        let svg = r#"<svg style="vertical-align: -0.5ex;" xmlns="http://www.w3.org/2000/svg" width="10ex" height="2ex" viewBox="0 0 100 20"><g fill="currentColor" stroke="currentColor"></g></svg>"#;
        let sized = sized(svg, "#112233", 20.).unwrap();
        let text = String::from_utf8(sized.svg).unwrap();
        assert!(!text.contains("currentColor") && text.contains("#112233"));
        let ex = 20. * EX;
        assert!((sized.width - 10. * ex).abs() < 0.01 && (sized.height - 2. * ex).abs() < 0.01);
        assert!(
            text.contains(&format!(r#"width="{}""#, 10. * ex * 3.)),
            "{text}"
        );
    }
}
