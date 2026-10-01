//! Candidate links between changed files, each with one line of evidence.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    collect::imports::{EXTS, Import},
    facts::{Candidate, CandidateKind, Evidence},
    paths::{join, parent},
};

pub struct FileInfo {
    pub path: String,
    pub exists: bool,
    pub added: Vec<(usize, String)>,
    pub imports: Vec<Import>,
    pub changed_exports: BTreeSet<String>,
    pub first_line: String,
}

/// Ids are `c1..cN` in sorted (from, to) order, so reruns on one HEAD give the same ids.
pub fn candidates(files: &[FileInfo], resolve: &dyn Fn(&str, &str) -> Option<String>) -> Vec<Candidate> {
    let by_path: BTreeMap<&str, &FileInfo> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut best: BTreeMap<(String, String), (CandidateKind, Evidence)> = BTreeMap::new();
    let mut offer = |from: &str, to: &str, kind: CandidateKind, ev: Evidence| {
        if from == to {
            return;
        }
        let key = (from.to_string(), to.to_string());
        if best.get(&key).is_none_or(|(k, _)| *k < kind) {
            best.insert(key, (kind, ev));
        }
    };
    for f in files.iter().filter(|f| f.exists) {
        for imp in &f.imports {
            let Some(target) = resolve(&f.path, &imp.specifier).and_then(|t| by_path.get(t.as_str()).copied()) else { continue };
            match export_use(f, imp, target) {
                Some(ev) => offer(&target.path, &f.path, CandidateKind::ExportUse, ev),
                None => offer(&target.path, &f.path, CandidateKind::Import, Evidence { path: f.path.clone(), line: imp.line, quote: imp.text.clone() }),
            }
        }
        if let Some(src) = test_source(&f.path, &by_path) {
            let ev = f
                .imports
                .iter()
                .find(|i| resolve(&f.path, &i.specifier).as_deref() == Some(src))
                .map(|i| Evidence { path: f.path.clone(), line: i.line, quote: i.text.clone() })
                .unwrap_or_else(|| Evidence { path: f.path.clone(), line: 1, quote: f.first_line.clone() });
            offer(src, &f.path, CandidateKind::TestPair, ev);
        }
    }
    best.into_iter()
        .enumerate()
        .map(|(i, ((from, to), (kind, evidence)))| Candidate { id: format!("c{}", i + 1), from, to, kind, evidence })
        .collect()
}

// First added, non-import line of `importer` that uses a name `target` added or changed as an export.
fn export_use(importer: &FileInfo, imp: &Import, target: &FileInfo) -> Option<Evidence> {
    let locals: Vec<&str> = imp.names.iter().filter(|n| target.changed_exports.contains(&n.imported)).map(|n| n.local.as_str()).collect();
    if locals.is_empty() {
        return None;
    }
    let in_import = |line: usize| importer.imports.iter().any(|i| line >= i.line && line <= i.end_line);
    importer
        .added
        .iter()
        .filter(|(l, _)| !in_import(*l))
        .find(|(_, text)| locals.iter().any(|n| has_word(text, n)))
        .map(|(l, text)| Evidence { path: importer.path.clone(), line: *l, quote: text.trim().to_string() })
}

// `foo.test.ts`, `foo.spec.ts` or `__tests__/foo.ts` -> an existing changed `foo.*` source.
fn test_source<'a>(test: &str, files: &BTreeMap<&'a str, &FileInfo>) -> Option<&'a str> {
    let (dir, file) = test.rsplit_once('/').unwrap_or(("", test));
    let stem = file.rsplit_once('.')?.0;
    let stripped = stem.strip_suffix(".test").or_else(|| stem.strip_suffix(".spec"));
    let in_tests_dir = dir == "__tests__" || dir.ends_with("/__tests__");
    if stripped.is_none() && !in_tests_dir {
        return None;
    }
    let base = stripped.unwrap_or(stem);
    let dir = if in_tests_dir { parent(dir) } else { dir };
    EXTS.iter()
        .map(|e| join(dir, &format!("{base}{e}")))
        .find_map(|p| files.get_key_value(p.as_str()).filter(|(k, f)| f.exists && **k != test).map(|(k, _)| *k))
}

pub fn has_word(text: &str, word: &str) -> bool {
    let is_id = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(is_id) && !after.is_some_and(is_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::imports::ImportName;

    fn imp(spec: &str, names: &[(&str, &str)], line: usize) -> Import {
        Import {
            specifier: spec.into(),
            names: names.iter().map(|(i, l)| ImportName { imported: i.to_string(), local: l.to_string() }).collect(),
            line,
            end_line: line,
            text: format!("import ... from '{spec}';"),
        }
    }

    fn file(path: &str, added: &[(usize, &str)], imports: Vec<Import>, exports: &[&str]) -> FileInfo {
        FileInfo {
            path: path.into(),
            exists: true,
            added: added.iter().map(|(n, t)| (*n, t.to_string())).collect(),
            imports,
            changed_exports: exports.iter().map(|s| s.to_string()).collect(),
            first_line: "// first".into(),
        }
    }

    fn resolve(_from: &str, spec: &str) -> Option<String> {
        match spec {
            "@/types" => Some("src/types.ts".into()),
            "./mills" => Some("src/mills.ts".into()),
            "./a" => Some("src/a.ts".into()),
            _ => None,
        }
    }

    fn summary(c: &[Candidate]) -> Vec<(&str, &str, &str, CandidateKind, usize)> {
        c.iter().map(|c| (c.id.as_str(), c.from.as_str(), c.to.as_str(), c.kind, c.evidence.line)).collect()
    }

    #[test]
    fn export_use_beats_import_and_skips_import_lines() {
        let files = vec![
            file("src/types.ts", &[(6, "export interface Insertion {")], vec![], &["Insertion"]),
            file(
                "src/apply.ts",
                &[(1, "import type { Insertion } from '@/types';"), (4, "export function f(i: Insertion[]) {")],
                vec![imp("@/types", &[("Insertion", "Insertion")], 1)],
                &[],
            ),
        ];
        assert_eq!(summary(&candidates(&files, &resolve)), vec![("c1", "src/types.ts", "src/apply.ts", CandidateKind::ExportUse, 4)]);
    }

    #[test]
    fn import_when_no_changed_export_is_used() {
        let files = vec![
            file("src/types.ts", &[(6, "export interface Insertion {")], vec![], &["Insertion"]),
            file("src/route.ts", &[(4, "function h(f: Fee) {")], vec![imp("@/types", &[("Fee", "Fee")], 1)], &[]),
        ];
        let c = candidates(&files, &resolve);
        assert_eq!(summary(&c), vec![("c1", "src/types.ts", "src/route.ts", CandidateKind::Import, 1)]);
        assert_eq!(c[0].evidence.quote, "import ... from '@/types';");
    }

    #[test]
    fn aliased_names_match_by_local_name() {
        let files = vec![
            file("src/mills.ts", &[], vec![], &["millsToDecimal"]),
            file("src/apply.ts", &[(5, "  return toDec(x);")], vec![imp("./mills", &[("millsToDecimal", "toDec")], 1)], &[]),
        ];
        assert_eq!(candidates(&files, &resolve)[0].kind, CandidateKind::ExportUse);
    }

    #[test]
    fn test_pairs_without_imports_use_line_one() {
        let files = vec![
            file("src/a.ts", &[], vec![], &[]),
            file("src/a.test.ts", &[], vec![], &[]),
            file("src/__tests__/a.ts", &[], vec![], &[]),
        ];
        let c = candidates(&files, &resolve);
        assert_eq!(
            summary(&c),
            vec![
                ("c1", "src/a.ts", "src/__tests__/a.ts", CandidateKind::TestPair, 1),
                ("c2", "src/a.ts", "src/a.test.ts", CandidateKind::TestPair, 1),
            ]
        );
        assert_eq!(c[0].evidence.quote, "// first");
    }

    #[test]
    fn dedupes_to_strongest_and_ignores_deleted_and_self() {
        let mut gone = file("src/gone.ts", &[], vec![imp("./a", &[], 1)], &[]);
        gone.exists = false;
        let files = vec![
            file("src/a.ts", &[], vec![imp("./a", &[], 1)], &["run"]),
            file("src/a.test.ts", &[(3, "run();")], vec![imp("./a", &[("run", "run")], 1)], &[]),
            gone,
        ];
        assert_eq!(summary(&candidates(&files, &resolve)), vec![("c1", "src/a.ts", "src/a.test.ts", CandidateKind::ExportUse, 3)]);
    }

    #[test]
    fn has_word_respects_identifier_boundaries() {
        assert!(has_word("x = millsToDecimal(1)", "millsToDecimal"));
        assert!(has_word("test('millsToDecimal', ...)", "millsToDecimal"));
        assert!(!has_word("millsToDecimalX", "millsToDecimal"));
        assert!(!has_word("$millsToDecimal", "millsToDecimal"));
    }
}
