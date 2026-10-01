//! `merge` checks annotations against facts and builds the `Map` both renderers draw.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    annotations::{Annotations, ContextCard, Missing},
    facts::{Candidate, Evidence, Facts, FileFacts, Status},
    paths::split,
};

#[derive(Debug, Clone, Serialize)]
pub struct Map {
    pub title: String,
    pub summary: String,
    pub repo: String,
    pub branch: String,
    pub head: String,
    pub base: String,
    pub commits: usize,
    pub files: usize,
    pub add: u32,
    pub del: u32,
    pub annotations_stale: bool,
    pub columns: Vec<MapColumn>,
    pub cards: Vec<Card>,
    pub links: Vec<Link>,
    pub trail: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapColumn {
    pub name: String,
    pub cards: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Changed,
    Context,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: String,
    pub kind: CardKind,
    pub path: Option<String>,
    pub name: String,
    pub dir: String,
    pub what: String,
    pub status: Option<Status>,
    pub binary: bool,
    pub add: u32,
    pub del: u32,
    pub diff: String,
    pub outline: Vec<MapOutline>,
    pub column: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapOutline {
    pub name: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub reason: String,
    pub evidence: Evidence,
}

impl Map {
    pub fn card_index(&self, id: &str) -> Option<usize> {
        self.cards.iter().position(|c| c.id == id)
    }
}

/// Reports every problem, not just the first. A head mismatch alone is `annotations_stale`, not an error.
pub fn merge(facts: &Facts, ann: &Annotations, read_head: &dyn Fn(&str) -> Option<String>) -> Result<Map, Vec<String>> {
    let mut p: Vec<String> = vec![];
    let changed: BTreeMap<&str, &FileFacts> = facts.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let context: BTreeMap<&str, &ContextCard> = ann.context.iter().map(|c| (c.path.as_str(), c)).collect();
    let known = |path: &str| changed.contains_key(path) || context.contains_key(path);

    // Rule 5: context paths exist at HEAD and are unchanged.
    for c in &ann.context {
        if changed.contains_key(c.path.as_str()) {
            p.push(format!("{}: context path is a changed file", c.path));
        } else if read_head(&c.path).is_none() {
            p.push(format!("{}: context path does not exist at HEAD", c.path));
        }
    }
    // Rules 3 and 4: one non-empty `what` per changed file; outline notes name real entries.
    for (path, note) in &ann.files {
        match changed.get(path.as_str()) {
            None => p.push(format!("{path}: not a changed file")),
            Some(f) => {
                for name in note.outline.keys().filter(|n| !f.outline.iter().any(|o| &o.name == *n)) {
                    p.push(format!("{path}: outline note for unknown entry \"{name}\""));
                }
            }
        }
    }
    for f in &facts.files {
        if ann.files.get(&f.path).is_none_or(|n| n.what.trim().is_empty()) {
            p.push(format!("{}: missing \"what\"", f.path));
        }
    }
    // Rule 1: columns and trail.
    let mut placed: BTreeMap<&str, usize> = BTreeMap::new();
    for (ci, col) in ann.columns.iter().enumerate() {
        for f in &col.files {
            if !known(f) {
                p.push(format!("{f}: in column \"{}\" but not a changed or context file", col.name));
            }
            if placed.insert(f, ci).is_some() {
                p.push(format!("{f}: in more than one column"));
            }
        }
    }
    for f in changed.keys().chain(context.keys()).filter(|f| !placed.contains_key(*f)) {
        p.push(format!("{f}: not in any column"));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for t in &ann.trail {
        if !known(t) {
            p.push(format!("{t}: in trail but not a changed or context file"));
        }
        if !seen.insert(t) {
            p.push(format!("{t}: in trail more than once"));
        }
    }
    for f in changed.keys().filter(|f| !seen.contains(*f)) {
        p.push(format!("{f}: not in trail"));
    }
    // Rule 2: every candidate kept or dropped, exactly once.
    let cands: BTreeMap<&str, &Candidate> = facts.candidates.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut judged: BTreeMap<&str, usize> = BTreeMap::new();
    for id in ann.links.iter().map(|l| &l.candidate).chain(ann.dropped.iter().map(|d| &d.candidate)) {
        if !cands.contains_key(id.as_str()) {
            p.push(format!("{id}: unknown candidate"));
        }
        *judged.entry(id).or_default() += 1;
    }
    for id in cands.keys() {
        match judged.get(id) {
            None => p.push(format!("{id}: candidate neither kept in links nor dropped")),
            Some(n) if *n > 1 => p.push(format!("{id}: candidate listed more than once")),
            _ => {}
        }
    }
    // Rule 6: context links join known cards and quote a real line.
    for l in &ann.context_links {
        for end in [&l.from, &l.to] {
            if !known(end) {
                p.push(format!("{end}: link endpoint is not a changed or context file"));
            }
        }
        p.extend(check_quote(&l.evidence, read_head));
    }
    // Rule 7: missing cards sit near a changed file and have unique ids.
    let mut ids = BTreeSet::new();
    for m in &ann.missing {
        if !changed.contains_key(m.near.as_str()) {
            p.push(format!("{}: near \"{}\" is not a changed file", m.id, m.near));
        }
        if !ids.insert(&m.id) || known(&m.id) {
            p.push(format!("{}: duplicate missing id", m.id));
        }
    }
    if !p.is_empty() {
        return Err(p);
    }
    Ok(build(facts, ann, &changed, &context, &placed, &cands))
}

fn check_quote(ev: &Evidence, read_head: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let Some(src) = read_head(&ev.path) else {
        return Some(format!("{}: evidence file does not exist at HEAD", ev.path));
    };
    let q = ev.quote.trim();
    match src.lines().nth(ev.line.wrapping_sub(1)) {
        _ if q.is_empty() => Some(format!("{}:{}: evidence quote is empty", ev.path, ev.line)),
        Some(l) if l.contains(q) => None,
        Some(_) => Some(format!("{}:{}: evidence quote not found on that line", ev.path, ev.line)),
        None => Some(format!("{}:{}: line out of range", ev.path, ev.line)),
    }
}

fn build(
    facts: &Facts,
    ann: &Annotations,
    changed: &BTreeMap<&str, &FileFacts>,
    context: &BTreeMap<&str, &ContextCard>,
    placed: &BTreeMap<&str, usize>,
    cands: &BTreeMap<&str, &Candidate>,
) -> Map {
    let mut cards: Vec<Card> = vec![];
    for (ci, col) in ann.columns.iter().enumerate() {
        for path in &col.files {
            cards.push(match changed.get(path.as_str()) {
                Some(f) => changed_card(f, ann, ci),
                None => context_card(context[path.as_str()], ci),
            });
        }
    }
    for m in &ann.missing {
        cards.push(missing_card(m, placed[m.near.as_str()]));
    }
    let index: BTreeMap<&str, usize> = cards.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
    let mut links: Vec<Link> = ann
        .links
        .iter()
        .map(|l| {
            let c = cands[l.candidate.as_str()];
            Link { from: index[c.from.as_str()], to: index[c.to.as_str()], reason: l.reason.clone(), evidence: c.evidence.clone() }
        })
        .collect();
    links.extend(ann.context_links.iter().map(|l| Link {
        from: index[l.from.as_str()],
        to: index[l.to.as_str()],
        reason: l.reason.clone(),
        evidence: l.evidence.clone(),
    }));
    let columns = ann
        .columns
        .iter()
        .enumerate()
        .map(|(ci, c)| MapColumn { name: c.name.clone(), cards: (0..cards.len()).filter(|&i| cards[i].column == ci).collect() })
        .collect();
    Map {
        title: ann.title.clone(),
        summary: ann.summary.clone(),
        repo: facts.repo.clone(),
        branch: facts.branch.clone(),
        head: facts.head.clone(),
        base: facts.base.clone(),
        commits: facts.commits.len(),
        files: facts.files.len(),
        add: facts.files.iter().map(|f| f.add).sum(),
        del: facts.files.iter().map(|f| f.del).sum(),
        annotations_stale: ann.head != facts.head,
        trail: ann.trail.iter().map(|t| index[t.as_str()]).collect(),
        columns,
        cards,
        links,
    }
}

fn changed_card(f: &FileFacts, ann: &Annotations, column: usize) -> Card {
    let note = &ann.files[&f.path];
    let (dir, name) = split(&f.path);
    Card {
        id: f.path.clone(),
        kind: CardKind::Changed,
        path: Some(f.path.clone()),
        name,
        dir,
        what: note.what.clone(),
        status: Some(f.status),
        binary: f.binary,
        add: f.add,
        del: f.del,
        diff: f.diff.clone(),
        outline: f
            .outline
            .iter()
            .map(|o| MapOutline {
                name: o.name.clone(),
                kind: o.kind.clone(),
                start: o.start,
                end: o.end,
                note: note.outline.get(&o.name).cloned().unwrap_or_default(),
            })
            .collect(),
        column,
    }
}

fn context_card(c: &ContextCard, column: usize) -> Card {
    let (dir, name) = split(&c.path);
    Card {
        id: c.path.clone(),
        kind: CardKind::Context,
        path: Some(c.path.clone()),
        name,
        dir,
        what: c.what.clone(),
        status: None,
        binary: false,
        add: 0,
        del: 0,
        diff: String::new(),
        outline: vec![],
        column,
    }
}

fn missing_card(m: &Missing, column: usize) -> Card {
    Card {
        id: m.id.clone(),
        kind: CardKind::Missing,
        path: None,
        name: m.name.clone(),
        dir: String::new(),
        what: m.why.clone(),
        status: None,
        binary: false,
        add: 0,
        del: 0,
        diff: String::new(),
        outline: vec![],
        column,
    }
}
