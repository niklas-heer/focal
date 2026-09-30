//! Generates the Milestone 0 stress document (about 5,000 lines with 50 tables
//! and 50 code blocks) and times analysis and restyling.
//!
//! ```sh
//! cargo run --release -p focal-core --example stress            # timings
//! cargo run --release -p focal-core --example stress -- out.md  # also write it
//! ```

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use focal_core::{Caret, analyze, line_view};

fn document() -> String {
    let mut text = String::from("---\ntitle: Stress test\n---\n\n# Stress test\n\n");
    for section in 0..50 {
        let _ = write!(
            text,
            "## Section {section}\n\n\
             Some **bold** text, some _emphasis_, `code`, a [link](https://example.com/{section}) \
             and ==highlight== in a paragraph long enough to wrap across the column at least once \
             or twice, so layout has real work to do.\n\n\
             - A bullet with *style*\n- [ ] A task\n- [x] A finished task\n\n\
             > [!NOTE]\n> An alert in section {section}.\n\n\
             | Name | Value | Notes |\n|:-----|------:|:-----:|\n| alpha | {section} | **bold** |\n| beta | 2 | `code` |\n\n\
             ```rust\nfn section_{section}() -> usize {{\n    {section}\n}}\n```\n\n"
        );
        for line in 0..77 {
            let _ = writeln!(
                text,
                "Line {line} of section {section} with *a little* style and a `span`."
            );
        }
        text.push('\n');
    }
    text
}

fn main() {
    let text = document();
    if let Some(path) = std::env::args().nth(1)
        && let Err(error) = std::fs::write(&path, &text)
    {
        eprintln!("stress: writing {path}: {error}");
        return;
    }
    let runs = 20;
    let mut analyze_time = Duration::ZERO;
    let mut restyle_time = Duration::ZERO;
    let mut lines = 0;
    for _ in 0..runs {
        let started = Instant::now();
        let analysis = analyze(&text);
        analyze_time += started.elapsed();
        let middle = text.len() / 2;
        let caret = Caret::new(&analysis, middle..middle, middle);
        let started = Instant::now();
        for line in 0..analysis.line_count() {
            std::hint::black_box(line_view(&analysis, &text, line, Some(&caret)));
        }
        restyle_time += started.elapsed();
        lines = analysis.line_count();
    }
    println!(
        "{lines} lines, {} KB: analyze {:.2} ms, restyle all lines {:.2} ms (mean of {runs})",
        text.len() / 1024,
        analyze_time.as_secs_f64() * 1000. / f64::from(runs),
        restyle_time.as_secs_f64() * 1000. / f64::from(runs),
    );
}
