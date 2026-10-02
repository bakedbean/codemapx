# codemapx

A navigable map of the code changes an agent made on a branch: which files changed, which edit made the next one necessary, and why. Built for catching up on a branch before review, not for watching edits stream in.

## Install

```sh
cargo install --path .
ln -s "$PWD/skill/codemapx" ~/.claude/skills/codemapx
```

## Use

1. When an agent finishes a branch, run `/codemapx` in its session. It runs `codemapx collect`, writes the annotations and validates them.
2. Run `codemapx` in the worktree to open the map. Use `codemapx html -o map.html` to get a page you can share.

| Command | Does |
|---|---|
| `codemapx collect [--base REF]` | writes facts.json for the branch and prints the map dir |
| `codemapx validate` | checks annotations.json against facts.json |
| `codemapx` / `codemapx view` | opens the TUI |
| `codemapx html -o FILE` | writes a self-contained HTML page |

Maps live in `~/.local/state/codemapx/<repo>/<branch>/<head-sha>/`; set `CODEMAPX_STATE_DIR` to move them.

## Keys

| Key | Action |
|---|---|
| `←` `→` | step through the edits in reading order |
| `tab` | move between panes |
| `↑` `↓` | move within the focused pane (in the map, within the current column) |
| `h` `l` | map pane: move to the next column left or right (reaches context cards, which are off the trail) |
| `⏎` | in a came-from / leads-to pane, jump to that card; in the inside or functions pane, focus the diff |
| `o` | open the editor at the diff line, outline entry, function, or link evidence |
| `d` | full-screen diff |
| `f` | show or hide the functions panel left of the diff: a TS/JS file's top-level functions (including consts holding one), classes and their methods, `+` on those this branch added or deleted lines inside; moving in it jumps the diff to that function, or clears the highlight if the diff doesn't show it |
| `J` `K` | page the diff |
| `t` | expand the Tests/Docs columns, which start collapsed (in narrow terminals this collapses the others) |
| `c` | collapse or expand the column holding the selected card |
| `C` | reset every column to the automatic layout |
| `q` / `esc` | quit (`esc` first closes the full-screen diff) |
| mouse drag | drag the border between two rows, or between the functions panel and the diff, to resize them |
| mouse wheel | move the pane under the pointer: scroll the diff, or ↑/↓ in the map and middle panes (focus stays put) |

`o` runs `$EDITOR +<line> <path>` (default `nvim`). For editors that take another form, set a template, e.g. `CODEMAPX_EDITOR='code -g {path}:{line}'`.

The TUI captures the mouse for border drags and wheel scrolling; most terminals still select text with shift-drag (option-drag in iTerm2 and Terminal.app).

The map wants about 170 columns. Below that, Tests and Docs collapse to counts; below 130 the functions panel hides; below 100 it asks for a wider terminal.

## Development

`cargo test` runs against a synthetic TS repo built from `tests/sample/`. Goldens (`tests/sample/expected-facts.json`, `tests/golden/*.txt`) are refreshed with `UPDATE_GOLDEN=1 cargo test`; review the diff before committing.

`fixtures/` is local only and gitignored: pilot maps built from private branches. Never commit it.

See [docs/design-notes.md](docs/design-notes.md) for the pilot and [docs/superpowers/specs/2026-10-01-codemapx-v1-design.md](docs/superpowers/specs/2026-10-01-codemapx-v1-design.md) for the v1 design.
