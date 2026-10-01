//! Prototype codemapx TUI. Renders a change map (config + diffs) as a navigable file map.

use std::{collections::HashMap, env, fs, process::Command};

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;


const AMBER: Color = Color::Rgb(224, 177, 87);
const BLUE: Color = Color::Rgb(132, 162, 255);
const GREEN: Color = Color::Rgb(111, 207, 127);
const RED: Color = Color::Rgb(255, 138, 138);
const DIM: Color = Color::Rgb(110, 118, 132);
const FAINT: Color = Color::Rgb(70, 76, 88);
const SEL_BG: Color = Color::Rgb(38, 44, 58);
const HL_BG: Color = Color::Rgb(48, 58, 92);

#[derive(Deserialize)]
struct Config {
    title: String,
    eyebrow: String,
    #[serde(rename = "PREFIX")]
    prefix: String,
    /// Worktree the map describes; `o` opens files under it.
    #[serde(rename = "ROOT", default = "default_root")]
    root: String,
    #[serde(rename = "COLS")]
    cols: Vec<String>,
    #[serde(rename = "NODES")]
    nodes: Vec<Node>,
    #[serde(rename = "EDGES")]
    edges: Vec<(String, String, String)>,
    #[serde(rename = "TRAIL")]
    trail: Vec<String>,
}

fn default_root() -> String {
    ".".into()
}

#[derive(Deserialize)]
struct Node {
    id: String,
    col: usize,
    name: String,
    dir: String,
    #[serde(default)]
    paths: Vec<String>,
    what: String,
    #[serde(default)]
    outline: Vec<(String, usize, String)>,
    ghost: Option<String>,
    #[serde(rename = "ref")]
    reference: Option<String>,
}

#[derive(Deserialize)]
struct FileDiff {
    add: u32,
    del: u32,
    diff: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Map,
    From,
    Inside,
    To,
    Diff,
}
const PANES: [Pane; 5] = [Pane::Map, Pane::From, Pane::Inside, Pane::To, Pane::Diff];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    File,
    Hunk,
    Add,
    Del,
    Ctx,
    Note,
}

struct DLine {
    n: Option<usize>,
    kind: Kind,
    text: String,
}

enum Rel {
    Sel,
    From,
    To,
    None,
}

struct App {
    cfg: Config,
    diffs: HashMap<String, FileDiff>,
    idx: HashMap<String, usize>,
    map_order: Vec<usize>,
    cur: usize,
    focus: Pane,
    from: ListState,
    to: ListState,
    inside: ListState,
    lines: Vec<DLine>,
    scroll: usize,
    hl: Option<usize>,
    diff_full: bool,
    flash: Option<String>,
}

impl App {
    fn new(cfg: Config, diffs: HashMap<String, FileDiff>) -> Self {
        let idx = cfg.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
        let mut map_order: Vec<usize> = (0..cfg.nodes.len()).collect();
        map_order.sort_by_key(|&i| cfg.nodes[i].col);
        let mut app = App {
            cfg,
            diffs,
            idx,
            map_order,
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
        };
        let first = app.idx[&app.cfg.trail[0]];
        app.select(first);
        app
    }

    fn node(&self) -> &Node {
        &self.cfg.nodes[self.cur]
    }

    fn links(&self, incoming: bool) -> Vec<(usize, String)> {
        let id = &self.node().id;
        self.cfg
            .edges
            .iter()
            .filter(|(f, t, _)| if incoming { t == id } else { f == id })
            .map(|(f, t, why)| (self.idx[if incoming { f } else { t }], why.clone()))
            .collect()
    }

    fn rel(&self, i: usize) -> Rel {
        if i == self.cur {
            return Rel::Sel;
        }
        let id = &self.cfg.nodes[i].id;
        let cur = &self.node().id;
        if self.cfg.edges.iter().any(|(f, t, _)| f == id && t == cur) {
            Rel::From
        } else if self.cfg.edges.iter().any(|(f, t, _)| f == cur && t == id) {
            Rel::To
        } else {
            Rel::None
        }
    }

    fn select(&mut self, i: usize) {
        self.cur = i;
        self.from.select(Some(0));
        self.to.select(Some(0));
        self.inside.select(if self.node().outline.is_empty() { None } else { Some(0) });
        self.scroll = 0;
        self.hl = None;
        self.lines = self.build_lines();
    }

    // Numbers context/added lines with their line in the branch's version of the file.
    fn build_lines(&self) -> Vec<DLine> {
        let node = self.node();
        if let Some(r) = &node.reference {
            return vec![
                DLine { n: None, kind: Kind::File, text: r.clone() },
                DLine { n: None, kind: Kind::Note, text: "Not changed in this branch; shown because edits here depend on it.".into() },
                DLine { n: None, kind: Kind::Note, text: "Press o to open it.".into() },
            ];
        }
        let mut out = vec![];
        for p in &node.paths {
            if node.paths.len() > 1 {
                out.push(DLine { n: None, kind: Kind::File, text: p.strip_prefix(&self.cfg.prefix).unwrap_or(p).into() });
            }
            let mut n = 0usize;
            for l in self.diffs.get(p).map(|d| d.diff.as_str()).unwrap_or("").lines() {
                if let Some(rest) = l.strip_prefix("@@ ") {
                    n = rest
                        .split_whitespace()
                        .find_map(|t| t.strip_prefix('+'))
                        .and_then(|t| t.split(',').next())
                        .and_then(|t| t.parse().ok())
                        .unwrap_or(0);
                    out.push(DLine { n: None, kind: Kind::Hunk, text: l.into() });
                } else if l.starts_with('-') {
                    out.push(DLine { n: None, kind: Kind::Del, text: l.into() });
                } else {
                    let kind = if l.starts_with('+') { Kind::Add } else { Kind::Ctx };
                    out.push(DLine { n: Some(n), kind, text: l.into() });
                    n += 1;
                }
            }
        }
        out
    }

    fn jump_to_line(&mut self, line: usize) {
        if let Some(i) = self.lines.iter().position(|l| l.n == Some(line)) {
            self.scroll = i.saturating_sub(2);
            self.hl = Some(i);
        }
    }

    fn step(&mut self, d: isize) {
        let pos = self.cfg.trail.iter().position(|t| *t == self.node().id);
        let next = match pos {
            None => 0,
            Some(p) => (p as isize + d).clamp(0, self.cfg.trail.len() as isize - 1) as usize,
        };
        let i = self.idx[&self.cfg.trail[next]];
        if i != self.cur {
            self.select(i);
        }
    }

    fn move_in(&mut self, d: isize) {
        match self.focus {
            Pane::Map => {
                let p = self.map_order.iter().position(|&i| i == self.cur).unwrap_or(0) as isize;
                let n = self.map_order.len() as isize;
                self.select(self.map_order[((p + d).rem_euclid(n)) as usize]);
            }
            Pane::From | Pane::To => {
                let len = self.links(self.focus == Pane::From).len();
                let st = if self.focus == Pane::From { &mut self.from } else { &mut self.to };
                if len > 0 {
                    st.select(Some((st.selected().unwrap_or(0) as isize + d).clamp(0, len as isize - 1) as usize));
                }
            }
            Pane::Inside => {
                let len = self.node().outline.len();
                if len > 0 {
                    let s = (self.inside.selected().unwrap_or(0) as isize + d).clamp(0, len as isize - 1) as usize;
                    self.inside.select(Some(s));
                    let line = self.node().outline[s].1;
                    self.jump_to_line(line);
                }
            }
            Pane::Diff => self.scroll_by(d),
        }
    }

    fn scroll_by(&mut self, d: isize) {
        let max = self.lines.len().saturating_sub(1) as isize;
        self.scroll = (self.scroll as isize + d).clamp(0, max) as usize;
    }

    fn enter(&mut self) {
        match self.focus {
            Pane::From | Pane::To => {
                let incoming = self.focus == Pane::From;
                let st = if incoming { &self.from } else { &self.to };
                if let Some((i, _)) = self.links(incoming).get(st.selected().unwrap_or(0)) {
                    let i = *i;
                    self.select(i);
                }
            }
            Pane::Inside => self.focus = Pane::Diff,
            Pane::Map | Pane::Diff => {}
        }
    }

    /// The file and line `o` should open: the outline entry, else the top visible diff line.
    fn editor_target(&self) -> Option<(String, usize)> {
        let node = self.node();
        if let Some(r) = &node.reference {
            let spec = r.split(" · ").next()?;
            let (path, line) = match spec.rsplit_once(':') {
                Some((p, l)) => (p, l.parse().unwrap_or(1)),
                None => (spec, 1),
            };
            if !path.contains('/') {
                return None;
            }
            return Some((format!("{}/{}{path}", self.cfg.root, self.cfg.prefix), line));
        }
        let line = if self.focus == Pane::Inside {
            self.inside.selected().map(|s| node.outline[s].1)
        } else {
            self.lines[self.scroll..].iter().find_map(|l| l.n)
        };
        Some((format!("{}/{}", self.cfg.root, node.paths.first()?), line.unwrap_or(1)))
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn trunc(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.width() + 2 > w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Keeps the end of a file name, which is what tells siblings apart.
fn trunc_left(s: &str, w: usize) -> String {
    let s = s.trim_start_matches('…');
    if s.width() <= w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for c in chars.iter().rev() {
        if out.width() + 2 > w {
            break;
        }
        out.insert(0, *c);
    }
    format!("…{out}")
}

fn pane_block(title: Line<'static>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(if focused { Color::White } else { FAINT }))
        .title(title)
}

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let map_h = (0..app.cfg.cols.len())
        .map(|c| app.cfg.nodes.iter().filter(|n| n.col == c).count())
        .max()
        .unwrap_or(0) as u16
        + 3;
    let rows = if app.diff_full {
        Layout::vertical([Constraint::Length(1), Constraint::Length(0), Constraint::Length(0), Constraint::Min(3), Constraint::Length(1)]).split(area)
    } else {
        Layout::vertical([Constraint::Length(1), Constraint::Length(map_h), Constraint::Length(16), Constraint::Min(5), Constraint::Length(1)]).split(area)
    };

    // header
    let pos = app.cfg.trail.iter().position(|t| *t == app.node().id);
    let step = match pos {
        Some(p) => format!("step {}/{}", p + 1, app.cfg.trail.len()),
        None => app.node().ghost.clone().unwrap_or_default(),
    };
    let left = format!(" {}  ", app.cfg.title);
    let right = format!("{step} ");
    let mid = trunc(&app.cfg.eyebrow, (rows[0].width as usize).saturating_sub(left.width() + right.width() + 1));
    let pad = (rows[0].width as usize).saturating_sub(left.width() + mid.width() + right.width());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, Style::default().bold()),
            Span::styled(mid, Style::default().fg(DIM)),
            Span::raw(" ".repeat(pad)),
            Span::styled(right, Style::default().fg(BLUE)),
        ])),
        rows[0],
    );

    if !app.diff_full {
        draw_map(f, app, rows[1]);
        draw_middle(f, app, rows[2]);
    }
    draw_diff(f, app, rows[3]);

    let help = app.flash.clone().unwrap_or_else(|| {
        " ←/→ step   tab pane   ↑/↓ move   ⏎ follow   o open in $EDITOR   d full diff   J/K page   q quit".into()
    });
    f.render_widget(Paragraph::new(Span::styled(help, Style::default().fg(DIM))), rows[4]);
}

fn draw_map(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(Line::from(" map "), app.focus == Pane::Map);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let n = app.cfg.cols.len() as u32;
    let cols = Layout::horizontal((0..n).map(|_| Constraint::Ratio(1, n))).spacing(2).split(inner);
    for (c, title) in app.cfg.cols.iter().enumerate() {
        let w = cols[c].width as usize;
        let mut lines = vec![Line::from(Span::styled(trunc(&title.to_uppercase(), w), Style::default().fg(DIM)))];
        for (i, node) in app.cfg.nodes.iter().enumerate().filter(|(_, n)| n.col == c) {
            let (marker, color) = match app.rel(i) {
                Rel::Sel => ("▶ ", Color::White),
                Rel::From => ("◂ ", AMBER),
                Rel::To => ("▸ ", BLUE),
                Rel::None => ("  ", FAINT),
            };
            let stats = if node.ghost.is_some() {
                "┆".to_string()
            } else {
                let (a, d) = node.paths.iter().fold((0, 0), |(a, d), p| {
                    app.diffs.get(p).map(|x| (a + x.add, d + x.del)).unwrap_or((a, d))
                });
                if d > 0 { format!("+{a} −{d}") } else { format!("+{a}") }
            };
            let name_w = w.saturating_sub(2 + stats.width() + 1);
            let name = trunc_left(&node.name, name_w);
            let pad = w.saturating_sub(2 + name.width() + stats.width());
            let mut name_style = Style::default().fg(color);
            if node.ghost.is_some() {
                name_style = name_style.italic();
            }
            let mut row_style = Style::default();
            if i == app.cur {
                name_style = name_style.bold();
                row_style = row_style.bg(SEL_BG);
            }
            let unrelated = matches!(app.rel(i), Rel::None);
            let stat_style = |c: Color| Style::default().fg(if unrelated { FAINT } else { c });
            let mut spans = vec![Span::styled(marker, Style::default().fg(color)), Span::styled(name, name_style), Span::raw(" ".repeat(pad))];
            if node.ghost.is_some() {
                spans.push(Span::styled(stats, stat_style(DIM)));
            } else if let Some((a, d)) = stats.split_once(' ') {
                spans.push(Span::styled(a.to_string(), stat_style(GREEN)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(d.to_string(), stat_style(RED)));
            } else {
                spans.push(Span::styled(stats, stat_style(GREEN)));
            }
            lines.push(Line::from(spans).style(row_style));
        }
        f.render_widget(Paragraph::new(lines), cols[c]);
    }
}

fn link_items(app: &App, links: &[(usize, String)], color: Color, width: usize) -> Vec<ListItem<'static>> {
    if links.is_empty() {
        return vec![ListItem::new(Span::styled("Nothing in this branch.", Style::default().fg(DIM)))];
    }
    links
        .iter()
        .map(|(i, why)| {
            let node = &app.cfg.nodes[*i];
            let mut name = Span::styled(node.name.clone(), Style::default().fg(color).bold());
            if node.ghost.is_some() {
                name = name.italic();
            }
            let mut lines = vec![Line::from(vec![name, Span::styled(if node.ghost.is_some() { " ┆" } else { "" }, Style::default().fg(DIM))])];
            lines.extend(wrap(why, width.saturating_sub(2)).into_iter().map(|l| Line::from(Span::styled(format!("  {l}"), Style::default().fg(DIM)))));
            ListItem::new(lines)
        })
        .collect()
}

fn draw_middle(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)]).split(area);

    for (k, incoming) in [(0usize, true), (2usize, false)] {
        let pane = if incoming { Pane::From } else { Pane::To };
        let color = if incoming { AMBER } else { BLUE };
        let title = Line::from(Span::styled(if incoming { " came from " } else { " leads to " }, Style::default().fg(color)));
        let block = pane_block(title, app.focus == pane);
        let w = block.inner(cols[k]).width as usize;
        let links = app.links(incoming);
        let list = List::new(link_items(app, &links, color, w))
            .block(block)
            .highlight_style(if app.focus == pane { Style::default().bg(SEL_BG) } else { Style::default() });
        let st = if incoming { &mut app.from } else { &mut app.to };
        f.render_stateful_widget(list, cols[k], st);
    }

    let node = app.node();
    let title = Line::from(Span::styled(format!(" {} ", node.name), Style::default().bold()));
    let block = pane_block(title, app.focus == Pane::Inside);
    let inner = block.inner(cols[1]);
    f.render_widget(block, cols[1]);
    let w = inner.width as usize;
    let path = match &node.reference {
        Some(r) => r.clone(),
        None if node.paths.len() > 1 => format!("{} ({} files)", node.dir, node.paths.len()),
        None => node.paths[0].strip_prefix(&app.cfg.prefix).unwrap_or(&node.paths[0]).to_string(),
    };
    let mut head: Vec<Line> = vec![Line::from(Span::styled(trunc(&path, w), Style::default().fg(DIM)))];
    let what = wrap(&node.what, w);
    let what_budget = if node.outline.is_empty() { inner.height as usize } else { 4 };
    let clipped = what.len() > what_budget;
    head.extend(what.into_iter().take(what_budget).map(Line::from));
    if clipped {
        if let Some(last) = head.last_mut() {
            last.spans.push(Span::styled(" …", Style::default().fg(DIM)));
        }
    }
    let head_h = head.len() as u16;
    let parts = Layout::vertical([Constraint::Length(head_h), Constraint::Length(1), Constraint::Min(0)]).split(inner);
    f.render_widget(Paragraph::new(head), parts[0]);
    if !node.outline.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("INSIDE THIS FILE", Style::default().fg(DIM))), parts[1]);
        let items: Vec<ListItem> = node
            .outline
            .iter()
            .map(|(name, line, why)| {
                let head = format!("{line:>4} {name}");
                let rest = w.saturating_sub(head.width() + 3);
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{line:>4} "), Style::default().fg(DIM)),
                    Span::styled(name.clone(), Style::default().bold()),
                    Span::styled(format!("  {}", trunc(why, rest)), Style::default().fg(DIM)),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(if app.focus == Pane::Inside { Style::default().bg(SEL_BG) } else { Style::default() });
        f.render_stateful_widget(list, parts[2], &mut app.inside);
    }
}

fn draw_diff(f: &mut Frame, app: &App, area: Rect) {
    let node = app.node();
    let title = match &node.reference {
        Some(_) => " diff · unchanged ".to_string(),
        None if node.paths.len() == 1 => format!(" diff · {} ", node.paths[0].strip_prefix(&app.cfg.prefix).unwrap_or(&node.paths[0])),
        None => format!(" diff · {} files ", node.paths.len()),
    };
    let block = pane_block(Line::from(title), app.focus == Pane::Diff);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<Line> = app
        .lines
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(inner.height as usize)
        .map(|(i, l)| {
            let (fg, gutter) = match l.kind {
                Kind::Add => (GREEN, l.n.map(|n| format!("{n:>4} ")).unwrap_or_default()),
                Kind::Del => (RED, "     ".into()),
                Kind::Hunk => (DIM, "     ".into()),
                Kind::Ctx => (Color::Reset, l.n.map(|n| format!("{n:>4} ")).unwrap_or_default()),
                Kind::File => (BLUE, String::new()),
                Kind::Note => (DIM, String::new()),
            };
            let near_hl = app.hl.is_some_and(|h| i >= h && i < h + 3);
            let style = if near_hl { Style::default().bg(HL_BG) } else { Style::default() };
            let text_style = if l.kind == Kind::File { Style::default().fg(fg).bold() } else { Style::default().fg(fg) };
            Line::from(vec![Span::styled(gutter, Style::default().fg(FAINT)), Span::styled(l.text.replace('\t', "    "), text_style)]).style(style)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn snapshot(app: &mut App, w: u16, h: u16) {
    let mut term = Terminal::new(backend::TestBackend::new(w, h)).unwrap();
    term.draw(|f| draw(f, app)).unwrap();
    let buf = term.backend().buffer();
    for y in 0..h {
        let row: String = (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect();
        println!("{}", row.trim_end());
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: codemapx <map-dir> [--snapshot <node-id> [outline-index]]");
        eprintln!("  <map-dir> holds config.json and diffs.json");
        std::process::exit(2);
    };
    let read = |name: &str| {
        fs::read_to_string(format!("{dir}/{name}")).unwrap_or_else(|e| {
            eprintln!("codemapx: can't read {dir}/{name}: {e}");
            std::process::exit(1);
        })
    };
    let cfg: Config = serde_json::from_str(&read("config.json")).expect("config.json");
    let diffs: HashMap<String, FileDiff> = serde_json::from_str(&read("diffs.json")).expect("diffs.json");
    let args = &args[1..];
    let mut app = App::new(cfg, diffs);

    // --snapshot <node-id> <outline-index>: print one frame as text, for checking layout.
    if args.first().map(String::as_str) == Some("--snapshot") {
        if let Some(i) = args.get(1).and_then(|id| app.idx.get(id).copied()) {
            app.select(i);
        }
        if let Some(k) = args.get(2).and_then(|k| k.parse::<isize>().ok()) {
            app.focus = Pane::Inside;
            app.move_in(k);
        }
        snapshot(&mut app, 180, 52);
        return Ok(());
    }

    let mut term = ratatui::init();
    let res = run(&mut term, &mut app);
    ratatui::restore();
    res
}

fn run(term: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    loop {
        term.draw(|f| draw(f, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        app.flash = None;
        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
            KeyCode::Esc if app.diff_full => app.diff_full = false,
            KeyCode::Esc => return Ok(()),
            KeyCode::Left => app.step(-1),
            KeyCode::Right => app.step(1),
            KeyCode::Tab => {
                let p = PANES.iter().position(|p| *p == app.focus).unwrap();
                app.focus = PANES[(p + 1) % PANES.len()];
            }
            KeyCode::BackTab => {
                let p = PANES.iter().position(|p| *p == app.focus).unwrap();
                app.focus = PANES[(p + PANES.len() - 1) % PANES.len()];
            }
            KeyCode::Up | KeyCode::Char('k') => app.move_in(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_in(1),
            KeyCode::Char('K') | KeyCode::PageUp => app.scroll_by(-15),
            KeyCode::Char('J') | KeyCode::PageDown => app.scroll_by(15),
            KeyCode::Enter => app.enter(),
            KeyCode::Char('d') => app.diff_full = !app.diff_full,
            KeyCode::Char('o') => match app.editor_target() {
                Some((path, line)) => {
                    ratatui::restore();
                    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".into());
                    let _ = Command::new(editor).arg(format!("+{line}")).arg(&path).status();
                    *term = ratatui::init();
                    term.clear()?;
                }
                None => app.flash = Some(" Nothing to open: this file doesn't exist yet.".into()),
            },
            _ => {}
        }
    }
}
