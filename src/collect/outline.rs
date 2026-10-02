//! Top-level declarations of TS/JS files, via tree-sitter.

use std::collections::BTreeSet;

use tree_sitter::{Language, Node, Parser, Tree};

use crate::facts::{FunctionItem, OutlineItem};

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

/// A declaration in source order; lines are 1-based and inclusive. `nested` ones are functions declared
/// inside a body or callback rather than at top level, and `depth` counts the listed functions around them.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
    pub exported: bool,
    pub depth: usize,
    pub nested: bool,
}

pub fn declarations(tree: &Tree, src: &str) -> Vec<Decl> {
    let root = tree.root_node();
    let mut cur = root.walk();
    let mut out = vec![];
    for node in root.children(&mut cur) {
        if node.kind() == "export_statement" {
            match node.child_by_field_name("declaration") {
                Some(d) => push_decl(d, true, src, &mut out),
                None => push_nested(node, 0, src, &mut out),
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
        _ => return push_nested(node, 0, src, out),
    };
    if !sound(node) {
        return;
    }
    let (start, end) = lines(node);
    if kind == "variable" {
        let mut c = node.walk();
        for d in node.named_children(&mut c).filter(|d| d.kind() == "variable_declarator" && sound(*d)) {
            let (s, e) = lines(d);
            let f = holds_fn(d);
            out.push(Decl { name: name(d), kind: if f { "function" } else { kind }, start: s, end: e, exported, depth: 0, nested: false });
            push_nested(d, usize::from(f), src, out);
        }
        return;
    }
    let decl_name = name(node);
    out.push(Decl { name: decl_name.clone(), kind, start, end, exported, depth: 0, nested: false });
    if kind == "function" {
        push_nested(node, 1, src, out);
    }
    if kind == "class"
        && let Some(body) = node.child_by_field_name("body")
    {
        let mut c = body.walk();
        // TS calls class fields public_field_definition (name), JS field_definition (property).
        let is_method = |m: &Node| m.kind() == "method_definition" || (matches!(m.kind(), "public_field_definition" | "field_definition") && holds_fn(*m));
        for m in body.named_children(&mut c).filter(|m| is_method(m) && sound(*m)) {
            let (s, e) = lines(m);
            let n = m.child_by_field_name("name").or_else(|| m.child_by_field_name("property")).map(&text).unwrap_or_default();
            out.push(Decl { name: format!("{decl_name}.{n}"), kind: "method", start: s, end: e, exported: false, depth: 1, nested: false });
            push_nested(m, 2, src, out);
        }
    }
}

/// Functions declared anywhere under `node` (bodies, callbacks like `describe`), in source order; `depth` is
/// the depth of the first one found. Iterative, so deep expression trees can't overflow the stack.
fn push_nested(node: Node, depth: usize, src: &str, out: &mut Vec<Decl>) {
    let text = |n: Node| n.utf8_text(src.as_bytes()).unwrap_or("").to_string();
    let mut stack = vec![(node, depth)];
    while let Some((n, d)) = stack.pop() {
        let mut inner = d;
        if n != node && sound(n) && let Some(name) = n.child_by_field_name("name").filter(|_| is_nested_fn(n)) {
            let (start, end) = (n.start_position().row + 1, n.end_position().row + 1);
            out.push(Decl { name: text(name), kind: "function", start, end, exported: false, depth: d, nested: true });
            inner = d + 1;
        }
        let mut c = n.walk();
        let kids: Vec<Node> = n.named_children(&mut c).collect();
        stack.extend(kids.into_iter().rev().map(|k| (k, inner)));
    }
}

fn is_nested_fn(n: Node) -> bool {
    match n.kind() {
        "function_declaration" | "generator_function_declaration" => true,
        "variable_declarator" => holds_fn(n) && n.child_by_field_name("name").is_some_and(|m| m.kind() == "identifier"),
        _ => false,
    }
}

/// False when parse recovery broke the declaration itself (an ERROR or MISSING direct child, or a broken name),
/// so its name or range can't be trusted; errors nested deeper, e.g. in the body, leave it sound.
fn sound(n: Node) -> bool {
    let mut c = n.walk();
    n.child_by_field_name("name").is_none_or(|m| !m.is_missing() && !m.has_error()) && !n.children(&mut c).any(|k| k.is_error() || k.is_missing())
}

/// True when the value is a function, also behind parens, `as`, `satisfies` or `!`.
fn holds_fn(n: Node) -> bool {
    let mut v = n.child_by_field_name("value");
    while let Some(w) = v.filter(|w| matches!(w.kind(), "parenthesized_expression" | "as_expression" | "satisfies_expression" | "non_null_expression")) {
        v = w.named_child(0);
    }
    v.is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function" | "generator_function"))
}

fn touches(d: &Decl, added: &[usize]) -> bool {
    added.iter().any(|&l| l >= d.start && l <= d.end)
}

/// Functions, classes and methods, plus exported variables and types, that the branch touched.
pub fn outline(decls: &[Decl], added: &[usize]) -> Vec<OutlineItem> {
    decls
        .iter()
        .filter(|d| !d.nested && (matches!(d.kind, "function" | "class" | "method") || d.exported))
        .filter(|d| touches(d, added))
        .map(|d| OutlineItem { name: d.name.clone(), kind: d.kind.into(), start: d.start, end: d.end })
        .collect()
}

/// Functions at any depth, classes and their methods, for the functions panel; `deleted` is from `diff::deleted_at`.
/// A deletion just above a declaration's first line falls outside it.
pub fn functions(decls: &[Decl], added: &[usize], deleted: &[usize]) -> Vec<FunctionItem> {
    decls
        .iter()
        .filter(|d| matches!(d.kind, "function" | "class" | "method"))
        .map(|d| {
            let changed = touches(d, added) || deleted.iter().any(|&n| n > d.start && n <= d.end);
            FunctionItem { name: d.name.clone(), kind: d.kind.into(), start: d.start, end: d.end, changed, depth: d.depth }
        })
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
    fn functions_lists_every_function_class_and_method_marking_changed_ones() {
        let got: Vec<String> = functions(&decls(), &[7], &[]).into_iter().map(|f| format!("{} {} {}", f.kind, f.name, f.changed)).collect();
        assert_eq!(got, vec!["function apply true", "class Writer false", "method Writer.write false"]);
    }

    #[test]
    fn consts_holding_functions_are_functions() {
        let src = "export const a = () => 1;\nconst b = function () {\n  return 2;\n};\nconst c = 3;\n";
        let d = declarations(&parse(Lang::Ts, src).unwrap(), src);
        let got: Vec<(&str, &str, usize, usize)> = d.iter().map(|d| (d.name.as_str(), d.kind, d.start, d.end)).collect();
        assert_eq!(got, vec![("a", "function", 1, 1), ("b", "function", 2, 4), ("c", "variable", 5, 5)]);
    }

    #[test]
    fn each_declarator_gets_its_own_lines() {
        let src = "const a = () => 1,\n  b = () => 2;\n";
        let d = declarations(&parse(Lang::Ts, src).unwrap(), src);
        let got: Vec<String> = functions(&d, &[2], &[]).into_iter().map(|f| format!("{} {}-{} {}", f.name, f.start, f.end, f.changed)).collect();
        assert_eq!(got, vec!["a 1-1 false", "b 2-2 true"]);
    }

    #[test]
    fn nested_functions_are_listed_at_their_depth_but_left_out_of_the_outline() {
        let src = "function outer() {\n  const inner = () => {\n    function deepest() {}\n  };\n  const n = 1;\n  return [1].map((x) => x);\n}\n\ndescribe('x', () => {\n  const setup = async () => 1;\n});\n\nclass C {\n  go() {\n    const h = () => 1;\n  }\n}\n";
        let d = declarations(&parse(Lang::Ts, src).unwrap(), src);
        let got: Vec<String> = functions(&d, &[3], &[]).into_iter().map(|f| format!("{} {} {} {}-{} {}", f.depth, f.kind, f.name, f.start, f.end, f.changed)).collect();
        assert_eq!(
            got,
            vec![
                "0 function outer 1-7 true",
                "1 function inner 2-4 true",
                "2 function deepest 3-3 true",
                "0 function setup 10-10 false",
                "0 class C 13-17 false",
                "1 method C.go 14-16 false",
                "2 function h 15-15 false",
            ]
        );
        let names: Vec<String> = outline(&d, &[3, 10, 15]).into_iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["outer", "C", "C.go"]);
    }

    #[test]
    fn wrapped_function_values_are_functions() {
        let src = "const a = (() => 1);\nconst b = (() => 1) as F;\nconst c = (() => 1) satisfies F;\nconst d = memo(() => 1);\n";
        let got: Vec<String> = declarations(&parse(Lang::Ts, src).unwrap(), src).into_iter().map(|d| format!("{} {}", d.kind, d.name)).collect();
        assert_eq!(got, vec!["function a", "function b", "function c", "variable d"]);
    }

    #[test]
    fn class_fields_holding_functions_are_methods() {
        let src = "class C {\n  run = () => 1;\n  limit = 3;\n  go() {}\n}\n";
        let d = declarations(&parse(Lang::Ts, src).unwrap(), src);
        let got: Vec<String> = functions(&d, &[], &[]).into_iter().map(|f| format!("{} {} {}", f.kind, f.name, f.start)).collect();
        assert_eq!(got, vec!["class C 1", "method C.run 2", "method C.go 4"]);
    }

    #[test]
    fn deletions_inside_a_function_mark_it_changed() {
        // apply is lines 6-8: a deletion before line 8 is inside it; one before line 6 is not.
        let changed = |deleted: &[usize]| functions(&decls(), &[], deleted).into_iter().filter(|f| f.changed).map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(changed(&[8]), vec!["apply"]);
        assert!(changed(&[6]).is_empty());
    }

    #[test]
    fn declarations_survive_errors_inside_function_bodies() {
        // tree-sitter-typescript 0.23 can't parse a generic tagged template (Prisma's `$queryRaw<T>`...``).
        let src = "async function f() {\n  const r = await db.q<{ id: string }[]>`x`;\n  return r;\n}\n\nexport function g() {\n  return 1;\n}\n";
        let tree = parse(Lang::Ts, src).unwrap();
        assert!(tree.root_node().has_error());
        let got: Vec<String> = declarations(&tree, src).into_iter().map(|d| format!("{} {} {}-{} {}", d.kind, d.name, d.start, d.end, d.exported)).collect();
        assert_eq!(got, vec!["function f 1-4 false", "function g 6-8 true"]);
    }

    #[test]
    fn declarations_broken_by_recovery_are_dropped() {
        // Recovery names the class `extends` and the function `x`; the stray `const = 3` becomes a top-level ERROR.
        let src = "export class extends B {}\nfunction 1x() {}\nconst = 3;\nfunction f(a {\n  return 1;\n}\n";
        let got: Vec<String> = declarations(&parse(Lang::Ts, src).unwrap(), src).into_iter().map(|d| format!("{} {}-{}", d.name, d.start, d.end)).collect();
        assert_eq!(got, vec!["f 4-6"]);
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
