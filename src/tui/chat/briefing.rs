//! What the chat agent is told at launch: the map as text, no diffs, so it stays a few KB.

use std::{fmt::Write, path::Path};

use crate::map::{Card, CardKind, Map};

fn label(c: &Card) -> &str {
    c.path.as_deref().unwrap_or(&c.name)
}

/// `wsx` is the branch's wsx workspace (`repo/slug`), if any; it adds how to hand changes to its primary agent.
pub fn briefing(map: &Map, map_dir: Option<&Path>, wsx: Option<&str>) -> String {
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
    if let Some(ws) = wsx {
        let _ = writeln!(s, "\n## Handing changes to the workspace agent\n");
        let _ = writeln!(s, "This branch is wsx workspace `{ws}`; its primary agent edits it. When the user asks you to hand a change over, run:\n");
        let _ = writeln!(s, "`{} '<instruction>'`\n", super::wsx::send_prefix(ws));
        s.push_str("- The instruction must stand on its own: the file and lines, the current text, the replacement, and why. Send only what the user agreed to, once.\n");
        s.push_str("- Keep it in single quotes, writing each `'` inside it as `'\\''`, so `$`, backticks and `\\` reach the agent literally.\n");
        s.push_str("- The send only queues the message: tell the user it was queued, not done, and relay any error or warning it prints (one about no running dashboard means wsx must be open to deliver it). Don't retry on failure or wait for a reply.\n");
    }
    s
}
