---
title: Focal showcase
tags: [markdown, spike]
---

# Focal, in Rust and GPUI

Focal hides Markdown syntax until the caret reaches it. This is **bold**, this is _emphasis_, this is ***both***, and this is `inline code`. You can ~~strike things out~~, ==highlight== them, or link to [the GPUI site](https://gpui.rs). Math like $e^{i\pi} + 1 = 0$ stays inline, and a footnote sits here[^1].

## Lists

- A plain bullet
- A bullet with a much longer line that wraps across the column so we can see how wrapped list items behave when the text keeps going
  - A nested bullet
1. First ordered item
2. Second ordered item

- [ ] An open task
- [x] A finished task

## Quotes and alerts

> A quiet quote, the way iA Writer draws it.
> It can span several lines.

> [!NOTE]
> GitHub alerts get a colored bar and a title.

> [!WARNING]
> Careful with this one.

## Code

```rust
fn main() {
    println!("Hello from a fenced block");
}
```

## Table

| Feature | Status | Notes |
|:--------|:------:|------:|
| Live rendering | **works** | markers hide |
| Tables | grid | click to edit source |
| Mermaid | later | M5 |

---

### A smaller heading

Wiki links like [[design]] resolve against the folder. That's it.

[^1]: Footnotes render as quiet references.
