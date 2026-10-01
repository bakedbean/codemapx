//! TUI state and navigation over a merged `Map`.

use std::path::PathBuf;

use ratatui::widgets::ListState;

use crate::{
    diff::hunk_start,
    facts::Status,
    map::{Card, CardKind, Map},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pane {
    Map,
    From,
    Inside,
    To,
    Diff,
}
pub const PANES: [Pane; 5] = [Pane::Map, Pane::From, Pane::Inside, Pane::To, Pane::Diff];

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Hunk,
    Add,
    Del,
    Ctx,
    Note,
}

pub struct DLine {
    pub n: Option<usize>,
    pub kind: Kind,
    pub text: String,
}

pub enum Rel {
    Sel,
    From,
    To,
    None,
}

pub struct App {
    pub map: Map,
    pub root: PathBuf,
    pub behind: Option<usize>,
    pub cur: usize,
    pub focus: Pane,
    pub from: ListState,
    pub to: ListState,
    pub inside: ListState,
    pub lines: Vec<DLine>,
    pub scroll: usize,
    pub hl: Option<usize>,
    pub diff_full: bool,
    pub flash: Option<String>,
    pub show_hidden: bool,
    map_order: Vec<usize>,
}

impl App {
    pub fn new(map: Map, root: PathBuf) -> Self {
        let map_order = map.columns.iter().flat_map(|c| c.cards.iter().copied()).collect();
        let first = map.trail.first().copied().unwrap_or(0);
        let mut app = App {
            map,
            root,
            behind: None,
            cur: 0,
            focus: Pane::Map,
            from: ListState::default(),
            to: ListState::default(),
            inside: ListState::default(),
            lines: vec![],
            scroll: 0,
            hl: None,
            diff_full: false,
            flash: None,
            show_hidden: false,
            map_order,
        };
        app.select(first);
        app
    }

    pub fn card(&self) -> &Card {
        &self.map.cards[self.cur]
    }

    /// Links into (`incoming`) or out of the selected card, as (other card, link index).
    pub fn links(&self, incoming: bool) -> Vec<(usize, usize)> {
        self.map
            .links
            .iter()
            .enumerate()
            .filter_map(|(k, l)| match incoming {
                true if l.to == self.cur => Some((l.from, k)),
                false if l.from == self.cur => Some((l.to, k)),
                _ => None,
            })
            .collect()
    }

    pub fn rel(&self, i: usize) -> Rel {
        if i == self.cur {
            return Rel::Sel;
        }
        let ls = &self.map.links;
        if ls.iter().any(|l| l.from == i && l.to == self.cur) {
            Rel::From
        } else if ls.iter().any(|l| l.from == self.cur && l.to == i) {
            Rel::To
        } else {
            Rel::None
        }
    }

    pub fn select(&mut self, i: usize) {
        self.cur = i;
        self.from.select(Some(0));
        self.to.select(Some(0));
        self.inside.select(if self.card().outline.is_empty() { None } else { Some(0) });
        self.scroll = 0;
        self.hl = None;
        self.lines = self.build_lines();
    }

    fn build_lines(&self) -> Vec<DLine> {
        let c = self.card();
        let note = |t: &str| DLine { n: None, kind: Kind::Note, text: t.to_string() };
        match c.kind {
            CardKind::Context => vec![note("Not changed in this branch; shown because edits here depend on it."), note("Press o to open it.")],
            CardKind::Missing => vec![note("Not built yet."), note(&c.what)],
            CardKind::Changed if c.binary => vec![note("Binary file; no diff.")],
            CardKind::Changed if c.diff.is_empty() => vec![note("No content change (rename only).")],
            CardKind::Changed => numbered(&c.diff),
        }
    }

    pub fn jump_to_line(&mut self, line: usize) {
        if let Some(i) = self.lines.iter().position(|l| l.n == Some(line)) {
            self.scroll = i.saturating_sub(2);
            self.hl = Some(i);
        }
    }

    pub fn step(&mut self, d: isize) {
        let trail = &self.map.trail;
        if trail.is_empty() {
            return;
        }
        let next = match trail.iter().position(|&t| t == self.cur) {
            None => 0,
            Some(p) => (p as isize + d).clamp(0, trail.len() as isize - 1) as usize,
        };
        let i = trail[next];
        if i != self.cur {
            self.select(i);
        }
    }

    pub fn move_in(&mut self, d: isize) {
        match self.focus {
            Pane::Map => {
                let p = self.map_order.iter().position(|&i| i == self.cur).unwrap_or(0) as isize;
                let n = self.map_order.len() as isize;
                self.select(self.map_order[(p + d).rem_euclid(n) as usize]);
            }
            Pane::From | Pane::To => {
                let len = self.links(self.focus == Pane::From).len();
                let st = if self.focus == Pane::From { &mut self.from } else { &mut self.to };
                if len > 0 {
                    st.select(Some((st.selected().unwrap_or(0) as isize + d).clamp(0, len as isize - 1) as usize));
                }
            }
            Pane::Inside => {
                let len = self.card().outline.len();
                if len > 0 {
                    let s = (self.inside.selected().unwrap_or(0) as isize + d).clamp(0, len as isize - 1) as usize;
                    self.inside.select(Some(s));
                    let line = self.card().outline[s].start;
                    self.jump_to_line(line);
                }
            }
            Pane::Diff => self.scroll_by(d),
        }
    }

    pub fn scroll_by(&mut self, d: isize) {
        let max = self.lines.len().saturating_sub(1) as isize;
        self.scroll = (self.scroll as isize + d).clamp(0, max) as usize;
    }

    pub fn enter(&mut self) {
        match self.focus {
            Pane::From | Pane::To => {
                let incoming = self.focus == Pane::From;
                let st = if incoming { &self.from } else { &self.to };
                if let Some(&(i, _)) = self.links(incoming).get(st.selected().unwrap_or(0)) {
                    self.select(i);
                }
            }
            Pane::Inside => self.focus = Pane::Diff,
            Pane::Map | Pane::Diff => {}
        }
    }

    /// Stale-map warning shown under the header, if any.
    pub fn banner(&self) -> Option<String> {
        match self.behind {
            Some(n) if n > 0 => Some(format!("map is {n} commit{} behind HEAD — run /codemapx in the agent session", if n == 1 { "" } else { "s" })),
            Some(_) => Some("map was made for a different commit — run /codemapx in the agent session".into()),
            None if self.map.annotations_stale => Some("annotations were written for an older commit — run /codemapx to refresh them".into()),
            None => None,
        }
    }

    /// File and line `o` opens: the selected link's evidence in the came-from/leads-to panes,
    /// the outline entry in the inside pane, else the top visible diff line.
    pub fn editor_target(&self) -> Option<(PathBuf, usize)> {
        if matches!(self.focus, Pane::From | Pane::To) {
            let incoming = self.focus == Pane::From;
            let st = if incoming { &self.from } else { &self.to };
            let &(_, k) = self.links(incoming).get(st.selected().unwrap_or(0))?;
            let ev = &self.map.links[k].evidence;
            return Some((self.root.join(&ev.path), ev.line));
        }
        let c = self.card();
        let path = self.root.join(c.path.as_ref()?);
        match c.kind {
            CardKind::Missing => None,
            CardKind::Context => Some((path, 1)),
            CardKind::Changed if c.status == Some(Status::Deleted) => None,
            CardKind::Changed => {
                let line = if self.focus == Pane::Inside {
                    self.inside.selected().map(|s| c.outline[s].start)
                } else {
                    self.lines[self.scroll.min(self.lines.len())..].iter().find_map(|l| l.n)
                };
                Some((path, line.unwrap_or(1)))
            }
        }
    }
}

// Numbers context and added lines with their line in the branch's version of the file.
fn numbered(diff: &str) -> Vec<DLine> {
    let mut out = vec![];
    let mut n = 0usize;
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("@@ ") {
            n = hunk_start(rest);
            out.push(DLine { n: None, kind: Kind::Hunk, text: l.into() });
        } else if l.starts_with('-') || l.starts_with('\\') {
            out.push(DLine { n: None, kind: Kind::Del, text: l.into() });
        } else {
            let kind = if l.starts_with('+') { Kind::Add } else { Kind::Ctx };
            out.push(DLine { n: Some(n), kind, text: l.into() });
            n += 1;
        }
    }
    out
}
