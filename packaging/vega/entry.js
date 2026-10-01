// Vega 6.4.0 and Vega-Lite 6.4.3 (BSD-3-Clause, see LICENSE beside this file), bundled by scripts/vega.
import * as vega from 'vega';
import {compile} from 'vega-lite';

// Text widths come from the real font when Focal provides a measurer
// (focalTextWidth, from Rust), instead of Vega's estimate.
const estimate = vega.textMetrics.width;
vega.textMetrics.width = (item, text) => {
  if (typeof globalThis.focalTextWidth !== 'function') return estimate(item, text);
  const size = item.fontSize ?? 11;
  const weight = item.fontWeight;
  const bold = weight === 'bold' || weight === 'bolder' || Number(weight) >= 600;
  return globalThis.focalTextWidth(String(text ?? ''), size, bold);
};

// Draws a Vega (or, with `lite`, Vega-Lite) spec in JSON to SVG. Without a
// canvas, Vega estimates text widths.
globalThis.focalVega = (spec, lite) => {
  let parsed = JSON.parse(spec);
  if (lite) parsed = compile(parsed).spec;
  const view = new vega.View(vega.parse(parsed), {renderer: 'none'});
  return view.toSVG();
};
