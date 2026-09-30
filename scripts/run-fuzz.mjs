#!/usr/bin/env node
// Runs a short cargo-fuzz smoke session. Requires nightly rust + cargo-fuzz;
// prints honest setup instructions when missing rather than pretending.

import { spawnSync } from "node:child_process";

const haveFuzz = spawnSync("cargo", ["fuzz", "--version"], { encoding: "utf8" });
if (haveFuzz.status !== 0) {
  console.error("[vdf] cargo-fuzz is not installed.");
  console.error("    Install: rustup toolchain install nightly && cargo +nightly install cargo-fuzz");
  console.error("    (On distro Rust without rustup, install cargo-fuzz with a nightly cargo.)");
  process.exit(1);
}

const result = spawnSync(
  "cargo",
  ["+nightly", "fuzz", "run", "vdf_core_affine", "--", "-max_total_time=15"],
  { stdio: "inherit", cwd: new URL("..", import.meta.url).pathname },
);
process.exit(result.status ?? 1);
