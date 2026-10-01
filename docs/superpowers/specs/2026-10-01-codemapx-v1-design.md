# codemapx v1 design

Date: 2026-10-01. Status: approved in conversation, awaiting written-spec review.
Background: [docs/design-notes.md](../../design-notes.md) records the pilot this builds on.

## Goal

When an agent finishes a branch, catch the developer up on what changed and how the pieces connect.
codemapx draws a file map: one card per file, links meaning "an edit here made an edit there
necessary", each link with a one-line reason and cited evidence, a diff per file, and a reading
order. It is for branch review, not for watching edits stream in.

v1 is done when:

1. `cargo test` passes on the synthetic sample repo.
2. On a real TS branch chosen by the user (re-mapping pilot branch 3046 is the default candidate),
   running `/codemapx` in the agent session produces a map that passes `codemapx validate`.
3. The user can review that branch in the TUI without first reading the raw diff.
4. `codemapx html` produces a page the user would share.

## Decisions

| Topic | Decision |
|---|---|
| Entry point | Standalone CLI, `codemapx [<worktree>]`. No wsx integration in v1. |
| Annotation | In-session skill: the agent that wrote the branch writes the annotations while it still has context. |
| Split of work | Code writes `facts.json`; the agent writes `annotations.json`, which refers to facts by id and holds no line numbers or diffs. |
| Storage | `~/.local/state/codemapx/<repo>/<branch>/<head-sha>/`. Never inside the worktree. |
| Toolchain | One Rust binary. The Python and Node scripts are removed once their replacements land. |
| Languages | TS/JS (incl. TSX) for outlines and import candidates. Other files get cards and diffs only. |
| Primary view | TUI. HTML is for sharing. |
| Narrow terminals | Collapse Tests and Docs columns below 170 columns; refuse below 100. |
| Test data | A synthetic TS repo checked into `tests/sample/`. Private pilot fixtures stay local and untested. |

## Out of scope for v1

- A wsx key binding (later: a thin wrapper around `codemapx view`).
- Headless annotation via `claude -p`, and transcript parsing.
- Languages other than TS/JS.
- Opening the diff (rather than the file) in an external editor.
- Links through re-exports (`export * from`); no type checker is used.

## Commands

All subcommands take an optional worktree path, defaulting to the current directory.

| Command | Does |
|---|---|
| `codemapx collect [--base <ref>]` | Diffs against the merge-base with `<ref>` (default `origin/main`, then `main`), builds outlines and candidate links, writes `facts.json`, prints the map dir. |
| `codemapx validate` | Merges `facts.json` with `annotations.json` and prints every problem. Exit 0 if none, 1 otherwise. |
| `codemapx` / `codemapx view` | Opens the TUI on the map for the current HEAD. |
| `codemapx html -o <file>` | Writes a self-contained HTML page. |
| `codemapx view --snapshot <path> [<outline-idx>] [--width N]` | Prints one TUI frame as text, for tests. |

## Storage

Maps live in `~/.local/state/codemapx/<repo>/<branch>/<head-sha>/` as `facts.json` and
`annotations.json`. `<repo>` is the basename of the main checkout (the parent of
`git rev-parse --git-common-dir`), so all worktrees of a repo share a namespace. `<branch>` has `/`
replaced by `__`.

- `view` uses the dir for the current HEAD. If there is none, it opens the newest map for the branch
  with a stale banner. If the branch has no map at all, it exits with a message telling the user to
  run `/codemapx` in the agent session.
- `collect` on a new HEAD copies the previous map's `annotations.json` forward when one exists.
  Entries whose ids no longer exist then show up as `validate` problems, so the agent re-annotates
  only what changed.

## Data model

### facts.json (written by `collect` only)

```jsonc
{
  "version": 1,
  "repo": "sample", "branch": "feature/x", "base": "<sha>", "head": "<sha>",
  "commits": [{ "sha": "…", "subject": "…", "issues": ["#12"] }],
  "files": [{
    "path": "src/billing/apply.ts",
    "status": "added",            // added | modified | deleted | renamed
    "old_path": null,             // set for renamed
    "binary": false,
    "add": 120, "del": 0,
    "diff": "@@ -0,0 +1,120 @@ …",
    "outline": [{ "name": "applyChanges", "kind": "function", "start": 40, "end": 88 }]
  }],
  "candidates": [{
    "id": "c3", "from": "src/billing/types.ts", "to": "src/billing/apply.ts",
    "kind": "export-use",          // import | export-use | test-pair
    "evidence": { "path": "src/billing/apply.ts", "line": 52, "quote": "const ins: Insertion[] = …" }
  }],
  "warnings": ["src/broken.ts: parse failed, no outline"]
}
```

- A file's id is its path. That keeps ids stable across HEADs.
- A link points from cause to effect: "an edit in `from` made the edit in `to` necessary". For
  import candidates, `from` is the imported file and `to` the importer.
- Rerunning `collect` on the same HEAD produces byte-identical output.

### annotations.json (written by the agent only)

```jsonc
{
  "version": 1,
  "head": "<sha the annotations were written against>",
  "title": "…", "summary": "…",
  "columns": [{ "name": "Shared contracts", "files": ["src/billing/types.ts"] }],
  "files": {
    "src/billing/apply.ts": { "what": "…", "outline": { "applyChanges": "one line" } }
  },
  "links":   [{ "candidate": "c3", "reason": "…" }],
  "dropped": [{ "candidate": "c5", "why": "type-only import, no behavioural link" }],
  "context": [{ "path": "src/jobs/other-writer.ts", "what": "…" }],
  "context_links": [{
    "from": "src/jobs/other-writer.ts", "to": "src/billing/apply.ts", "reason": "…",
    "evidence": { "path": "src/jobs/other-writer.ts", "line": 31, "quote": "…" }
  }],
  "missing": [{ "id": "enqueue", "name": "Enqueue job", "why": "…", "near": "src/api/route.ts" }],
  "trail": ["src/billing/types.ts", "src/billing/apply.ts"]
}
```

`columns` may also list `context` paths, which places their cards. `missing` cards go in the column
of their `near` file.

### Merge and validation

`merge(facts, annotations) -> Result<Map, Vec<Problem>>` is both the validator and the loader that
the TUI and HTML use, so the renderers can't drift from the checks. It reports every problem, never
just the first. Rules:

1. Every changed file appears in exactly one column and exactly once in `trail`. Every `trail`
   entry is a changed file or a context path.
2. Every candidate id appears in exactly one of `links` or `dropped`, and no unknown candidate id
   appears.
3. Every `files` key is a changed path, and every changed file has a `what`.
4. Outline notes are keyed by names in that file's facts outline. Line numbers come from facts only.
5. Every `context` path exists at HEAD and is not a changed file.
6. Every `context_links` endpoint is a changed file or a context path, and its evidence `quote`
   occurs on that line of the file at HEAD (whitespace-trimmed substring match).
7. Every `missing.near` is a changed file, and `missing.id`s are unique and differ from all paths.
8. `annotations.head != facts.head` is reported as stale, not as invalid: `view` still renders it
   with the banner, while `validate` exits 1 so the skill re-annotates.

Changes from the prototype schema: one card per file (no multi-path cards), columns list their
files instead of each node holding `col`, `ghost`/`ref` become `missing`, and edges gain evidence.

## Collect

**Git.** `collect` shells out to the `git` CLI rather than linking libgit2, so it honours the
user's git config. It runs `merge-base HEAD <base>`, `diff -M --numstat <base>`, and a per-file
`diff -M -U3 <base> -- <path>`. Binary files (numstat `-`) get `binary: true` and no diff. Commits
come from `git log <base>..HEAD`, and issue refs from `#\d+` in subjects plus a leading number in
the branch name.

**Outlines.** tree-sitter TypeScript, TSX and JavaScript grammars are compiled in, so nothing needs
installing. Entries are top-level functions, classes, class methods, and exported
`const`/`let`/`type`/`interface`/`enum` declarations in the file at HEAD. Only entries whose range
overlaps a line added on the branch side are kept. Deleted files, non-TS files and files that fail
to parse get no outline; a parse failure also adds a warning.

**Candidates.** Only between changed files. Each carries one evidence line.

- `import`: a changed file imports another changed file. Relative specifiers resolve with `.ts`,
  `.tsx`, `.js`, `.jsx` and `/index.*`. Aliases resolve through `compilerOptions.paths` and
  `baseUrl` of the nearest `tsconfig.json`. Bare package specifiers and unresolved imports are
  ignored. Evidence is the import line.
- `export-use`: an import where the importer's added lines reference a name that the imported file
  added or changed as an export. It replaces the `import` candidate for that pair. Evidence is the
  first added line using the name.
- `test-pair`: `foo.test.ts`, `foo.spec.ts` or `__tests__/foo.ts` paired with `foo.ts` (any TS/JS
  extension), both changed, linked from source to test. Evidence is the test's import of the source,
  or line 1 if there is none.

Candidates are deduplicated per `(from, to)`, keeping the strongest kind
(export-use > import > test-pair). Ids `c1…cN` are assigned in sorted `(from, to)` order.

## Rendering

Both renderers take the merged `Map`. If merge reports anything other than staleness, they print
the problems and exit 1.

### TUI

It keeps the prototype's layout: map, then came-from / inside-this-file / leads-to, then the diff.
It keeps the keys `← →` (trail), `tab`, `↑ ↓`, `⏎` (follow link), `o`, `d`, `J K`, `q`. The
prototype's `main.rs` is split into `tui/{app, map_pane, link_panes, diff_pane, keys}.rs`, and
`main.rs` keeps only CLI dispatch.

- **Card kinds.** Changed files render as today. Context cards are dashed (`┆`), dimmed, and have no
  diff; selecting one shows its `what` and links. Missing cards read "not built" and show `why`.
- **Evidence.** In came-from and leads-to, each link shows its reason and a dim `path:line` line.
- **Open in editor (`o`).** Suspends the TUI, runs the editor, and restores the TUI afterwards. The
  target depends on focus:
  - diff pane: the top visible numbered line of the selected file
  - inside pane: the selected outline entry's start line
  - came-from / leads-to: the selected link's evidence line

  The command is `$EDITOR +<line> <path>` (default `nvim`). If `CODEMAPX_EDITOR` is set, it is
  used as a template with `{path}` and `{line}` placeholders, e.g. `code -g {path}:{line}`. Paths
  resolve against the worktree passed to `codemapx`, which replaces the prototype's `ROOT` key.
  Deleted files and missing cards flash "nothing to open".
- **Narrow terminals.** Below 170 columns, Tests and Docs collapse to count headers
  (`Tests · 9`); `t` toggles showing them in place of the other columns. Layout is chosen per frame,
  so resizing works live. Below 100 columns, only "terminal too narrow (need 100)" is shown.
  Tests and Docs columns are identified by name (case-insensitive `tests` / `docs`).
- **Stale banner.** When the map's HEAD isn't the worktree's HEAD: `map is N commits behind HEAD —
  run /codemapx in the agent session`. View and html use the newest finished map (annotations
  written for its head) until the agent finishes one for HEAD; validate checks the exact-HEAD map.

### HTML

`codemapx html` embeds `web/filemap.tpl.html` with `include_str!`, fills it with the serialized
`Map` (escaping `</`), and writes one self-contained page. The template JS is updated to the merged
shape and renders context cards, missing cards and evidence as the TUI does. It keeps its connector
lines.

## Skill

`skill/codemapx/SKILL.md` is installed by symlinking it into `~/.claude/skills/`. It is invoked as
`/codemapx` once the branch work is done, and it instructs the agent to:

1. Run `codemapx collect` and read `facts.json`.
2. Write `annotations.json`:
   - columns in cause-then-effect order
   - a `what` per file, written for someone who hasn't seen the code
   - keep or drop every candidate, with a reason
   - context cards for unchanged files it read that shaped the edits, each linked with a quoted line
   - `missing` cards for things referenced but not built
   - a trail in reading order
3. Run `codemapx validate` and fix every problem without editing `facts.json`.
4. End with "Map ready: run `codemapx`".

It includes a worked example based on the synthetic sample repo.

## Errors

- Not a git repo, or no base found: `collect` exits 2 with one line naming the problem and `--base`.
- No changes vs base: `collect` exits 2 with "no changes vs base".
- `validate` prints `<path or id>: <message>`, one per line, then `N problem(s)`.
- `view` with no map: exits 1 with the `/codemapx` hint.
- A tree-sitter parse failure is a warning, not an error.

## Testing

TDD throughout.

- **Sample repo.** `tests/sample/` contains a made-up TS project of about 8 files: a `base/` tree
  and a `branch/` tree. A test helper builds a temp git repo with one base commit and one branch
  commit from them. The changes cover a new export used elsewhere, an alias import (`@/…`), a test
  pair, a rename, a deletion, and an unchanged file suitable as a context card.
- **collect.** `facts.json` must match `tests/sample/expected-facts.json` byte for byte. Unit tests
  cover import resolution, export-use detection, test pairing and outline overlap.
- **merge/validate.** `tests/sample/annotations.json` is valid. One test per rule breaks exactly one
  thing and asserts the expected problem.
- **TUI.** `--snapshot` frames are compared to golden text at 180 columns and at 120 columns
  (collapsed). Plus a test of the `o` target in each pane.
- **HTML.** Smoke test: the output contains the serialized map, and `</` is escaped.

## Module layout

```
src/
  main.rs              CLI dispatch
  git.rs               git CLI wrapper
  collect/{mod, outline, imports, candidates}.rs
  facts.rs             facts.json types
  annotations.rs       annotations.json types
  map.rs               merge + validation rules -> Map
  store.rs             state-dir paths, HEAD lookup, carry-forward
  tui/{app, map_pane, link_panes, diff_pane, keys}.rs
  html.rs
skill/codemapx/SKILL.md
web/filemap.tpl.html
tests/sample/…
```
