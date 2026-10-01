# codemapx

A navigable map of the code changes an agent made on a branch: which files changed, which edit
made the next one necessary, and why. Built for catching up on a branch before review, not for
watching edits stream in.

Status: prototype. See [docs/design-notes.md](docs/design-notes.md) for what the pilot learned and
the planned collect → annotate → render pipeline.

## Try the prototype TUI

```sh
cargo run -- <map-dir>
```

`<map-dir>` holds a `config.json` (the map) and a `diffs.json` (from `scripts/collect-diffs.py`).
An optional `"ROOT"` in the config is the worktree `o` opens files under. Needs a terminal about
180×45.

| Key | Action |
|---|---|
| `←` `→` | step through the edits in reading order |
| `tab` | move between panes |
| `↑` `↓` | move within the focused pane |
| `⏎` | follow a came-from / leads-to link |
| `o` | open `$EDITOR` at the outline entry or the top visible diff line |
| `d` | full-screen diff |
| `J` `K` | page the diff |
| `q` | quit |

`cargo run -- <map-dir> --snapshot <node-id> [outline-index]` prints one 180×52 frame as text.

## Scripts

```sh
scripts/collect-diffs.py <worktree> [base-ref] > diffs.json
scripts/validate-map.js <config.json> <diffs.json>
scripts/render-html.py <config.json> <diffs.json> "<title>" > change-map.html
```

## Layout

- `src/` – prototype TUI (ratatui)
- `web/` – HTML template
- `scripts/` – diff collection, validation, HTML rendering
- `fixtures/` – local only, gitignored: pilot maps built from private ssk-web branches
- `docs/` – design notes
