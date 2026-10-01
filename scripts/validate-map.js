#!/usr/bin/env node
// Checks a map config against its diffs: edge/trail ids exist, every changed file is mapped,
// and outline line numbers land on a numbered line of the branch-side diff.
// Usage: scripts/validate-map.js <config.json> <diffs.json>
const fs = require('fs');

const [configPath, diffsPath] = process.argv.slice(2);
const C = JSON.parse(fs.readFileSync(configPath, 'utf8'));
const D = JSON.parse(fs.readFileSync(diffsPath, 'utf8'));
const problems = [];
const ids = new Set(C.NODES.map((n) => n.id));

C.EDGES.forEach((e) => { if (!ids.has(e[0]) || !ids.has(e[1])) problems.push(`unknown edge ${e[0]} -> ${e[1]}`); });
C.TRAIL.forEach((t) => { if (!ids.has(t)) problems.push(`unknown trail id ${t}`); });

const mapped = new Set();
for (const n of C.NODES) {
  for (const p of n.paths || []) {
    mapped.add(p);
    if (!D[p]) problems.push(`${n.id}: no diff for ${p}`);
  }
  for (const [name, line] of n.outline || []) {
    let k = 0, found = false;
    for (const l of D[n.paths[0]].diff.split('\n')) {
      const h = l.match(/^@@ -\d+(?:,\d+)? \+(\d+)/);
      if (h) { k = +h[1]; continue; }
      if (l.startsWith('-')) continue;
      if (k === line) found = true;
      k++;
    }
    if (!found) problems.push(`${n.id}: outline "${name}" line ${line} not in diff`);
  }
}
Object.keys(D).forEach((p) => { if (!mapped.has(p)) problems.push(`unmapped file ${p}`); });

problems.forEach((p) => console.log(p));
console.log(`${problems.length} problem(s)`);
process.exit(problems.length ? 1 : 0);
