//! Top-level declarations of TS/JS files, via tree-sitter.

use std::collections::BTreeSet;

use tree_sitter::{Language, Node, Parser, Tree};

use crate::facts::OutlineItem;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lang {
    Ts,
    Tsx,
    Js,
}

pub fn lang_for(path: &str) -> Option<Lang> {
    match path.rsplit_once('.')?.1 {
        "ts" | "mts" | "cts" => Some(Lang::Ts),
        "tsx" => Some(Lang::Tsx),
        "js" | "jsx" | "mjs" | "cjs" => Some(Lang::Js),
        _ => None,
    }
}

pub fn parse(lang: Lang, src: &str) -> Option<Tree> {
    let language: Language = match lang {
        Lang::Ts => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Js => tree_sitter_javascript::LANGUAGE.into(),
    };
    let mut p = Parser::new();
    p.set_language(&language).ok()?;
    p.parse(src, None)
}

/// A top-level declaration; lines are 1-based and inclusive.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
    pub exported: bool,
}

pub fn declarations(tree: &Tree, src: &str) -> Vec<Decl> {
    let root = tree.root_node();
    let mut cur = root.walk();
    let mut out = vec![];
    for node in root.children(&mut cur) {
        if node.kind() == "export_statement" {
            if let Some(d) = node.child_by_field_name("declaration") {
                push_decl(d, true, src, &mut out);
            }
        } else {
            push_decl(node, false, src, &mut out);
        }
    }
    out
}

fn push_decl(node: Node, exported: bool, src: &str, out: &mut Vec<Decl>) {
    let text = |n: Node| n.utf8_text(src.as_bytes()).unwrap_or("").to_string();
    let name = |n: Node| n.child_by_field_name("name").map(&text).unwrap_or_else(|| "default".into());
    let lines = |n: Node| (n.start_position().row + 1, n.end_position().row + 1);
    let kind = match node.kind() {
        "function_declaration" | "generator_function_declaration" => "function",
        "class_declaration" | "abstract_class_declaration" => "class",
        "interface_declaration" => "interface",
        "type_alias_declaration" => "type",
        "enum_declaration" => "enum",
        "lexical_declaration" | "variable_declaration" => "variable",
        _ => return,
    };
    let (start, end) = lines(node);
    if kind == "variable" {
        let mut c = node.walk();
        for d in node.named_children(&mut c).filter(|d| d.kind() == "variable_declarator") {
            out.push(Decl { name: name(d), kind, start, end, exported });
        }
        return;
    }
    let decl_name = name(node);
    out.push(Decl { name: decl_name.clone(), kind, start, end, exported });
    if kind == "class" {
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            for m in body.named_children(&mut c).filter(|m| m.kind() == "method_definition") {
                let (s, e) = lines(m);
                out.push(Decl { name: format!("{decl_name}.{}", name(m)), kind: "method", start: s, end: e, exported: false });
            }
        }
    }
}

fn touches(d: &Decl, added: &[usize]) -> bool {
    added.iter().any(|&l| l >= d.start && l <= d.end)
}

/// Functions, classes and methods, plus exported variables and types, that the branch touched.
pub fn outline(decls: &[Decl], added: &[usize]) -> Vec<OutlineItem> {
    decls
        .iter()
        .filter(|d| matches!(d.kind, "function" | "class" | "method") || d.exported)
        .filter(|d| touches(d, added))
        .map(|d| OutlineItem { name: d.name.clone(), kind: d.kind.into(), start: d.start, end: d.end })
        .collect()
}

pub fn changed_exports(decls: &[Decl], added: &[usize]) -> BTreeSet<String> {
    decls.iter().filter(|d| d.exported && touches(d, added)).map(|d| d.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "import { x } from './x';\n\nexport const LIMIT = 3;\nconst local = 1;\n\nexport function apply(a: number): number {\n  return a;\n}\n\nclass Writer {\n  write() {\n    return 1;\n  }\n}\n\nexport interface Row {\n  id: string;\n}\n\ninterface Hidden { x: number }\nexport type Id = string;\nexport enum Mode { A }\n";

    fn decls() -> Vec<Decl> {
        declarations(&parse(Lang::Ts, SRC).unwrap(), SRC)
    }

    #[test]
    fn lists_top_level_declarations_with_lines() {
        let d = decls();
        let got: Vec<(&str, &str, usize, usize, bool)> = d.iter().map(|d| (d.name.as_str(), d.kind, d.start, d.end, d.exported)).collect();
        assert_eq!(
            got,
            vec![
                ("LIMIT", "variable", 3, 3, true),
                ("local", "variable", 4, 4, false),
                ("apply", "function", 6, 8, true),
                ("Writer", "class", 10, 14, false),
                ("Writer.write", "method", 11, 13, false),
                ("Row", "interface", 16, 18, true),
                ("Hidden", "interface", 20, 20, false),
                ("Id", "type", 21, 21, true),
                ("Mode", "enum", 22, 22, true),
            ]
        );
    }

    #[test]
    fn outline_keeps_listable_declarations_touching_added_lines() {
        let names: Vec<String> = outline(&decls(), &[4, 7, 12, 20, 21]).into_iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["apply", "Writer", "Writer.write", "Id"]);
    }

    #[test]
    fn changed_exports_are_exported_declarations_touching_added_lines() {
        let got: Vec<String> = changed_exports(&decls(), &[4, 7, 12, 20, 21]).into_iter().collect();
        assert_eq!(got, vec!["Id", "apply"]);
    }

    #[test]
    fn detects_parse_errors_and_languages() {
        assert!(parse(Lang::Ts, "export function (").unwrap().root_node().has_error());
        assert_eq!(lang_for("a/b.tsx"), Some(Lang::Tsx));
        assert_eq!(lang_for("a/b.mjs"), Some(Lang::Js));
        assert_eq!(lang_for("a/b.d.ts"), Some(Lang::Ts));
        assert_eq!(lang_for("README.md"), None);
    }
}
