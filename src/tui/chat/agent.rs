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

/// Claude's composer is up on the alternate screen as a `❯` row under a `─` rule (its trust dialog has `❯` rows
/// but no rule above them); codex's is a `›` row with the cursor visible (its trust dialog hides the cursor).
pub fn ready(kind: AgentKind, screen: &vt100::Screen) -> bool {
    match kind {
        AgentKind::Claude => {
            let (rows, cols) = screen.size();
            let row = |r| screen.contents_between(r, 0, r, cols);
            screen.alternate_screen() && (1..rows).any(|r| row(r).trim_start().starts_with('❯') && row(r - 1).trim_start().starts_with('─'))
        }
        AgentKind::Codex => {
            let (rows, cols) = screen.size();
            !screen.hide_cursor() && (0..rows).any(|r| screen.contents_between(r, 0, r, cols).trim_start().starts_with('›'))
        }
    }
}
