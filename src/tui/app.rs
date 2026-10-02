//! TUI state and navigation over a merged `Map`.

use std::path::PathBuf;

use ratatui::{layout::Rect, widgets::ListState};

use super::map_pane::{self, ColumnView};
use crate::{
    diff::{hunk_first, hunk_start},
    facts::Status,
    map::{Card, CardKind, Map},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pane {
    Map,
    From,
    Inside,
    To,
    Functions,
    Diff,
}
pub const PANES: [Pane; 6] = [Pane::Map, Pane::From, Pane::Inside, Pane::To, Pane::Functions, Pane::Diff];

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
    pub fns: ListState,
    pub lines: Vec<DLine>,
    pub scroll: usize,
    pub hl: Option<usize>,
    pub diff_full: bool,
    pub flash: Option<String>,
    pub show_hidden: bool,
    /// Per-column `c` choices, by column index; None follows `column_layout`.
    pub col_overrides: Vec<Option<ColumnView>>,
    /// Width of the last drawn map, so `c` can flip what is on screen.
    pub map_width: u16,
    /// Map and middle-row heights set by dragging a border; None sizes them automatically.
    pub heights: Option<(u16, u16)>,
    /// Map, middle row and diff as last drawn, so mouse rows can be hit-tested.
    pub panes: [Rect; 3],
    /// From, Inside and To as last drawn (empty when hidden), so the wheel can find the pane under it.
    pub mid_panes: [Rect; 3],
    /// `f` shows or hides the functions panel; narrow terminals hide it regardless.
    pub show_fns: bool,
    /// The functions panel as last drawn (empty when hidden).
    pub fns_pane: Rect,
    /// Functions panel width set by dragging its border; None uses the default.
    pub fns_width: Option<u16>,
    /// The border being dragged, and the grab row's offset from it.
    pub drag: Option<(Divider, i32)>,
}

/// A draggable border: above the middle row, above the diff, or between the functions panel and the diff.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Divider {
    MapMid,
    MidDiff,
    FnsDiff,
}

impl App {
    pub fn new(map: Map, root: PathBuf) -> Self {
        let first = map.trail.first().copied().unwrap_or(0);
        let ncols = map.columns.len();
        let mut app = App {
            map,
            root,
            behind: None,
            cur: 0,
            focus: Pane::Map,
            from: ListState::default(),
            to: ListState::default(),
            inside: ListState::default(),
            fns: ListState::default(),
            lines: vec![],
            scroll: 0,
            hl: None,
            diff_full: false,
            flash: None,
            show_hidden: false,
            col_overrides: vec![None; ncols],
            map_width: map_pane::FULL_WIDTH,
            heights: None,
            fns_width: None,
            panes: [Rect::default(); 3],
            mid_panes: [Rect::default(); 3],
            show_fns: true,
            fns_pane: Rect::default(),
            drag: None,
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
        self.fns.select(if self.card().functions.is_empty() { None } else { Some(0) });
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
            CardKind::Changed => match &c.source {
                Some(src) => whole_file(&c.diff, src),
                None => numbered(&c.diff),
            },
        }
    }

    pub fn jump_to_line(&mut self, line: usize) {
        if let Some(i) = self.lines.iter().position(|l| l.n == Some(line)) {
            self.scroll = i.saturating_sub(2);
            self.hl = Some(i);
        }
    }

    /// Highlights the first diff line inside `start..=end`, or nothing when the diff doesn't show that range.
    fn jump_to_range(&mut self, start: usize, end: usize) {
        match self.lines.iter().position(|l| l.n.is_some_and(|n| n >= start && n <= end)) {
            Some(i) => {
                self.scroll = i.saturating_sub(2);
                self.hl = Some(i);
            }
            None => self.hl = None,
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
        self.move_pane(self.focus, d);
    }

    /// ↑/↓ in `pane`, focused or not: the mouse wheel moves the pane under the pointer.
    pub fn move_pane(&mut self, pane: Pane, d: isize) {
        match pane {
            Pane::Map => {
                let cards = &self.map.columns[self.card().column].cards;
                let p = cards.iter().position(|&i| i == self.cur).unwrap_or(0) as isize;
                let i = cards[(p + d).clamp(0, cards.len() as isize - 1) as usize];
                if i != self.cur {
                    self.select(i);
                }
            }
            Pane::From | Pane::To => {
                let len = self.links(pane == Pane::From).len();
                let st = if pane == Pane::From { &mut self.from } else { &mut self.to };
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
            Pane::Functions => {
                let len = self.card().functions.len();
                if len > 0 {
                    let s = (self.fns.selected().unwrap_or(0) as isize + d).clamp(0, len as isize - 1) as usize;
                    self.fns.select(Some(s));
                    let f = &self.card().functions[s];
                    let (start, end) = (f.start, f.end);
                    self.jump_to_range(start, end);
                }
            }
            Pane::Diff => self.scroll_by(d),
        }
    }

    /// Moves the map selection to the next non-empty column in direction `d`, keeping the row where it can.
    /// Context cards sit outside the trail, so this is how ←/→-only users reach them.
    pub fn move_column(&mut self, d: isize) {
        let cols = &self.map.columns;
        let c = self.card().column;
        let row = cols[c].cards.iter().position(|&i| i == self.cur).unwrap_or(0);
        let mut n = c as isize + d;
        while (0..cols.len() as isize).contains(&n) {
            let cards = &cols[n as usize].cards;
            if let Some(&i) = cards.get(row).or(cards.last()) {
                self.select(i);
                return;
            }
            n += d;
        }
    }

    pub fn column_views(&self, width: u16) -> Vec<ColumnView> {
        let names: Vec<&str> = self.map.columns.iter().map(|c| c.name.as_str()).collect();
        let auto = map_pane::column_layout(&names, width, self.show_hidden).unwrap_or_else(|| vec![ColumnView::Expanded; names.len()]);
        auto.into_iter().zip(&self.col_overrides).map(|(a, o)| o.unwrap_or(a)).collect()
    }

    /// Collapses or expands the column holding the selected card.
    pub fn toggle_column(&mut self) {
        let c = self.card().column;
        self.col_overrides[c] = Some(match self.column_views(self.map_width)[c] {
            ColumnView::Expanded => ColumnView::Collapsed,
            ColumnView::Collapsed => ColumnView::Expanded,
        });
    }

    /// `t`: swaps tests/docs with the rest, dropping `c` choices on tests/docs so the swap shows.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        for (o, col) in self.col_overrides.iter_mut().zip(&self.map.columns) {
            if map_pane::hideable(&col.name) {
                *o = None;
            }
        }
    }

    /// `f`: hides or shows the functions panel; hiding it while focused focuses the diff.
    pub fn toggle_fns(&mut self) {
        self.show_fns = !self.show_fns;
        if !self.show_fns && self.focus == Pane::Functions {
            self.focus = Pane::Diff;
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
            Pane::Inside | Pane::Functions => self.focus = Pane::Diff,
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
    /// the outline entry in the inside pane, the function in the functions panel, else the top visible diff line.
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
                } else if self.focus == Pane::Functions {
                    self.fns.selected().map(|s| c.functions[s].start)
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

/// The whole HEAD file with the diff's deletions and additions in place; hunk headers are dropped.
fn whole_file(diff: &str, src: &str) -> Vec<DLine> {
    let src: Vec<&str> = src.lines().collect();
    let mut out = vec![];
    let mut n = 1usize;
    let ctx_to = |out: &mut Vec<DLine>, n: &mut usize, end: usize| {
        while *n < end && *n <= src.len() {
            out.push(DLine { n: Some(*n), kind: Kind::Ctx, text: format!(" {}", src[*n - 1]) });
            *n += 1;
        }
    };
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("@@ ") {
            ctx_to(&mut out, &mut n, hunk_first(rest));
        } else if l.starts_with('-') {
            out.push(DLine { n: None, kind: Kind::Del, text: l.into() });
        } else if !l.starts_with('\\') {
            let kind = if l.starts_with('+') { Kind::Add } else { Kind::Ctx };
            out.push(DLine { n: Some(n), kind, text: l.into() });
            n += 1;
        }
    }
    ctx_to(&mut out, &mut n, usize::MAX);
    out
}
