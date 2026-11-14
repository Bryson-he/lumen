// Sets Lumen's version in the three places that hold it.
// Usage: node scripts/set-version.mjs 0.1.1   (from the repository root)
import fs from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error("give the version, like 0.1.1");

let cargo = fs.readFileSync("Cargo.toml", "utf8");
const before = cargo;
cargo = cargo.replace(/(\[workspace\.package\]\r?\nversion = ")[^"]+(")/, `$1${version}$2`);
if (cargo === before && !before.includes(`version = "${version}"`)) throw new Error("could not find the version in Cargo.toml");
fs.writeFileSync("Cargo.toml", cargo);

for (const file of ["apps/settings/src-tauri/tauri.conf.json", "apps/settings/package.json"]) {
  const json = JSON.parse(fs.readFileSync(file, "utf8"));
  json.version = version;
  fs.writeFileSync(file, JSON.stringify(json, null, 2) + "\n");
}
console.log(`version is now ${version}`);
