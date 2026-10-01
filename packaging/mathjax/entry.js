// MathJax 3.2.2 (Apache License 2.0, see LICENSE beside this file), bundled by scripts/mathjax.
import {mathjax} from 'mathjax-full/js/mathjax.js';
import {TeX} from 'mathjax-full/js/input/tex.js';
import {SVG} from 'mathjax-full/js/output/svg.js';
import {liteAdaptor} from 'mathjax-full/js/adaptors/liteAdaptor.js';
import {RegisterHTMLHandler} from 'mathjax-full/js/handlers/html.js';
import {AllPackages} from 'mathjax-full/js/input/tex/AllPackages.js';

const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);
const packages = AllPackages.filter((name) => !['bussproofs', 'require', 'autoload'].includes(name));
const doc = mathjax.document('', {
  InputJax: new TeX({packages, formatError: (_jax, error) => { throw error; }}),
  OutputJax: new SVG({fontCache: 'none'}),
});
globalThis.focalMath = (tex, display) => adaptor.innerHTML(doc.convert(tex, {display}));
