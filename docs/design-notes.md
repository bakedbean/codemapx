# codemapx design notes

Findings from the pilot (2026-10-01), so the spec starts from what was learned rather than from scratch.

## Problem

When agents write the code, the developer loses touch with the codebase. Chronox (live, edit-by-edit
view of agent edits) was too granular to rebuild a mental model. codemapx targets **branch review**:
after an agent finishes a branch, catch the developer up on what changed and how the pieces connect.

## What was tried

1. **Three "lenses" as Mermaid diagrams** (structure, contracts, outcomes) on ssk-web #4123.
   Rejected: too visually noisy; "my eyes glaze over trying to decipher them."
2. **File map** (#4123, then the larger #3046): one quiet card per file, in columns ordered by
   cause and effect; a link means "an edit here made an edit there necessary". Selecting a file
   lights up what it came from (amber) and leads to (blue), each with a one-line reason, plus the
   file's diff. Prev/Next walks a reading order. This is the direction to keep.
3. **What the larger branch needed** (#3046, 20 files, +2316):
   - **Context cards** (dashed): unchanged files the edits depend on, e.g. other writers of the same
     rows. Much of the new code exists because of them.
   - **Missing pieces** ("not built"): e.g. a new job endpoint nothing enqueues yet.
   - **Outlines** for big files: functions with line ranges and one line each; jump into the diff.
4. **TUI** (this repo's `src/main.rs`): same data, no connector lines. Tracing happens by colour
   cues on the map plus came-from / selected / leads-to panes; `⏎` follows a link, `o` opens
   `$EDITOR` at the line. Preferred over HTML as the primary view; HTML stays for sharing.

`fixtures/` holds both pilots (map config, diffs, rendered HTML). It is gitignored because the
diffs are private ssk-web code; it exists only on the machine that ran the pilot.

## Pipeline

1. **Collect** (deterministic, no model) → `facts.json`
   - files, numstat, per-file diffs vs merge-base (`scripts/collect-diffs.py`)
   - function outlines with line ranges (tree-sitter; never model-generated line numbers)
   - candidate links with evidence: imports between changed files, test → source, new exports used
   - commits, issue refs from commit messages
   - from the agent transcript: edit order (Edit/Write tool calls) and files **read then edited
     after** → candidate context cards
2. **Annotate** (one model call) → map config
   - per-file "what", per-link reason, columns, reading order, context and missing cards
   - may only keep or drop candidate links, or add context links that cite a file read in the
     session
   - `scripts/validate-map.js` rejects unknown ids, unmapped files, and outline lines not in the diff
3. **Render**
   - TUI (primary) and HTML (`scripts/render-html.py`), both reading the same config + diffs
   - cache by HEAD sha; re-annotate only nodes whose files changed

## Sources per map element

| Element | Source |
|---|---|
| files, +/−, diffs, line numbers | git |
| outlines | tree-sitter |
| candidate links | import graph, test pairing |
| reading order | transcript edit order, commits |
| context cards | transcript: read-then-edit |
| reasons, summaries | model, from issue + commits + transcript |
| missing pieces | model + grep for callers |

Claude Code transcripts live at `~/.claude/projects/<cwd-slug>/*.jsonl`; wsx knows each workspace's
worktree path, so it can locate them. Other agents (codex, pi, …) need one adapter per format.

## Open questions

- Entry point: standalone `codemapx <worktree>` vs integration hook in wsx (detail screen key).
- Annotation runner: in-session skill (agent still has context) vs headless `claude -p` from the
  transcript (works after the session ends). Pilot suggests skill first, headless second.
- Map config schema: current fixtures use `NODES/EDGES/TRAIL/COLS` with `[from, to, reason]` edges;
  edges likely need an `evidence` field.
- Narrow terminals: the map needs ~170 columns; collapse tests/docs to counts below that.
- Fixtures for a public/CI test suite need a non-proprietary sample branch.
