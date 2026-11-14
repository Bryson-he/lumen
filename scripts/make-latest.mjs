// Writes latest.json, the small file Lumen reads to learn about a new version.
// Usage: node scripts/make-latest.mjs <version> "<notes>"
// Run after `pnpm tauri build` (with the signing key set), from the repository root.
import fs from "node:fs";

const [version, notes = ""] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error("give the version, like 0.1.1");
const folder = "target/release/bundle/nsis";
const installer = `Lumen_${version}_x64-setup.exe`;
const signature = fs.readFileSync(`${folder}/${installer}.sig`, "utf8").trim();
const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `https://github.com/Bryson-he/lumen/releases/download/v${version}/${installer}`,
    },
  },
};
fs.writeFileSync(`${folder}/latest.json`, JSON.stringify(latest, null, 2) + "\n");
console.log(`wrote ${folder}/latest.json for ${installer}`);
