// Generates dist/tokens.css from tokens.json.
// Light is the default; dark applies when Windows is dark (prefers-color-scheme)
// or when <html data-theme="dark"> is set. data-theme="light" forces light.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const tokens = JSON.parse(readFileSync(join(here, "tokens.json"), "utf8"));

const name = (...parts) => "--" + parts.join("-").replace(/\./g, "-").replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
const block = (selector, decls, indent = "") =>
  `${indent}${selector} {\n${decls.map((d) => `${indent}  ${d}`).join("\n")}\n${indent}}`;
const colors = (mode) => Object.entries(tokens.color[mode]).map(([k, v]) => `${name(k)}: ${v};`);
const px = (group) => Object.entries(tokens[group]).map(([k, v]) => `${name(group, k)}: ${v}px;`);

const shared = [
  ...Object.entries(tokens.font).map(([k, v]) => `${name("font", k)}: ${v};`),
  ...px("radius"),
  ...px("space"),
  ...px("size"),
  `${name("motion", "fast")}: ${tokens.motion.fastMs}ms;`,
  `${name("motion", "normal")}: ${tokens.motion.normalMs}ms;`,
  `${name("motion", "theme")}: ${tokens.motion.themeCrossfadeMs}ms;`,
  `${name("motion", "easing")}: ${tokens.motion.easing};`,
];

const css = [
  "/* Generated from tokens.json by build.mjs. Do not edit. */",
  block(":root", ["color-scheme: light;", ...colors("light"), ...shared]),
  `@media (prefers-color-scheme: dark) {\n${block(':root:not([data-theme="light"])', ["color-scheme: dark;", ...colors("dark")], "  ")}\n}`,
  block(':root[data-theme="dark"]', ["color-scheme: dark;", ...colors("dark")]),
  "",
].join("\n\n");

mkdirSync(join(here, "dist"), { recursive: true });
writeFileSync(join(here, "dist", "tokens.css"), css);
console.log(`tokens: wrote dist/tokens.css (${css.length} bytes)`);
