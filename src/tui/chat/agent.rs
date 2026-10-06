//! The agents the chat panel runs: how to launch each read-only with the briefing, and when its composer takes input.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
}

/// Pre-approved claude tools: file reads only. Git prompts too: `git diff --output=<path>` writes files.
const CLAUDE_TOOLS: [&str; 3] = ["Read", "Grep", "Glob"];

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
            v.extend(["--permission-mode", "default", "--allowedTools"].map(String::from));
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

/// The agent's input box as read off its screen.
#[derive(Debug, PartialEq)]
pub struct Composer {
    /// What follows the prompt glyph, trimmed; a placeholder hint when `empty`.
    pub text: String,
    /// The cursor sits right after the prompt glyph, so anything shown is a placeholder.
    pub empty: bool,
}

/// Claude's box is a `❯` row under a `─` rule (its trust dialog has `❯` rows but no rule above them);
/// codex's is a `›` row. None when neither is on screen.
pub fn composer(kind: AgentKind, screen: &vt100::Screen) -> Option<Composer> {
    let (rows, cols) = screen.size();
    let row = |r| screen.contents_between(r, 0, r, cols);
    let (glyph, from) = match kind {
        AgentKind::Claude => ('❯', 1),
        AgentKind::Codex => ('›', 0),
    };
    let r = (from..rows).find(|&r| row(r).trim_start().starts_with(glyph) && (kind == AgentKind::Codex || row(r - 1).trim_start().starts_with('─')))?;
    let line = row(r);
    let indent = line.chars().take_while(|c| c.is_whitespace()).count() as u16;
    let text = line.trim_start().trim_start_matches(glyph).trim().to_string();
    Some(Composer { text, empty: screen.cursor_position() == (r, indent + 2) })
}

/// Claude's composer is up on the alternate screen; codex's needs the cursor visible (its trust dialog hides it).
pub fn ready(kind: AgentKind, screen: &vt100::Screen) -> bool {
    let up = match kind {
        AgentKind::Claude => screen.alternate_screen(),
        AgentKind::Codex => !screen.hide_cursor(),
    };
    up && composer(kind, screen).is_some()
}
