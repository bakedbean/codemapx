# Diff agent chat design

Date: 2026-10-06. Status: approved in conversation, awaiting written-spec review.

## Goal

While reviewing a file's diff in the TUI, ask a coding agent about the change without leaving
codemapx or typing out paths and line numbers. A chat panel sits between the diff and the
minimap and hosts an agent's own interactive TUI, seeded with the whole changeset.

Done when:

1. `cargo test` passes, including the new briefing, agent, keys, PTY and layout tests.
2. In a real worktree, `a` opens the panel, claude starts with the briefing, focusing the panel
   prefills a `path:start-end ` reference, and a question about the diff gets an answer.
3. The same works with `CODEMAPX_AGENT=codex`.
4. Ctrl-x returns focus to the diff; the panel resizes by drag and hides with `a`.

## Decisions

| Topic | Decision |
|---|---|
| Which agent | A separate agent that codemapx launches itself. No wsx dependency; it does not talk to the workspace's primary agent. |
| How it runs | The agent's interactive TUI in a PTY, rendered into the panel, the way wsx hosts agents. Never a headless/print/SDK mode, so it bills like normal interactive use. |
| Agent kinds | claude (default) and codex. `CODEMAPX_AGENT=claude\|codex` picks; `CODEMAPX_AGENT_BIN` overrides the binary. A kind table leaves room for pi/omp/hermes. |
| Session life | Started the first time the panel opens, kept while hidden, killed on quit. Nothing persisted; no resume flags. |
| Seeding | A briefing built from the merged `Map`, passed as launch instructions. |
| File context | On focusing the panel, codemapx types a `path:start-end ` reference into the agent's composer without Enter. |
| Permissions | Read-only: reads, search and read-only git are pre-approved; anything else prompts in the pane. |
| PTY stack | `portable-pty` 0.9 and `fnug-vt100` 0.15.2 (imported as `vt100`), matching wsx. Renderer and key encoder lifted from wsx. No tokio. |

## Layout and keys

The diff row becomes `[functions | diff | chat | minimap]`; the minimap's width reserves the chat's and functions panel's minimums.

- `a` shows or hides the panel. It starts hidden; the first show starts the agent and focuses
  the panel. Hiding while focused focuses the diff.
- Default width is 40% of the row, minimum 40 columns. The diff/chat border drags like the
  functions and minimap borders (`Divider::DiffChat`); the width is clamped the same way.
- The panel also shows in full-screen diff (`d`). Narrow terminals (below `MIN_WIDTH`) hide it
  like everything else.
- `Pane::Chat` joins the tab cycle when the panel is on screen. A left click inside it focuses it.
- While focused, every key is encoded and written to the PTY, except Ctrl-x, which focuses the
  diff. `q`, `esc`, arrows etc. all go to the agent.
- Bracketed pastes are forwarded wrapped in `ESC[200~ … ESC[201~` when the panel is focused.
- The wheel over the panel goes to the agent as SGR wheel reports when it has mouse reporting on;
  otherwise it scrolls codemapx's own scrollback (1000 lines). Any key returns to the live screen.
- The panel title shows the agent kind, and `· ctrl-x leaves` while focused.
- When the agent exits, the panel shows `agent exited (code N) — ⏎ to restart`; ⏎ in the
  focused panel starts a fresh one.
- `o` (open in editor) suspends the TUI as today; the agent keeps running.
- `view --snapshot` never starts an agent.
- The help line gains `a chat`.

## Briefing

`briefing(map: &Map, map_dir: &Path) -> String`, pure. No diff text, so it stays a few KB.

1. Role: you are answering a reviewer's questions about branch `<branch>` of `<repo>` from inside
   codemapx; read-only, do not edit files; references like `path:42-61` are line numbers in the
   file at HEAD; keep answers short unless asked.
2. Facts: base, head, commit count, files changed, `+add −del`.
3. The map title and summary.
4. Changed files in trail order: path, status, `+/-`, the card's `what`, and its outline entries
   with their notes and line ranges.
5. Links: `from → to: reason`.
6. Context and missing cards with their notes.
7. Where to look: `git diff <base>...HEAD -- <path>` for the change itself; `<map_dir>/facts.json`
   and `<map_dir>/annotations.json` for raw detail.

`App` gains `map_dir: Option<PathBuf>`, set by `cmd_view` from `Loaded::dir`.

## Launch

Spawned in the worktree root with the parent environment and PTY size equal to the panel's inner
area.

| | claude | codex |
|---|---|---|
| Instructions | `--append-system-prompt <briefing>` | `-c developer_instructions=<briefing as TOML basic string>` |
| Read-only | `--permission-mode default --allowedTools Read Grep Glob "Bash(git diff:*)" "Bash(git log:*)" "Bash(git show:*)"` | `-s read-only -a on-request` |
| Composer ready | alternate screen is active and a `❯` row sits under a `─` rule | a row starting with `›` and the cursor visible |
| Insert | bracketed paste, no CR | bracketed paste, no CR |

If the binary can't be started, the panel shows `can't run <bin>: <err>` and nothing else changes.

## Prefill

`reference(app) -> Option<String>` for the selected card's HEAD path:

1. Functions panel selection: its `start-end`.
2. Inside pane selection: its `start-end`.
3. Highlighted diff line: that line.
4. Otherwise the first and last numbered lines visible in the diff.

None for missing cards, deleted files and binaries. The result is `path:start-end ` (or
`path:line `). Each time the panel gains focus, if the reference differs from the last one typed,
it is queued. A queued reference is written once the session is at least 1.5 s old, its output has
been quiet for 400 ms, and the kind's composer-ready check passes. It is dropped when focus leaves the chat, and typing into a ready composer cancels it. Only
the newest queued reference is kept.

## Event loop

- `init` also enables bracketed paste; `restore` disables it.
- The PTY reader is a std thread: blocking 4 KB reads into the shared
  `Arc<Mutex<vt100::Parser>>`, updating a last-output instant, then sending `()` on an
  `mpsc::Sender`. On EOF it records the exit code from `child.wait()`.
- With no live session the loop blocks on `event::read()` as today. With one it calls
  `event::poll(16ms)`, drains the wake channel, tries the queued prefill, and redraws only after
  input or PTY output.
- Drawing resizes the PTY and parser when the panel's inner size changes.
- `Session` kills the child on drop, so quitting codemapx ends the agent.

## Modules

New, under `src/tui/chat/`:

| File | Does |
|---|---|
| `briefing.rs` | `briefing(&Map, &Path) -> String` |
| `agent.rs` | `AgentKind`, `from_env`, `argv(kind, bin, briefing)`, `ready(kind, &vt100::Screen)` |
| `pty.rs` | `Session`: spawn, write, resize, exit status, reader thread, wake channel, quiet timer |
| `render.rs` | vt100 screen → ratatui `Buffer`, from wsx `src/pty/render.rs` |
| `keys.rs` | `encode_key(KeyEvent) -> Vec<u8>`, `wrap_paste`, from wsx `src/app/input/keys.rs`, plus Home/End/PgUp/PgDn/Delete/Shift-Tab |
| `mod.rs` | `Chat` state (session, queued and last reference, scrollback offset), `draw`, `reference` |

Changed: `tui/app.rs` (pane, divider, `show_chat`, `chat_width`, `chat_pane`, `chat: Option<Chat>`,
`map_dir`), `tui/mod.rs` (layout, loop, paste), `tui/keys.rs` (`a`, Ctrl-x, forwarding),
`tui/mouse.rs` (focus click, divider, wheel), `main.rs` (`map_dir`), `README.md`, `Cargo.toml`.

## Testing

- `briefing`: golden `tests/golden/briefing.txt` from the sample map.
- `agent`: argv for both kinds, including TOML escaping of quotes, backslashes and newlines;
  `ready` against hand-built vt100 screens.
- `keys`: table test of encodings.
- `reference`: precedence and the none cases.
- `pty`: spawn `/bin/sh -c 'printf hi; sleep 1'`, see `hi` on the screen, resize, observe exit.
- Layout: snapshot goldens at 180 columns with the panel shown (placeholder, no session), the tab
  cycle including `Chat`, Ctrl-x back to the diff, hiding while focused.
- Real claude and codex runs are checked by hand.

## Out of scope

- pi, omp and hermes.
- Persisting or resuming chats.
- Talking to the wsx primary agent.
- Auto-appending context to every message.
- Chat in the HTML page.
