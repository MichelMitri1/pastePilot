#!/usr/bin/env node
// Publishes a new PastePilot version that installed copies can update to.
//
//   npm run release -- 1.2.0 "What changed"
//   npm run release -- 1.2.0 "What changed" --dry-run   (build + sign only, publish nothing)
//
// Steps: bump the version → build the .app and .dmg → sign the update with
// ~/.tauri/pastepilot.key → write latest.json → commit, tag, push → create the
// GitHub release with the files. Installed apps then see the update.
// The pushed tag also starts .github/workflows/windows.yml, which adds the
// Windows installer to the same release about 15 minutes later.

import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "MichelMitri1/pastePilot";
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const KEY = path.join(os.homedir(), ".tauri", "pastepilot.key");
const FILES = {
  pkg: path.join(ROOT, "package.json"),
  conf: path.join(ROOT, "src-tauri", "tauri.conf.json"),
  cargo: path.join(ROOT, "src-tauri", "Cargo.toml"),
  lock: path.join(ROOT, "src-tauri", "Cargo.lock"),
};
const BUNDLE = path.join(ROOT, "src-tauri", "target", "release", "bundle");
const OUT = path.join(ROOT, "release");

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const [version, ...noteParts] = args.filter((a) => a !== "--dry-run");
const notes = noteParts.join(" ").trim() || `PastePilot ${version}`;

const fail = (msg) => {
  console.error(`\n✖ ${msg}\n`);
  process.exit(1);
};
const step = (msg) => console.log(`\n▸ ${msg}`);
const git = (...a) => execFileSync("git", a, { cwd: ROOT, encoding: "utf8" }).trim();
const has = (cmd, a = ["--version"]) => spawnSync(cmd, a, { stdio: "ignore" }).status === 0;
const parse = (v) => v.split(".").map(Number);
const newer = (a, b) => {
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
};

// ----- checks -----
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) fail('Give a version like: npm run release -- 1.2.0 "What changed"');
const current = JSON.parse(fs.readFileSync(FILES.conf, "utf8")).version;
if (!newer(version, current)) fail(`Version must be higher than the current ${current}.`);
if (!fs.existsSync(KEY)) fail(`Signing key not found at ${KEY}. Without it, installed apps won't accept updates.`);
// Code-signing identity: keeps macOS permissions (Accessibility, Keychain, Microphone) across updates.
const identity = JSON.parse(fs.readFileSync(FILES.conf, "utf8")).bundle?.macOS?.signingIdentity;
if (identity && spawnSync("security", ["find-certificate", "-c", identity], { stdio: "ignore" }).status !== 0) {
  fail(`Code-signing identity "${identity}" isn't in your Keychain. Run: npm run setup-signing`);
}
if (!dryRun) {
  if (git("status", "--porcelain")) fail("Commit or stash your changes first, so the release matches what's pushed.");
  if (!has("gh")) fail("Install the GitHub CLI first: brew install gh");
  if (!has("gh", ["auth", "status"])) fail("Log in to GitHub first: gh auth login");
}

// ----- bump -----
step(`Version ${current} → ${version}`);
const originals = Object.fromEntries(Object.entries(FILES).map(([k, f]) => [k, fs.existsSync(f) ? fs.readFileSync(f, "utf8") : null]));
const restore = () => Object.entries(FILES).forEach(([k, f]) => originals[k] !== null && fs.writeFileSync(f, originals[k]));
for (const f of [FILES.pkg, FILES.conf]) {
  const json = JSON.parse(fs.readFileSync(f, "utf8"));
  json.version = version;
  fs.writeFileSync(f, JSON.stringify(json, null, 2) + "\n");
}
fs.writeFileSync(FILES.cargo, originals.cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`));

// ----- build + sign -----
step("Building and signing (takes a minute)…");
const build = spawnSync(
  "npx",
  ["tauri", "build", "--bundles", "app,dmg", "--config", JSON.stringify({ bundle: { createUpdaterArtifacts: true } })],
  {
    cwd: ROOT,
    stdio: "inherit",
    env: {
      ...process.env,
      CI: "true", // skips the Finder window styling of the .dmg, which needs extra permissions
      TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(KEY, "utf8"),
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
    },
  },
);
if (build.status !== 0) {
  restore();
  fail("Build failed, version change undone.");
}

const tarball = path.join(BUNDLE, "macos", "PastePilot.app.tar.gz");
const sig = `${tarball}.sig`;
const dmg = path.join(BUNDLE, "dmg", `PastePilot_${version}_aarch64.dmg`);
for (const f of [tarball, sig, dmg]) if (!fs.existsSync(f)) (restore(), fail(`Missing build output: ${f}`));

fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT);
const assets = {
  tarball: path.join(OUT, "PastePilot.app.tar.gz"),
  dmg: path.join(OUT, "PastePilot.dmg"), // stable name: /releases/latest/download/PastePilot.dmg always works
  manifest: path.join(OUT, "latest.json"),
};
fs.copyFileSync(tarball, assets.tarball);
fs.copyFileSync(dmg, assets.dmg);
fs.writeFileSync(
  assets.manifest,
  JSON.stringify(
    {
      version,
      notes,
      pub_date: new Date().toISOString(),
      platforms: {
        "darwin-aarch64": {
          signature: fs.readFileSync(sig, "utf8").trim(),
          url: `https://github.com/${REPO}/releases/download/v${version}/PastePilot.app.tar.gz`,
        },
      },
    },
    null,
    2,
  ) + "\n",
);
console.log(`\nRelease files are in ${path.relative(ROOT, OUT)}/`);

if (dryRun) {
  restore();
  console.log("\n✔ Dry run done. Nothing was committed or published, and the version change was undone.\n");
  process.exit(0);
}

// ----- publish -----
step("Committing and tagging…");
git("add", FILES.pkg, FILES.conf, FILES.cargo, FILES.lock);
git("commit", "-m", `Release v${version}`);
git("tag", `v${version}`);
git("push");
git("push", "origin", `v${version}`);

step("Creating the GitHub release…");
const gh = spawnSync(
  "gh",
  ["release", "create", `v${version}`, assets.tarball, assets.dmg, assets.manifest, "--repo", REPO, "--title", `v${version}`, "--notes", notes],
  { cwd: ROOT, stdio: "inherit" },
);
if (gh.status !== 0) fail(`The tag v${version} is pushed but the release wasn't created. Re-run the "gh release create" step above.`);

console.log(`\n✔ PastePilot ${version} is live for Mac. Installed apps will offer it under Check for Updates.`);
console.log(`  The Windows build is running on GitHub Actions and joins the release in about 15 minutes:`);
console.log(`  https://github.com/${REPO}/actions/workflows/windows.yml\n`);
