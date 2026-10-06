# Diff Agent Chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A chat panel between the diff and the minimap that hosts claude's or codex's own interactive TUI in a PTY, briefed on the branch's map, with a `path:start-end ` reference prefilled when the panel gains focus.

**Architecture:** New `src/tui/chat/` module: a pure briefing builder, an agent-kind table (argv + composer-ready check), a key encoder and vt100→ratatui renderer lifted from wsx, and a `Session` that owns a `portable-pty` child plus a reader thread feeding a shared `vt100::Parser`. The TUI's blocking event loop becomes a 16 ms poll while an agent is alive; the panel slots into the diff row's horizontal layout like the functions panel and minimap.

**Tech Stack:** Rust 2024, ratatui 0.29 (crossterm 0.28), `portable-pty` 0.9, `fnug-vt100` 0.15.2 imported as `vt100`. No tokio.

**Spec:** `docs/superpowers/specs/2026-10-06-diff-agent-chat-design.md`

## Global Constraints

- Never launch an agent in headless/print/SDK mode; always its interactive TUI.
- Agent kinds: `claude` (default) and `codex`; `CODEMAPX_AGENT=claude|codex` picks, `CODEMAPX_AGENT_BIN` overrides the binary.
- Read-only: claude gets `--allowedTools Read Grep Glob "Bash(git diff:*)" "Bash(git log:*)" "Bash(git show:*)"`; codex gets `-s read-only -a on-request`.
- No session id / resume flags; nothing persisted.
- `view --snapshot` and every test must never start a real agent.
- Panel: `a` toggles, starts hidden, default width 40% of the row's room (row less the minimap, and less the functions panel's 16-column minimum when it is shown), minimum 40 columns, diff keeps `MIN_DIFF_WIDTH` (40).
- Ctrl-x is the only key the focused panel keeps; everything else goes to a live agent.
- Prefill waits for session age ≥ 1.5 s, output quiet ≥ 400 ms and the composer-ready check; dropped after 10 s; only the newest is kept.
- Dependencies: `portable-pty = "0.9"`, `vt100 = { version = "0.15.2", package = "fnug-vt100" }`.
- Comments: terse, one sentence for the non-obvious contract, matching the surrounding code. No Claude attribution in commits.
- Goldens refresh with `UPDATE_GOLDEN=1 cargo test`; review `git diff tests/golden` before committing.

**Clarification of the spec's prefill precedence:** the functions/outline selection is always set when those lists are non-empty, so "selection" means *the pane focus came from*: coming from the functions panel uses the selected function, from the inside pane the selected outline entry; otherwise the highlighted diff line, else the visible range.

## Review Focus

1. Ctrl-c, `q`, `esc`, Tab while the chat is focused must reach the agent, not quit codemapx or move focus — pinned in Task 5 (`focused_chat_forwards_quit_keys`).
2. The agent binary missing (`CODEMAPX_AGENT_BIN=/nope`) must show `can't run /nope: …` in the panel, not crash — pinned in Task 4 (`missing_binary_is_an_error`) and Task 8 manual check.
3. An agent that exits on its own must flip the panel to the exit message without a keypress and stop the 16 ms polling — pinned in Task 4 (`exit_is_observed`) and the Task 8 loop's `!c.live()` redraw.
4. Narrow terminals (100–129 cols) with the chat shown must keep the diff ≥ 40 columns and not underflow widths — pinned in Task 5 (`chat_width_leaves_the_diff_forty_columns`).
5. A briefing with quotes, backslashes and newlines must reach codex intact as a TOML string — pinned in Task 2 (`codex_argv_escapes_the_briefing`).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/tui/chat/mod.rs` | `Chat` state, panel `width`/`draw`, `reference`, `start`, prefill `queue`/`tick` |
| `src/tui/chat/briefing.rs` | `briefing(&Map, Option<&Path>) -> String` |
| `src/tui/chat/agent.rs` | `AgentKind`, `argv`, `ready` |
| `src/tui/chat/keys.rs` | `encode_key`, `wrap_paste` |
| `src/tui/chat/render.rs` | `render_screen` (vt100 → ratatui) |
| `src/tui/chat/pty.rs` | `Session`, `settled` |
| `src/tui/{app,mod,keys,mouse}.rs` | pane/divider/fields, layout + loop, key routing, mouse |
| `src/main.rs` | set `app.map_dir` |
| `tests/chat.rs` | all new tests |
| `README.md` | keys and env vars |

---

### Task 1: Briefing

**Files:**
- Create: `src/tui/chat/mod.rs`, `src/tui/chat/briefing.rs`, `tests/chat.rs`, `tests/golden/briefing.txt` (generated)
- Modify: `src/tui/mod.rs:28-35` (module list)

**Interfaces:**
- Produces: `codemapx::tui::chat::briefing::briefing(map: &Map, map_dir: Option<&Path>) -> String`

- [ ] **Step 1: Write the failing test** — create `tests/chat.rs`:

```rust
mod common;

use std::path::Path;

use codemapx::tui::chat::briefing::briefing;

#[test]
fn briefing_covers_the_map_without_diffs() {
    let map = common::sample_map();
    let b = briefing(&map, Some(Path::new("/state/map")));
    for needle in [
        "branch `feature/12-apply-fees`",
        "Read-only",
        "## Changed files, in reading order",
        "`src/billing/apply.ts` (added",
        "## Links",
        " → ",
        "git diff ",
        "/state/map/facts.json",
        "/state/map/annotations.json",
    ] {
        assert!(b.contains(needle), "missing {needle:?}\n{b}");
    }
    assert!(!b.contains("@@ "), "no diff text in the briefing");
    common::assert_golden("tests/golden/briefing.txt", &b);
}

#[test]
fn briefing_without_a_map_dir_skips_the_raw_files() {
    let b = briefing(&common::sample_map(), None);
    assert!(!b.contains("facts.json"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test chat`
Expected: compile error, `could not find chat in tui`.

- [ ] **Step 3: Implement** — create `src/tui/chat/mod.rs`:

```rust
//! The agent chat panel: an agent's own TUI in a PTY beside the diff, briefed on the branch.

pub mod briefing;
```

Create `src/tui/chat/briefing.rs`:

```rust
//! What the chat agent is told at launch: the map as text, no diffs, so it stays a few KB.

use std::{fmt::Write, path::Path};

use crate::map::{Card, CardKind, Map};

fn label(c: &Card) -> &str {
    c.path.as_deref().unwrap_or(&c.name)
}

pub fn briefing(map: &Map, map_dir: Option<&Path>) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "You are answering a reviewer's questions about branch `{}` of `{}`, from a chat panel inside codemapx, a TUI for reviewing a branch's changes.", map.branch, map.repo);
    s.push_str("- Read-only: do not edit, create or delete files, and do not commit.\n");
    s.push_str("- A reference like `src/a.ts:42-61` means those lines of the file at HEAD.\n");
    s.push_str("- Keep answers short unless asked for more.\n\n");
    let _ = writeln!(s, "## Branch\n\nbase {}, head {}, {} commits, {} files, +{} −{}\n", map.base, map.head, map.commits, map.files, map.add, map.del);
    let _ = writeln!(s, "## Map: {}\n\n{}\n", map.title, map.summary);
    s.push_str("## Changed files, in reading order\n\n");
    let rest = (0..map.cards.len()).filter(|i| !map.trail.contains(i));
    for i in map.trail.iter().copied().chain(rest) {
        let c = &map.cards[i];
        if c.kind != CardKind::Changed {
            continue;
        }
        let status = c.status.map(|st| format!("{st:?}").to_lowercase()).unwrap_or_default();
        let _ = writeln!(s, "- `{}` ({status}, +{} −{}): {}", label(c), c.add, c.del, c.what);
        for o in &c.outline {
            let note = if o.note.is_empty() { String::new() } else { format!(": {}", o.note) };
            let _ = writeln!(s, "  - {} ({}, lines {}-{}){note}", o.name, o.kind, o.start, o.end);
        }
    }
    s.push_str("\n## Links (an edit in the first made the edit in the second necessary)\n\n");
    for l in &map.links {
        let _ = writeln!(s, "- `{}` → `{}`: {}", label(&map.cards[l.from]), label(&map.cards[l.to]), l.reason);
    }
    let others: Vec<&Card> = map.cards.iter().filter(|c| c.kind != CardKind::Changed).collect();
    if !others.is_empty() {
        s.push_str("\n## Context and not built\n\n");
        for c in others {
            let why = if c.kind == CardKind::Context { "unchanged context" } else { "not built yet" };
            let _ = writeln!(s, "- `{}` ({why}): {}", label(c), c.what);
        }
    }
    let _ = writeln!(s, "\n## Where to look\n\n- `git diff {}...HEAD -- <path>` shows a file's change.", map.base);
    if let Some(d) = map_dir {
        let _ = writeln!(s, "- Raw facts: `{}`; annotations: `{}`.", d.join("facts.json").display(), d.join("annotations.json").display());
    }
    s
}
```

In `src/tui/mod.rs`, add to the module list (alphabetical, before `mod diff_pane;`):

```rust
pub mod chat;
```

- [ ] **Step 4: Generate the golden, then run**

Run: `UPDATE_GOLDEN=1 cargo test --test chat && cargo test --test chat`
Expected: PASS. Open `tests/golden/briefing.txt` and check it reads sensibly: every changed card once, links with reasons, no diff hunks. If `(added` does not match apply.ts's status in the sample, fix the needle to the real status shown in the golden.

- [ ] **Step 5: Commit**

```bash
git add src/tui/chat src/tui/mod.rs tests/chat.rs tests/golden/briefing.txt
git commit -m "feat(chat): brief an agent on the map"
```

---

### Task 2: Agent kinds

**Files:**
- Create: `src/tui/chat/agent.rs`
- Modify: `Cargo.toml` (deps), `src/tui/chat/mod.rs` (module list), `tests/chat.rs`

**Interfaces:**
- Produces: `agent::AgentKind { Claude, Codex }` (`Clone, Copy, PartialEq, Debug`), `AgentKind::from_env(Option<&str>) -> Result<AgentKind, String>`, `AgentKind::name(self) -> &'static str`, `agent::argv(kind, bin: Option<&str>, briefing: &str) -> Vec<String>`, `agent::ready(kind, &vt100::Screen) -> bool`

- [ ] **Step 1: Add the dependency** — in `Cargo.toml` `[dependencies]`, keep alphabetical order:

```toml
portable-pty = "0.9"
```
(after `clap`)
```toml
vt100 = { version = "0.15.2", package = "fnug-vt100" }
```
(after `unicode-width`)

- [ ] **Step 2: Write the failing tests** — append to `tests/chat.rs`:

```rust
use codemapx::tui::chat::agent::{AgentKind, argv, ready};

#[test]
fn agent_kind_comes_from_the_env() {
    assert_eq!(AgentKind::from_env(None), Ok(AgentKind::Claude));
    assert_eq!(AgentKind::from_env(Some(" ")), Ok(AgentKind::Claude));
    assert_eq!(AgentKind::from_env(Some("codex")), Ok(AgentKind::Codex));
    assert_eq!(AgentKind::from_env(Some("gpt")), Err("CODEMAPX_AGENT=gpt: expected claude or codex".into()));
}

#[test]
fn claude_argv_is_read_only_with_the_briefing() {
    assert_eq!(
        argv(AgentKind::Claude, None, "brief"),
        ["claude", "--allowedTools", "Read", "Grep", "Glob", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)", "--append-system-prompt", "brief"]
    );
    assert_eq!(argv(AgentKind::Claude, Some("/opt/claude"), "b")[0], "/opt/claude");
    assert_eq!(argv(AgentKind::Claude, Some(""), "b")[0], "claude");
}

#[test]
fn codex_argv_escapes_the_briefing() {
    let v = argv(AgentKind::Codex, None, "say \"hi\"\nC:\\x\t.");
    assert_eq!(v[..6], ["codex", "-s", "read-only", "-a", "on-request", "-c"]);
    assert_eq!(v[6], r#"developer_instructions="say \"hi\"\nC:\\x\t.""#);
    assert_eq!(v.len(), 7);
}

#[test]
fn claude_is_ready_on_the_alternate_screen() {
    let mut p = vt100::Parser::new(5, 40, 0);
    assert!(!ready(AgentKind::Claude, p.screen()));
    p.process(b"\x1b[?1049h");
    assert!(ready(AgentKind::Claude, p.screen()));
}

#[test]
fn codex_is_ready_at_its_composer_not_its_trust_dialog() {
    let mut p = vt100::Parser::new(5, 40, 0);
    assert!(!ready(AgentKind::Codex, p.screen()));
    p.process("\x1b[?25l› 1. Yes, continue".as_bytes());
    assert!(!ready(AgentKind::Codex, p.screen()), "cursor hidden: trust dialog");
    let mut p = vt100::Parser::new(5, 40, 0);
    p.process("\r\n  › Ask Codex to do anything".as_bytes());
    assert!(ready(AgentKind::Codex, p.screen()));
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --test chat`
Expected: compile error, unresolved `agent`.

- [ ] **Step 4: Implement** — add `pub mod agent;` to `src/tui/chat/mod.rs` (above `pub mod briefing;`). Create `src/tui/chat/agent.rs`:

```rust
//! The agents the chat panel runs: how to launch each read-only with the briefing, and when its composer takes input.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
}

/// Pre-approved claude tools: reading and read-only git.
const CLAUDE_TOOLS: [&str; 6] = ["Read", "Grep", "Glob", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)"];

impl AgentKind {
    /// `CODEMAPX_AGENT`'s value; unset or blank is claude.
    pub fn from_env(v: Option<&str>) -> Result<AgentKind, String> {
        match v.map(str::trim).unwrap_or("") {
            "" | "claude" => Ok(AgentKind::Claude),
            "codex" => Ok(AgentKind::Codex),
            other => Err(format!("CODEMAPX_AGENT={other}: expected claude or codex")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }
}

/// `bin` (CODEMAPX_AGENT_BIN) or the kind's own name, read-only, with `briefing` as standing instructions.
pub fn argv(kind: AgentKind, bin: Option<&str>, briefing: &str) -> Vec<String> {
    let bin = bin.filter(|b| !b.trim().is_empty()).unwrap_or(kind.name());
    let mut v = vec![bin.to_string()];
    match kind {
        AgentKind::Claude => {
            v.push("--allowedTools".into());
            v.extend(CLAUDE_TOOLS.map(String::from));
            v.extend(["--append-system-prompt".into(), briefing.into()]);
        }
        AgentKind::Codex => {
            v.extend(["-s", "read-only", "-a", "on-request", "-c"].map(String::from));
            v.push(format!("developer_instructions={}", toml_string(briefing)));
        }
    }
    v
}

/// `s` as a TOML basic string, for codex's `-c key=value`.
fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Claude's composer is up once it switches to the alternate screen; codex's is a `›` row with the cursor
/// visible (its trust dialog marks rows with `›` too, but hides the cursor).
pub fn ready(kind: AgentKind, screen: &vt100::Screen) -> bool {
    match kind {
        AgentKind::Claude => screen.alternate_screen(),
        AgentKind::Codex => {
            let (rows, cols) = screen.size();
            !screen.hide_cursor() && (0..rows).any(|r| screen.contents_between(r, 0, r, cols).trim_start().starts_with('›'))
        }
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --test chat`
Expected: PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/tui/chat tests/chat.rs
git commit -m "feat(chat): launch claude or codex read-only with the briefing"
```

---

### Task 3: Key encoding and screen rendering

**Files:**
- Create: `src/tui/chat/keys.rs`, `src/tui/chat/render.rs`
- Modify: `src/tui/chat/mod.rs` (module list), `tests/chat.rs`

**Interfaces:**
- Produces: `chat::keys::encode_key(KeyEvent) -> Vec<u8>`, `chat::keys::wrap_paste(&str) -> Vec<u8>`, `chat::render::render_screen(&vt100::Screen, &mut Buffer, Rect)`

- [ ] **Step 1: Write the failing tests** — append to `tests/chat.rs`:

```rust
use codemapx::tui::chat::{keys::{encode_key, wrap_paste}, render::render_screen};
use ratatui::{buffer::Buffer, crossterm::event::{KeyCode, KeyEvent, KeyModifiers}, layout::Rect, style::Color};

fn enc(code: KeyCode, m: KeyModifiers) -> Vec<u8> {
    encode_key(KeyEvent::new(code, m))
}

#[test]
fn keys_encode_like_a_terminal() {
    let none = KeyModifiers::NONE;
    let cases: [(KeyCode, KeyModifiers, &[u8]); 16] = [
        (KeyCode::Char('a'), none, b"a"),
        (KeyCode::Char('é'), none, "é".as_bytes()),
        (KeyCode::Char('c'), KeyModifiers::CONTROL, b"\x03"),
        (KeyCode::Char('z'), KeyModifiers::CONTROL, b""),
        (KeyCode::Char('d'), KeyModifiers::CONTROL, b""),
        (KeyCode::Char('b'), KeyModifiers::ALT, b"\x1bb"),
        (KeyCode::Enter, none, b"\r"),
        (KeyCode::Backspace, none, b"\x7f"),
        (KeyCode::Tab, none, b"\t"),
        (KeyCode::BackTab, KeyModifiers::SHIFT, b"\x1b[Z"),
        (KeyCode::Esc, none, b"\x1b"),
        (KeyCode::Up, none, b"\x1b[A"),
        (KeyCode::Home, none, b"\x1b[H"),
        (KeyCode::PageDown, none, b"\x1b[6~"),
        (KeyCode::Delete, none, b"\x1b[3~"),
        (KeyCode::F(5), none, b""),
    ];
    for (code, m, want) in cases {
        assert_eq!(enc(code, m), want, "{code:?} {m:?}");
    }
    assert_eq!(wrap_paste("a\nb"), b"\x1b[200~a\nb\x1b[201~");
}

fn render(bytes: &[u8]) -> Buffer {
    let mut p = vt100::Parser::new(2, 10, 0);
    p.process(bytes);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    render_screen(p.screen(), &mut buf, Rect::new(0, 0, 10, 2));
    buf
}

#[test]
fn screens_render_text_wide_glyphs_and_color() {
    let b = render(b"hello");
    assert_eq!((0..5).map(|x| b[(x, 0)].symbol().to_string()).collect::<String>(), "hello");
    assert_eq!(b[(7, 1)].symbol(), " ");
    let b = render("世界".as_bytes());
    assert_eq!([b[(0, 0)].symbol(), b[(1, 0)].symbol(), b[(2, 0)].symbol()], ["世", " ", "界"]);
    assert_eq!(render(b"\x1b[31mX")[(0, 0)].fg, Color::Indexed(1));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test chat`
Expected: compile error, unresolved `keys` / `render`.

- [ ] **Step 3: Implement** — module list in `src/tui/chat/mod.rs` becomes:

```rust
pub mod agent;
pub mod briefing;
pub mod keys;
pub mod render;
```

Create `src/tui/chat/keys.rs`:

```rust
//! crossterm keys to the bytes a terminal program reads, after wsx's encoder.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Empty for keys with no encoding, and for ctrl-z and ctrl-d, which would suspend or end the agent.
pub fn encode_key(k: KeyEvent) -> Vec<u8> {
    use KeyCode::*;
    let bytes: &[u8] = match k.code {
        Char(c) if k.modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_alphabetic() => {
            return match c.to_ascii_lowercase() {
                'z' | 'd' => vec![],
                c => vec![c as u8 - b'a' + 1],
            };
        }
        Char(c) => {
            let mut out = if k.modifiers.contains(KeyModifiers::ALT) { vec![0x1b] } else { vec![] };
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            return out;
        }
        Enter => b"\r",
        Backspace => b"\x7f",
        Tab => b"\t",
        BackTab => b"\x1b[Z",
        Esc => b"\x1b",
        Left => b"\x1b[D",
        Right => b"\x1b[C",
        Up => b"\x1b[A",
        Down => b"\x1b[B",
        Home => b"\x1b[H",
        End => b"\x1b[F",
        PageUp => b"\x1b[5~",
        PageDown => b"\x1b[6~",
        Delete => b"\x1b[3~",
        _ => b"",
    };
    bytes.to_vec()
}

/// Bracketed paste, so the agent takes newlines as text rather than Enter.
pub fn wrap_paste(s: &str) -> Vec<u8> {
    [b"\x1b[200~".as_slice(), s.as_bytes(), b"\x1b[201~"].concat()
}
```

Create `src/tui/chat/render.rs` (lifted from wsx `src/pty/render.rs`):

```rust
//! A vt100 screen drawn into a ratatui buffer, from wsx.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

/// Cells past the screen's size are blanked.
pub fn render_screen(screen: &vt100::Screen, buf: &mut Buffer, area: Rect) {
    let (rows, cols) = screen.size();
    for y in 0..area.height {
        for x in 0..area.width {
            let out = &mut buf[(area.x + x, area.y + y)];
            let Some(cell) = (y < rows && x < cols).then(|| screen.cell(y, x)).flatten() else {
                out.reset();
                continue;
            };
            // has_contents() is false only for blank cells; a wide glyph's continuation cell says true but is empty.
            let glyph = if cell.has_contents() { cell.contents() } else { String::new() };
            out.set_symbol(if glyph.is_empty() { " " } else { &glyph });
            out.set_style(style(cell));
        }
    }
}

fn style(cell: &vt100::Cell) -> Style {
    let mut s = Style::default().fg(color(cell.fgcolor())).bg(color(cell.bgcolor()));
    for (on, m) in [(cell.bold(), Modifier::BOLD), (cell.italic(), Modifier::ITALIC), (cell.underline(), Modifier::UNDERLINED), (cell.inverse(), Modifier::REVERSED)] {
        if on {
            s = s.add_modifier(m);
        }
    }
    s
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --test chat`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tui/chat tests/chat.rs
git commit -m "feat(chat): encode keys for and draw an embedded terminal"
```

---

### Task 4: PTY session

**Files:**
- Create: `src/tui/chat/pty.rs`
- Modify: `src/tui/chat/mod.rs` (module list), `tests/chat.rs`

**Interfaces:**
- Produces:
  - `pty::Session::spawn(argv: &[String], cwd: &Path, rows: u16, cols: u16, wake: mpsc::Sender<()>) -> Result<Session, String>` (error text starts `can't run <argv[0]>: `)
  - `Session::write(&mut self, &[u8])`, `resize(&mut self, rows, cols)`, `parser(&self) -> MutexGuard<'_, vt100::Parser>`, `exit_code(&self) -> Option<u32>`, `settled(&self, Instant) -> bool`, `scroll_by(&self, isize)`, `scroll_to_live(&self)`, `wheel_bytes(&self, up: bool, col: u16, row: u16) -> Option<Vec<u8>>`
  - `pty::settled(started: Instant, last_output: Option<Instant>, now: Instant) -> bool`
  - Dropping a `Session` kills the child.

- [ ] **Step 1: Write the failing tests** — append to `tests/chat.rs`:

```rust
use std::{sync::mpsc, time::{Duration, Instant}};

use codemapx::tui::chat::pty::{Session, settled};

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

/// Polls `done` until it holds or `secs` pass, sleeping on the wake channel in between.
fn wait_for(rx: &mpsc::Receiver<()>, secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if done() {
            return true;
        }
        let _ = rx.recv_timeout(Duration::from_millis(50));
    }
    done()
}

#[test]
fn a_session_shows_output_and_resizes() {
    let (tx, rx) = mpsc::channel();
    let mut s = Session::spawn(&sh("printf hi; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains("hi")));
    s.resize(10, 40);
    assert_eq!(s.parser().screen().size(), (10, 40));
    assert_eq!(s.exit_code(), None);
}

#[test]
fn exit_is_observed() {
    let (tx, rx) = mpsc::channel();
    let s = Session::spawn(&sh("exit 3"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.exit_code().is_some()));
    assert_eq!(s.exit_code(), Some(3));
}

#[test]
fn missing_binary_is_an_error() {
    let (tx, _rx) = mpsc::channel();
    let e = Session::spawn(&["/nope/agent".to_string()], Path::new("/"), 5, 20, tx).err().unwrap();
    assert!(e.starts_with("can't run /nope/agent: "), "{e}");
}

#[test]
fn settled_needs_age_quiet_and_some_output() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    assert!(!settled(t0, None, ms(5000)), "no output yet");
    assert!(!settled(t0, Some(ms(100)), ms(1000)), "too young");
    assert!(!settled(t0, Some(ms(1400)), ms(1600)), "not quiet");
    assert!(settled(t0, Some(ms(1100)), ms(1600)));
}

#[test]
fn wheel_reports_only_when_the_program_asks_for_mouse() {
    let (tx, rx) = mpsc::channel();
    let s = Session::spawn(&sh("printf '\\033[?1000h\\033[?1006hm'; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains('m')));
    assert_eq!(s.wheel_bytes(true, 3, 2), Some(b"\x1b[<64;3;2M".to_vec()));
    let (tx, rx) = mpsc::channel();
    let s = Session::spawn(&sh("printf x; sleep 2"), Path::new("/"), 5, 20, tx).unwrap();
    assert!(wait_for(&rx, 3, || s.parser().screen().contents().contains('x')));
    assert_eq!(s.wheel_bytes(true, 3, 2), None);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test chat`
Expected: compile error, unresolved `pty`.

- [ ] **Step 3: Implement** — add `pub mod pty;` to the module list in `src/tui/chat/mod.rs` (after `keys`). Create `src/tui/chat/pty.rs`:

```rust
//! An agent's TUI in a pseudo-terminal: a reader thread feeds a vt100 screen and pings the event loop.

use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, mpsc::Sender},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Minimum age before text is typed into the agent, so it isn't lost to start-up redraws.
const SETTLE: Duration = Duration::from_millis(1500);
/// Output silence that counts as idle.
const QUIET: Duration = Duration::from_millis(400);
const SCROLLBACK: usize = 1000;

pub struct Session {
    parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    exit: Arc<Mutex<Option<u32>>>,
    last_output: Arc<Mutex<Option<Instant>>>,
    started: Instant,
    size: (u16, u16),
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Session {
    /// Runs `argv` in `cwd` with the parent's environment; `wake` gets `()` after each chunk of output and at exit.
    pub fn spawn(argv: &[String], cwd: &Path, rows: u16, cols: u16, wake: Sender<()>) -> Result<Session, String> {
        let bin = argv.first().ok_or("no agent command")?;
        let fail = |e: &dyn std::fmt::Display| format!("can't run {bin}: {e}");
        let pair = native_pty_system().openpty(pty_size(rows, cols)).map_err(|e| fail(&e))?;
        let mut cmd = CommandBuilder::new(bin);
        cmd.args(&argv[1..]);
        cmd.cwd(cwd);
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| fail(&e))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| fail(&e))?;
        let writer = pair.master.take_writer().map_err(|e| fail(&e))?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK)));
        let exit = Arc::new(Mutex::new(None));
        let last_output = Arc::new(Mutex::new(None));
        let (p, x, l) = (parser.clone(), exit.clone(), last_output.clone());
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                p.lock().unwrap().process(&buf[..n]);
                *l.lock().unwrap() = Some(Instant::now());
                let _ = wake.send(());
            }
            *x.lock().unwrap() = Some(child.wait().map(|s| s.exit_code()).unwrap_or(u32::MAX));
            let _ = wake.send(());
        });
        Ok(Session { parser, master: pair.master, writer, killer, exit, last_output, started: Instant::now(), size: (rows, cols) })
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let _ = self.writer.write_all(bytes).and_then(|_| self.writer.flush());
        }
    }

    /// No-op unless the size changed, since drawing calls it every frame.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) != self.size {
            self.size = (rows, cols);
            let _ = self.master.resize(pty_size(rows, cols));
            self.parser.lock().unwrap().set_size(rows, cols);
        }
    }

    pub fn parser(&self) -> MutexGuard<'_, vt100::Parser> {
        self.parser.lock().unwrap()
    }

    pub fn exit_code(&self) -> Option<u32> {
        *self.exit.lock().unwrap()
    }

    pub fn settled(&self, now: Instant) -> bool {
        settled(self.started, *self.last_output.lock().unwrap(), now)
    }

    /// Positive `d` moves back into the scrollback.
    pub fn scroll_by(&self, d: isize) {
        let mut p = self.parser();
        let cur = p.screen().scrollback() as isize;
        p.set_scrollback((cur + d).max(0) as usize);
    }

    pub fn scroll_to_live(&self) {
        self.parser().set_scrollback(0);
    }

    /// The wheel as the program asked to hear it (`col`/`row` 1-based in the panel), or None when it didn't ask.
    pub fn wheel_bytes(&self, up: bool, col: u16, row: u16) -> Option<Vec<u8>> {
        let p = self.parser();
        let screen = p.screen();
        if screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
            return None;
        }
        let cb: u16 = if up { 64 } else { 65 };
        Some(match screen.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Sgr => format!("\x1b[<{cb};{col};{row}M").into_bytes(),
            _ => vec![0x1b, b'[', b'M', 32 + cb as u8, 32 + col.min(223) as u8, 32 + row.min(223) as u8],
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

/// Old enough, has drawn something, and quiet since.
pub fn settled(started: Instant, last_output: Option<Instant>, now: Instant) -> bool {
    now.duration_since(started) >= SETTLE && last_output.is_some_and(|t| now.duration_since(t) >= QUIET)
}
```

If `MouseProtocolMode` does not derive `PartialEq`, use `matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)` instead.

- [ ] **Step 4: Run the tests**

Run: `cargo test --test chat`
Expected: PASS. If `exit_is_observed` reports `Some(u32::MAX)`, the reader thread raced `wait()`; check that `child` was moved into the thread (it is in the code above) before changing anything else.

- [ ] **Step 5: Commit**

```bash
git add src/tui/chat tests/chat.rs
git commit -m "feat(chat): run an agent in a PTY behind a vt100 screen"
```

---

### Task 5: The panel: layout, toggle, focus, key forwarding

**Files:**
- Modify: `src/tui/chat/mod.rs`, `src/tui/app.rs`, `src/tui/mod.rs`, `src/tui/keys.rs`, `tests/chat.rs`, `tests/golden/apply-180.txt`, `tests/golden/apply-120.txt` (help line)

**Interfaces:**
- Consumes: `Session` (Task 4), `AgentKind` (Task 2), `render_screen` (Task 3), `encode_key` (Task 3)
- Produces:
  - `chat::room(row: u16, minimap: u16, fns_shown: bool) -> u16` (crate-visible)
  - `chat::Chat { kind: AgentKind, session: Option<Session>, error: Option<String>, queued: Option<(String, Instant)>, last_ref: Option<String>, .. }`, `Chat::new(AgentKind)`, `Chat::live(&self) -> bool`, `Chat::start(&mut self, argv: &[String], cwd: &Path, rows: u16, cols: u16)`, `Chat::write(&mut self, &[u8])`, `Chat::drain_wake(&self) -> bool`
  - `chat::width(want: Option<u16>, row: u16) -> u16`, `chat::draw(f, app, area)` (crate-visible)
  - `Pane::Chat`, `Divider::DiffChat`, `App.{show_chat, chat_pane, chat_width, chat, map_dir}`, `App::toggle_chat(&mut self) -> bool`
  - `keys::Action::StartChat`

- [ ] **Step 1: Write the failing tests** — append to `tests/chat.rs`:

```rust
use std::path::PathBuf;

use codemapx::tui::{self, App, Pane, keys::{self, Action}};

fn app() -> App {
    App::new(common::sample_map(), PathBuf::from("/wt"))
}

fn press(a: &mut App, code: KeyCode, m: KeyModifiers) -> Action {
    keys::handle(a, KeyEvent::new(code, m))
}

#[test]
fn a_shows_the_panel_focused_and_asks_for_an_agent() {
    let mut a = app();
    assert!(matches!(press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE), Action::StartChat));
    assert_eq!(a.focus, Pane::Chat);
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("chat · claude · ctrl-x leaves"), "{frame}");
    assert!(frame.contains("starting the agent"), "{frame}");
    assert_eq!(a.chat_pane.width, 57, "40% of 180 less the 20-column minimap and the functions panel's 16-column minimum");
    assert_eq!(a.chat_pane.right(), a.minimap.x, "sits between the diff and the minimap");
}

#[test]
fn ctrl_x_leaves_and_a_hides() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    tui::snapshot(&mut a, 180, 50);
    press(&mut a, KeyCode::Char('x'), KeyModifiers::CONTROL);
    assert_eq!(a.focus, Pane::Diff);
    assert!(matches!(press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE), Action::None));
    tui::snapshot(&mut a, 180, 50);
    assert_eq!(a.chat_pane.width, 0);
    // Hiding while focused hands focus to the diff.
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    a.focus = Pane::Chat;
    a.toggle_chat();
    assert_eq!(a.focus, Pane::Diff);
}

#[test]
fn focused_chat_without_an_agent_restarts_on_enter_and_keeps_other_keys() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(matches!(press(&mut a, KeyCode::Enter, KeyModifiers::NONE), Action::StartChat));
    assert!(matches!(press(&mut a, KeyCode::Char('q'), KeyModifiers::NONE), Action::Quit));
}

#[test]
fn focused_chat_forwards_quit_keys() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    tui::snapshot(&mut a, 180, 50);
    let chat = a.chat.as_mut().unwrap();
    chat.start(&sh("cat >/dev/null"), Path::new("/"), 5, 20);
    assert!(chat.live());
    for (code, m) in [(KeyCode::Char('q'), KeyModifiers::NONE), (KeyCode::Char('c'), KeyModifiers::CONTROL), (KeyCode::Esc, KeyModifiers::NONE), (KeyCode::Tab, KeyModifiers::NONE)] {
        assert!(matches!(press(&mut a, code, m), Action::None), "{code:?}");
        assert_eq!(a.focus, Pane::Chat, "{code:?}");
    }
}

#[test]
fn tab_reaches_the_chat_only_while_shown() {
    let mut a = app();
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Map, "hidden: diff wraps to the map");
    a.show_chat = true;
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Chat);
}

#[test]
fn chat_width_leaves_the_diff_forty_columns() {
    let mut a = app();
    a.show_chat = true;
    for w in [100u16, 120, 130, 150, 180] {
        tui::snapshot(&mut a, w, 50);
        let diff = a.panes[2].width - a.fns_pane.width - a.chat_pane.width - a.minimap.width;
        assert!(a.chat_pane.width >= 40 && diff >= 40, "w={w}: chat {} diff {diff}", a.chat_pane.width);
    }
}

#[test]
fn chat_errors_show_in_the_panel() {
    let mut a = app();
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    a.chat.as_mut().unwrap().start(&["/nope/agent".to_string()], Path::new("/"), 5, 20);
    let frame = tui::snapshot(&mut a, 180, 50);
    assert!(frame.contains("can't run /nope/agent"), "{frame}");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test chat`
Expected: compile errors: no `Pane::Chat`, `Action::StartChat`, `App.chat`, etc.

- [ ] **Step 3: Implement `Chat`, `width`, `draw`** — `src/tui/chat/mod.rs` becomes:

```rust
//! The agent chat panel: an agent's own TUI in a PTY beside the diff, briefed on the branch.

pub mod agent;
pub mod briefing;
pub mod keys;
pub mod pty;
pub mod render;

use std::{path::Path, sync::mpsc, time::Instant};

use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use super::{DIM, app::{App, Pane}, fns_pane::MIN_DIFF_WIDTH, pane_block};
use agent::AgentKind;
use pty::Session;

/// Narrowest the panel gets, borders included.
const MIN_WIDTH: u16 = 40;

/// The panel's width out of `room` columns (the diff row less the minimap and the functions panel's minimum): 40% by default, leaving the diff `MIN_DIFF_WIDTH`.
pub(crate) fn width(want: Option<u16>, room: u16) -> u16 {
    want.unwrap_or(room * 2 / 5).clamp(MIN_WIDTH, room.saturating_sub(MIN_DIFF_WIDTH).max(MIN_WIDTH))
}

pub struct Chat {
    pub kind: AgentKind,
    pub session: Option<Session>,
    /// Why the agent couldn't start; shown in place of it.
    pub error: Option<String>,
    /// A reference waiting for the composer, and when it was queued.
    pub queued: Option<(String, Instant)>,
    /// The last reference typed, so refocusing on the same lines doesn't repeat it.
    pub last_ref: Option<String>,
    wake_tx: mpsc::Sender<()>,
    wake: mpsc::Receiver<()>,
}

impl Chat {
    pub fn new(kind: AgentKind) -> Self {
        let (wake_tx, wake) = mpsc::channel();
        Chat { kind, session: None, error: None, queued: None, last_ref: None, wake_tx, wake }
    }

    pub fn live(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.exit_code().is_none())
    }

    /// Replaces any earlier agent with a fresh one; a spawn failure is kept in `error`.
    pub fn start(&mut self, argv: &[String], cwd: &Path, rows: u16, cols: u16) {
        self.session = None;
        self.last_ref = None;
        match Session::spawn(argv, cwd, rows, cols, self.wake_tx.clone()) {
            Ok(s) => {
                self.session = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Writes to the agent and returns its view to the live screen.
    pub fn write(&mut self, bytes: &[u8]) {
        if let Some(s) = &mut self.session {
            s.scroll_to_live();
            s.write(bytes);
        }
    }

    /// True when the agent produced output since the last call.
    pub fn drain_wake(&self) -> bool {
        self.wake.try_iter().count() > 0
    }
}

pub(crate) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Chat;
    let mut title = match &app.chat {
        Some(c) => format!(" chat · {} ", c.kind.name()),
        None => " chat ".into(),
    };
    if focused {
        title.push_str("· ctrl-x leaves ");
    }
    let block = pane_block(Line::from(title), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let note = |f: &mut Frame, t: String| f.render_widget(Paragraph::new(Span::styled(t, Style::default().fg(DIM))).wrap(Wrap { trim: true }), inner);
    let Some(chat) = app.chat.as_mut() else { return note(f, "starting the agent…".into()) };
    if let Some(e) = &chat.error {
        return note(f, format!("{e} — ⏎ to retry"));
    }
    let Some(s) = chat.session.as_mut() else { return note(f, "starting the agent…".into()) };
    if let Some(code) = s.exit_code() {
        return note(f, format!("agent exited (code {code}) — ⏎ to restart"));
    }
    s.resize(inner.height.max(1), inner.width.max(1));
    let p = s.parser();
    let screen = p.screen();
    render::render_screen(screen, f.buffer_mut(), inner);
    if focused && screen.scrollback() == 0 && !screen.hide_cursor() {
        let (r, c) = screen.cursor_position();
        f.set_cursor_position((inner.x + c, inner.y + r));
    }
}
```

- [ ] **Step 4: App state** — in `src/tui/app.rs`:

Add `Chat` to the `Pane` enum after `Diff`, and update the cycle order:

```rust
pub const PANES: [Pane; 7] = [Pane::Map, Pane::From, Pane::Inside, Pane::To, Pane::Functions, Pane::Diff, Pane::Chat];
```

Add `DiffChat` to `Divider` (after `FnsDiff`) and update its doc to "…or between the diff and a panel beside it."

Add `use super::chat::{Chat, agent::AgentKind};` to the imports. Add fields at the end of `App` (before `drag` is fine):

```rust
    /// `a` shows or hides the agent chat beside the diff.
    pub show_chat: bool,
    /// The chat panel as last drawn (empty when hidden).
    pub chat_pane: Rect,
    /// Chat width set by dragging its border; None uses the default.
    pub chat_width: Option<u16>,
    /// Made the first time the chat is shown; holds the agent until quit.
    pub chat: Option<Chat>,
    /// The map's state dir, so the chat briefing can point at facts.json and annotations.json.
    pub map_dir: Option<PathBuf>,
```

and in `App::new`: `show_chat: false, chat_pane: Rect::default(), chat_width: None, chat: None, map_dir: None,`.

In `move_pane`, add the arm `Pane::Chat => {}`; in `enter`, change `Pane::Map | Pane::Diff => {}` to `Pane::Map | Pane::Diff | Pane::Chat => {}`.

Add after `toggle_fns`:

```rust
    /// `a`: shows and focuses the chat, or hides it (focusing the diff if it had focus); true when an agent should start.
    pub fn toggle_chat(&mut self) -> bool {
        self.show_chat = !self.show_chat;
        if !self.show_chat {
            if self.focus == Pane::Chat {
                self.focus = Pane::Diff;
            }
            return false;
        }
        self.focus = Pane::Chat;
        let chat = self.chat.get_or_insert_with(|| Chat::new(AgentKind::from_env(std::env::var("CODEMAPX_AGENT").ok().as_deref()).unwrap_or(AgentKind::Claude)));
        chat.session.is_none() && chat.error.is_none()
    }
```

- [ ] **Step 5: Layout** — in `src/tui/mod.rs` `draw`:

After `app.minimap = Rect::default();` add `app.chat_pane = Rect::default();`. In the too-narrow branch change the focus check to `if matches!(app.focus, Pane::Functions | Pane::Chat) {`. Replace the diff-row block (from `let mm_w` through `diff_pane::draw(f, app, diff);`) with:

```rust
    let mm_w = if app.show_minimap { minimap::width(app.minimap_width, row.width) } else { 0 };
    let fns_on = app.show_fns && area.width >= fns_pane::MIN_TERM_WIDTH;
    let chat_w = if app.show_chat { chat::width(app.chat_width, chat::room(row.width, mm_w, fns_on)) } else { 0 };
    let fns_w = if fns_on { fns_pane::width(app.fns_width, row.width - mm_w - chat_w) } else { 0 };
    if fns_w == 0 && app.focus == Pane::Functions {
        app.focus = Pane::Diff;
    }
    let [fns, diff, chat_area, mm] = Layout::horizontal([Constraint::Length(fns_w), Constraint::Min(0), Constraint::Length(chat_w), Constraint::Length(mm_w)]).areas(row);
    if fns_w > 0 {
        app.fns_pane = fns;
        fns_pane::draw(f, app, fns);
    }
    if mm_w > 0 {
        app.minimap = mm;
        minimap::draw(f, app, mm, diff.height.saturating_sub(2) as usize);
    }
    if chat_w > 0 {
        app.chat_pane = chat_area;
        chat::draw(f, app, chat_area);
    }
    diff_pane::draw(f, app, diff);
```

In the help string, insert `a chat  ` after `m minimap  `.

Add to `src/tui/chat/mod.rs`, after `width`:

```rust
/// Columns the chat may share with the diff: the row less the minimap and, when shown, the functions panel's minimum.
pub(crate) fn room(row: u16, minimap: u16, fns_shown: bool) -> u16 {
    row - minimap - if fns_shown { fns_pane::MIN_WIDTH } else { 0 }
}
```

and import `fns_pane` there (`use super::{DIM, app::{App, Pane}, fns_pane::{self, MIN_DIFF_WIDTH}, pane_block};`). In `src/tui/fns_pane.rs` make `MIN_WIDTH` `pub(super)`.

Check: at 100 columns the functions panel is hidden, room is 80, the chat clamps to 40 and the diff gets 40. At 130, room is 94, the chat is 40, the functions panel gets `fns_pane::width(_, 70)` = 30 and the diff 40. At 180 a chat dragged to its maximum (104) leaves the functions panel 16 and the diff 40.

- [ ] **Step 6: Key routing** — in `src/tui/keys.rs`:

```rust
use super::{app::{App, PANES, Pane}, chat};

pub enum Action {
    None,
    Quit,
    Open(PathBuf, usize),
    /// Start (or restart) the chat agent once the panel has been drawn at its size.
    StartChat,
}
```

At the top of `handle`, after `app.flash = None;`:

```rust
    if app.focus == Pane::Chat {
        if let Some(a) = chat_key(app, key) {
            return a;
        }
    }
```

Add the match arm (next to `'m'`):

```rust
        KeyCode::Char('a') => {
            if app.toggle_chat() {
                return Action::StartChat;
            }
        }
```

Add:

```rust
/// The focused chat sends a live agent every key but ctrl-x; with no agent, ⏎ starts one and other keys fall through (None).
fn chat_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.focus = Pane::Diff;
        return Some(Action::None);
    }
    match app.chat.as_mut() {
        Some(c) if c.live() => {
            c.write(&chat::keys::encode_key(key));
            Some(Action::None)
        }
        _ if key.code == KeyCode::Enter => Some(Action::StartChat),
        _ => None,
    }
}
```

In `cycle`, change the skip condition to:

```rust
        let hidden = (PANES[p] == Pane::Functions && app.fns_pane.width == 0) || (PANES[p] == Pane::Chat && app.chat_pane.width == 0);
        if !hidden {
            break;
        }
```

and update its doc to "…skipping the functions and chat panels when they aren't on screen."

`src/tui/mod.rs` `event_loop` must compile: add `Action::StartChat => {}` to its match for now (Task 8 wires it).

- [ ] **Step 7: Run all tests; refresh goldens for the help line**

Run: `cargo test`
Expected: `tests/chat.rs` passes; `snapshot_apply_180`/`snapshot_apply_120_collapses` fail only on the help line. Then `UPDATE_GOLDEN=1 cargo test && git diff tests/golden` — the only change must be `a chat` in the last line. Run `cargo test` again: all PASS.

- [ ] **Step 8: Commit**

```bash
git add src/tui tests/chat.rs tests/golden
git commit -m "feat(tui): chat panel between the diff and the minimap"
```

---

### Task 6: Mouse: focus, resize, wheel

**Files:**
- Modify: `src/tui/mouse.rs`, `tests/chat.rs`

**Interfaces:**
- Consumes: `App.{chat_pane, chat_width, chat}`, `Divider::DiffChat`, `chat::width`, `Session::{wheel_bytes, scroll_by, write}`

- [ ] **Step 1: Write the failing tests** — append to `tests/chat.rs`:

```rust
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

fn mouse_at(a: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    tui::mouse::handle(a, MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
}

fn chat_app() -> App {
    let mut a = app();
    a.show_chat = true;
    tui::snapshot(&mut a, 180, 50);
    a
}

#[test]
fn clicking_the_chat_focuses_it() {
    let mut a = chat_app();
    let c = a.chat_pane;
    mouse_at(&mut a, MouseEventKind::Down(MouseButton::Left), c.x + 5, c.y + 3);
    mouse_at(&mut a, MouseEventKind::Up(MouseButton::Left), c.x + 5, c.y + 3);
    assert_eq!(a.focus, Pane::Chat);
}

#[test]
fn dragging_the_chat_border_resizes_it_within_limits() {
    let mut a = chat_app();
    let (c, diff) = (a.chat_pane, a.panes[2]);
    let row = c.y + 2;
    let drag = |a: &mut App, from: u16, to: u16| {
        mouse_at(a, MouseEventKind::Down(MouseButton::Left), from, row);
        mouse_at(a, MouseEventKind::Drag(MouseButton::Left), to, row);
        mouse_at(a, MouseEventKind::Up(MouseButton::Left), to, row);
        tui::snapshot(a, 180, 50);
    };
    drag(&mut a, c.x, c.x - 10);
    assert_eq!(a.chat_pane.width, c.width + 10);
    assert_eq!(a.panes[2], diff);
    assert_eq!(a.focus, Pane::Map, "a border drag doesn't focus the chat");
    let x = a.chat_pane.x;
    drag(&mut a, x, 179);
    assert_eq!(a.chat_pane.width, 40);
    let x = a.chat_pane.x;
    drag(&mut a, x, 0);
    assert_eq!(a.chat_pane.width, 104, "180 less the minimap (20), the functions minimum (16) and the diff (40)");
    assert_eq!(a.fns_pane.width, 16);
    assert_eq!(a.panes[2].width - a.fns_pane.width - a.chat_pane.width - a.minimap.width, 40);
}

#[test]
fn the_wheel_over_an_empty_chat_leaves_the_diff_alone() {
    let mut a = chat_app();
    let c = a.chat_pane;
    mouse_at(&mut a, MouseEventKind::ScrollDown, c.x + 5, c.y + 3);
    assert_eq!(a.scroll, 0);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test chat`
Expected: FAIL: focus stays `Map`; width unchanged.

- [ ] **Step 3: Implement** — in `src/tui/mouse.rs`, update the module doc to mention the chat panel, add `chat` to the `super::{...}` import, then:

In `handle`, replace the `Down` arm:

```rust
        MouseEventKind::Down(MouseButton::Left) => {
            app.drag = grab(app, ev.column, ev.row);
            if app.drag.is_none() && !focus_chat(app, ev.column, ev.row) {
                jump_minimap(app, ev.column, ev.row);
            }
        }
```

and add to the `Drag` arm, before `None =>`:

```rust
            Some((Divider::DiffChat, offset)) => resize_chat(app, ev.column as i32 - offset),
```

In `wheel`, before the minimap check:

```rust
    if app.chat_pane.contains(at) {
        return wheel_chat(app, ev, d);
    }
```

In `grab`, after the `mm` line:

```rust
    let c = app.chat_pane;
    let chat = (c.width > 0 && row > c.y && row + 1 < c.bottom() && (col == c.x || col + 1 == c.x)).then(|| (Divider::DiffChat, col as i32 - c.x as i32));
```

and return `rows.or(fns).or(chat).or(mm)`.

In `resize`, change the early-return arm to `Divider::FnsDiff | Divider::DiffChat | Divider::DiffMinimap => return,`.

In `resize_fns`, the available width must exclude the chat: `row.width - app.minimap.width - app.chat_pane.width`.

Add:

```rust
/// A press inside the chat's borders focuses it.
fn focus_chat(app: &mut App, col: u16, row: u16) -> bool {
    let c = app.chat_pane;
    let inside = c.width > 0 && col > c.x && col + 1 < c.right() && row > c.y && row + 1 < c.bottom();
    if inside {
        app.focus = Pane::Chat;
    }
    inside
}

/// Moves the chat's left edge to `x`, clamped like drawing clamps it.
fn resize_chat(app: &mut App, x: i32) {
    let room = chat::room(app.panes[2].width, app.minimap.width, app.fns_pane.width > 0);
    app.chat_width = Some(chat::width(Some((app.chat_pane.right() as i32 - x).max(0) as u16), room));
}

/// The wheel goes to an agent that asked for mouse reports, else scrolls its scrollback.
fn wheel_chat(app: &mut App, ev: MouseEvent, d: isize) {
    let c = app.chat_pane;
    let Some(s) = app.chat.as_mut().and_then(|c| c.session.as_mut()) else { return };
    match s.wheel_bytes(d < 0, ev.column - c.x, ev.row - c.y) {
        Some(b) => s.write(&b),
        None => s.scroll_by(-d * WHEEL_LINES),
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: PASS, including the existing functions-panel drag tests in `tests/tui.rs` (the chat is hidden there, so `chat_pane.width` is 0).

- [ ] **Step 5: Commit**

```bash
git add src/tui/mouse.rs tests/chat.rs
git commit -m "feat(tui): click, drag and wheel the chat panel"
```

---

### Task 7: Prefill references

**Files:**
- Modify: `src/tui/chat/mod.rs`, `src/tui/app.rs`, `src/tui/keys.rs`, `src/tui/mouse.rs`, `tests/chat.rs`

**Interfaces:**
- Consumes: `Chat` (Task 5), `Session::{settled, parser, write}`, `agent::ready`, `keys::wrap_paste`
- Produces: `chat::reference(app: &App, from: Pane) -> Option<String>`, `Chat::queue(&mut self, r: String, now: Instant)`, `Chat::tick(&mut self, now: Instant)`, `App::note_focus(&mut self, prev: Pane)`

- [ ] **Step 1: Write the failing tests** — append to `tests/chat.rs`:

```rust
use codemapx::tui::chat::reference;

fn at(a: &mut App, id: &str) {
    let i = a.map.card_index(id).unwrap();
    a.select(i);
}

#[test]
fn references_follow_the_pane_focus_came_from() {
    let mut a = app();
    at(&mut a, "src/billing/mills.ts");
    tui::snapshot(&mut a, 180, 50);
    let f = &a.card().functions[0];
    assert_eq!(reference(&a, Pane::Functions), Some(format!("src/billing/mills.ts:{}-{} ", f.start, f.end)));
    at(&mut a, "src/billing/apply.ts");
    let o = &a.card().outline[0];
    assert_eq!(reference(&a, Pane::Inside), Some(format!("src/billing/apply.ts:{}-{} ", o.start, o.end)));
    a.jump_to_line(7);
    assert_eq!(reference(&a, Pane::Diff), Some("src/billing/apply.ts:7 ".into()));
    a.hl = None;
    a.scroll = 0;
    let r = reference(&a, Pane::Map).unwrap();
    assert!(r.starts_with("src/billing/apply.ts:1-"), "{r}");
}

#[test]
fn no_reference_for_missing_or_deleted_files() {
    let mut a = app();
    at(&mut a, "enqueue");
    assert_eq!(reference(&a, Pane::Diff), None);
    at(&mut a, "src/api/legacy.ts");
    assert_eq!(reference(&a, Pane::Diff), None);
}

#[test]
fn focusing_the_chat_queues_a_new_reference_once() {
    let mut a = app();
    at(&mut a, "src/billing/apply.ts");
    tui::snapshot(&mut a, 180, 50);
    a.focus = Pane::Diff;
    press(&mut a, KeyCode::Char('a'), KeyModifiers::NONE);
    let queued = a.chat.as_ref().unwrap().queued.clone().map(|(r, _)| r);
    assert!(queued.as_deref().is_some_and(|r| r.starts_with("src/billing/apply.ts:1-")), "{queued:?}");
    // Already typed: refocusing on the same lines queues nothing.
    let chat = a.chat.as_mut().unwrap();
    chat.last_ref = queued;
    chat.queued = None;
    press(&mut a, KeyCode::Char('x'), KeyModifiers::CONTROL);
    press(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(a.focus, Pane::Chat);
    assert!(a.chat.as_ref().unwrap().queued.is_none());
}

#[test]
fn a_queued_reference_is_typed_once_the_composer_is_up() {
    let mut chat = codemapx::tui::chat::Chat::new(AgentKind::Claude);
    chat.start(&sh("printf '\\033[?1049h'; exec cat"), Path::new("/"), 5, 40);
    chat.queue("src/a.ts:1-2 ".into(), Instant::now());
    let end = Instant::now() + Duration::from_secs(5);
    while chat.queued.is_some() && Instant::now() < end {
        chat.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(chat.queued.is_none());
    assert_eq!(chat.last_ref.as_deref(), Some("src/a.ts:1-2 "));
    let s = chat.session.as_ref().unwrap();
    let mut seen = false;
    for _ in 0..40 {
        seen = s.parser().screen().contents().contains("src/a.ts:1-2");
        if seen {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(seen, "echoed by the tty");
}

#[test]
fn a_stale_reference_is_dropped() {
    let mut chat = codemapx::tui::chat::Chat::new(AgentKind::Claude);
    chat.queue("src/a.ts ".into(), Instant::now() - Duration::from_secs(11));
    chat.tick(Instant::now());
    assert!(chat.queued.is_none());
    assert!(chat.last_ref.is_none());
}
```

`src/billing/apply.ts:7` must be a numbered line in the apply diff; if `jump_to_line(7)` leaves `hl` as None in the sample, pick another line that `a.lines` numbers (check `a.lines.iter().filter_map(|l| l.n)`).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test chat`
Expected: compile error: no `reference`, `queue`, `tick`.

- [ ] **Step 3: Implement** — in `src/tui/chat/mod.rs` add `use std::time::Duration;` (merge into the `std` import), `use crate::{facts::Status, map::CardKind};`, and:

```rust
/// How long a reference waits for the agent's composer before it is dropped.
const PREFILL_TIMEOUT: Duration = Duration::from_secs(10);

impl Chat {
    /// Keeps only the newest reference, and none that was just typed.
    pub fn queue(&mut self, r: String, now: Instant) {
        if self.last_ref.as_deref() != Some(r.as_str()) {
            self.queued = Some((r, now));
        }
    }

    /// Types the queued reference, without Enter, once the agent is settled and its composer is up.
    pub fn tick(&mut self, now: Instant) {
        let Some((r, at)) = &self.queued else { return };
        if now.duration_since(*at) > PREFILL_TIMEOUT {
            self.queued = None;
            return;
        }
        let Some(s) = &mut self.session else { return };
        if !(s.settled(now) && agent::ready(self.kind, s.parser().screen())) {
            return;
        }
        let r = r.clone();
        s.write(&keys::wrap_paste(&r));
        self.last_ref = Some(r);
        self.queued = None;
    }
}

/// `path:start-end ` for what the reviewer was on in `from`, the pane focus came from: its function or outline entry,
/// else the highlighted diff line, else the visible lines. None when there's no file at HEAD to point at.
pub fn reference(app: &App, from: Pane) -> Option<String> {
    let c = app.card();
    if c.kind == CardKind::Missing || c.binary || c.status == Some(Status::Deleted) {
        return None;
    }
    let path = c.path.as_deref()?;
    let picked = match from {
        Pane::Functions => app.fns.selected().map(|s| (c.functions[s].start, c.functions[s].end)),
        Pane::Inside => app.inside.selected().map(|s| (c.outline[s].start, c.outline[s].end)),
        _ => None,
    };
    let range = picked.or_else(|| app.hl.and_then(|h| app.lines.get(h)?.n).map(|n| (n, n))).or_else(|| {
        let rows = app.panes[2].height.saturating_sub(2).max(1) as usize;
        let mut ns = app.lines.iter().skip(app.scroll).take(rows).filter_map(|l| l.n);
        let first = ns.next()?;
        Some((first, ns.last().unwrap_or(first)))
    });
    Some(match range {
        Some((a, b)) if a == b => format!("{path}:{a} "),
        Some((a, b)) => format!("{path}:{a}-{b} "),
        None => format!("{path} "),
    })
}
```

In `src/tui/app.rs` add `use std::time::Instant;` and `chat` to the `super::chat` import (`use super::chat::{self, Chat, agent::AgentKind};`), then:

```rust
    /// Queues a reference for the chat when focus just moved into it from `prev`.
    pub fn note_focus(&mut self, prev: Pane) {
        if prev == Pane::Chat || self.focus != Pane::Chat {
            return;
        }
        let Some(r) = chat::reference(self, prev) else { return };
        if let Some(c) = self.chat.as_mut() {
            c.queue(r, Instant::now());
        }
    }
```

In `src/tui/keys.rs`, rename the existing `pub fn handle` to `fn dispatch` and add:

```rust
pub fn handle(app: &mut App, key: KeyEvent) -> Action {
    let prev = app.focus;
    let action = dispatch(app, key);
    app.note_focus(prev);
    action
}
```

In `src/tui/mouse.rs`, likewise rename `pub fn handle` to `fn dispatch` and add:

```rust
pub fn handle(app: &mut App, ev: MouseEvent) {
    let prev = app.focus;
    dispatch(app, ev);
    app.note_focus(prev);
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tui tests/chat.rs
git commit -m "feat(chat): prefill a reference to the lines under review"
```

---

### Task 8: Wire the event loop, start the agent, docs

**Files:**
- Modify: `src/tui/chat/mod.rs`, `src/tui/mod.rs`, `src/tui/keys.rs`, `src/main.rs`, `README.md`

**Interfaces:**
- Consumes: everything above
- Produces: `chat::start(app: &mut App)`, `keys::paste(app: &mut App, s: &str)`

- [ ] **Step 1: `chat::start`** — in `src/tui/chat/mod.rs` add `use std::env;` (merge into `std` import) and `use ratatui::layout::Margin;` if not covered by the prelude:

```rust
/// Starts the agent sized to the panel as last drawn, briefed on the map; CODEMAPX_AGENT picks it, CODEMAPX_AGENT_BIN its binary.
pub fn start(app: &mut App) {
    let inner = app.chat_pane.inner(Margin::new(1, 1));
    let brief = briefing::briefing(&app.map, app.map_dir.as_deref());
    let chat = app.chat.get_or_insert_with(|| Chat::new(AgentKind::Claude));
    match AgentKind::from_env(env::var("CODEMAPX_AGENT").ok().as_deref()) {
        Ok(k) => chat.kind = k,
        Err(e) => {
            chat.error = Some(e);
            return;
        }
    }
    let argv = agent::argv(chat.kind, env::var("CODEMAPX_AGENT_BIN").ok().as_deref(), &brief);
    chat.start(&argv, &app.root, inner.height.max(1), inner.width.max(1));
}
```

- [ ] **Step 2: Paste** — in `src/tui/keys.rs`:

```rust
/// A bracketed paste goes to a live agent in the focused chat; elsewhere it is ignored.
pub fn paste(app: &mut App, s: &str) {
    if app.focus == Pane::Chat {
        if let Some(c) = app.chat.as_mut().filter(|c| c.live()) {
            c.write(&chat::keys::wrap_paste(s));
        }
    }
}
```

- [ ] **Step 3: Event loop** — in `src/tui/mod.rs`: import `EnableBracketedPaste, DisableBracketedPaste` from `crossterm::event`, and `std::time::{Duration, Instant}`.

`init`: `execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;` — `restore`: `let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);`. Update `init`'s doc: "…; bracketed paste lets a paste reach the chat agent whole."

Replace `event_loop`:

```rust
/// Blocks on input until an agent is running; then polls every 16 ms and redraws only on input or agent output.
fn event_loop(term: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut dirty = true;
    loop {
        if dirty {
            term.draw(|f| draw(f, app))?;
        }
        dirty = true;
        if let Some(c) = app.chat.as_mut().filter(|c| c.live()) {
            c.tick(Instant::now());
            let woke = c.drain_wake();
            if !event::poll(Duration::from_millis(16))? {
                // An exit also wakes, and the next draw shows it; after that the loop blocks again.
                dirty = woke || !c.live();
                continue;
            }
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            Event::Mouse(m) => {
                mouse::handle(app, m);
                continue;
            }
            Event::Paste(s) => {
                keys::paste(app, &s);
                continue;
            }
            _ => continue,
        };
        match keys::handle(app, key) {
            Action::Quit => return Ok(()),
            Action::StartChat => {
                // Draw first so the panel's size is known.
                term.draw(|f| draw(f, app))?;
                chat::start(app);
            }
            Action::Open(path, line) => {
                restore();
                let (t, e) = (env::var("CODEMAPX_EDITOR").ok(), env::var("EDITOR").ok());
                let res = open_editor(&path, line, t.as_deref(), e.as_deref());
                *term = init()?;
                term.clear()?;
                if let Err(e) = res {
                    app.flash = Some(format!(" {e}"));
                }
            }
            Action::None => {}
        }
    }
}
```

Borrow note: `c` borrows `app.chat` mutably only inside the `if let`; it ends before `event::read`.

- [ ] **Step 4: `map_dir`** — in `src/main.rs` `cmd_view`, after `let mut app = tui::App::new(...)`:

```rust
    app.map_dir = Some(loaded.dir.clone());
```

- [ ] **Step 5: README** — in the Keys table add, after the `f` row:

```markdown
| `a` | show or hide the agent chat between the diff and the minimap: claude (or codex) running in its own TUI, read-only, briefed on the branch's map; focusing it types a `path:start-end` reference for the lines you were on |
| `ctrl-x` | in the chat, return focus to the diff (every other key goes to the agent) |
```

and change the mouse-drag row to "…between the diff and the functions panel, chat or minimap…". After the `o` paragraph add:

```markdown
The chat runs `claude` by default; `CODEMAPX_AGENT=codex` runs codex instead, and `CODEMAPX_AGENT_BIN` points at another binary. The agent starts the first time you press `a`, lives until codemapx quits, and keeps nothing afterwards.
```

- [ ] **Step 6: Run the suite and clippy**

Run: `cargo test && cargo clippy --all-targets`
Expected: all PASS; no new clippy warnings in `src/tui/chat` (fix any it raises).

- [ ] **Step 7: Manual check in a real worktree** — `cargo build --release`, then in this worktree (after `/codemapx` has produced a map, or in any worktree with a map):

1. `codemapx` → `a`: the panel opens focused, claude's TUI appears, and within a few seconds `path:a-b ` is in its composer.
2. Ask "what does this change do?" → an answer that cites the briefing. Ctrl-x → diff focused; `f`/Tab into functions, select one, Tab to the chat → its range is typed.
3. Drag the diff/chat border; wheel over the chat; paste a multi-line block (it arrives as one paste).
4. `/exit` in claude → panel says `agent exited (code 0) — ⏎ to restart`; ⏎ restarts.
5. `CODEMAPX_AGENT=codex codemapx` → same with codex. `CODEMAPX_AGENT_BIN=/nope codemapx` → `can't run /nope: …` in the panel.
6. `q` from the diff quits and no `claude`/`codex` process is left (`pgrep -fl 'claude|codex'`).

Note anything that fails in the commit message or report it; don't paper over it.

- [ ] **Step 8: Commit**

```bash
git add src README.md
git commit -m "feat(tui): run the chat agent from the event loop"
```
