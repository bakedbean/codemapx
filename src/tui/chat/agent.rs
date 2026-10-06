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
