// Vega 6.4.0 and Vega-Lite 6.4.3 (BSD-3-Clause, see LICENSE beside this file), bundled by scripts/vega.
import * as vega from 'vega';
import {compile} from 'vega-lite';

// Draws a Vega (or, with `lite`, Vega-Lite) spec in JSON to SVG. Without a
// canvas, Vega estimates text widths.
globalThis.focalVega = (spec, lite) => {
  let parsed = JSON.parse(spec);
  if (lite) parsed = compile(parsed).spec;
  const view = new vega.View(vega.parse(parsed), {renderer: 'none'});
  return view.toSVG();
};
