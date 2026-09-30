#!/usr/bin/env node
// Placeholder for stable test command names that gain real tests in later
// milestones (MASTER_PLAN.md §14): `test:render` → M1, `test:e2e` → M6.
// The command name is reserved now so it never renames; the placeholder is
// honest about there being nothing to run yet.

const kind = process.argv[2];
const when = kind === "render" ? "M1 (PDF viewer)" : "M6 (product UI / E2E)";

console.log(`[vdf] test:${kind}: no ${kind} tests exist yet — scheduled for ${when}.`);
console.log(`[vdf] This placeholder reserves the stable command name (see MASTER_PLAN.md §14) and exits 0.`);
