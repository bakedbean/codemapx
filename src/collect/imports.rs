//! Import statements and how their specifiers resolve to repo paths.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use tree_sitter::{Node, Tree};

use crate::paths::{join, normalize, parent};

pub const EXTS: [&str; 4] = [".ts", ".tsx", ".js", ".jsx"];

#[derive(Debug, Clone, PartialEq)]
pub struct ImportName {
    pub imported: String,
    pub local: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub specifier: String,
    pub names: Vec<ImportName>,
    pub line: usize,
    pub end_line: usize,
    pub text: String,
}

pub fn imports(tree: &Tree, src: &str) -> Vec<Import> {
    let root = tree.root_node();
    let mut c = root.walk();
    let mut out = vec![];
    for node in root.children(&mut c).filter(|n| n.kind() == "import_statement") {
        let Some(source) = node.child_by_field_name("source") else { continue };
        let specifier = source.utf8_text(src.as_bytes()).unwrap_or("").trim_matches(|c| c == '\'' || c == '"').to_string();
        let mut names = vec![];
        collect_names(node, src, &mut names);
        let line = node.start_position().row + 1;
        out.push(Import {
            specifier,
            names,
            line,
            end_line: node.end_position().row + 1,
            text: src.lines().nth(line - 1).unwrap_or("").trim().to_string(),
        });
    }
    out
}

fn collect_names(node: Node, src: &str, out: &mut Vec<ImportName>) {
    let t = |n: Node| n.utf8_text(src.as_bytes()).unwrap_or("").to_string();
    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        match child.kind() {
            "import_clause" | "named_imports" => collect_names(child, src, out),
            "identifier" => out.push(ImportName { imported: "default".into(), local: t(child) }),
            "namespace_import" => {
                let mut c2 = child.walk();
                if let Some(id) = child.named_children(&mut c2).find(|n| n.kind() == "identifier") {
                    out.push(ImportName { imported: "*".into(), local: t(id) });
                }
            }
            "import_specifier" => {
                if let Some(name) = child.child_by_field_name("name") {
                    let local = child.child_by_field_name("alias").unwrap_or(name);
                    out.push(ImportName { imported: t(name), local: t(local) });
                }
            }
            _ => {}
        }
    }
}

/// `compilerOptions.baseUrl`/`paths` of one tsconfig.json; `extends` is not followed.
#[derive(Debug, Default, PartialEq)]
struct TsPaths {
    base: String,
    base_url_set: bool,
    patterns: Vec<(String, Vec<String>)>,
}

impl TsPaths {
    fn parse(dir: &str, text: &str) -> Option<TsPaths> {
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).ok()?;
        let opts = &v["compilerOptions"];
        let base_url = opts["baseUrl"].as_str();
        let patterns = opts["paths"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, t)| {
                        let targets = t.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
                        (k.clone(), targets)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(TsPaths { base: normalize(&join(dir, base_url.unwrap_or("."))), base_url_set: base_url.is_some(), patterns })
    }

    fn candidates(&self, spec: &str) -> Vec<String> {
        let mut out = vec![];
        for (pat, targets) in &self.patterns {
            let star = match pat.split_once('*') {
                Some((pre, suf)) => spec.strip_prefix(pre).and_then(|r| r.strip_suffix(suf)),
                None => (pat == spec).then_some(""),
            };
            if let Some(star) = star {
                out.extend(targets.iter().map(|t| normalize(&join(&self.base, &t.replacen('*', star, 1)))));
            }
        }
        if self.base_url_set {
            out.push(normalize(&join(&self.base, spec)));
        }
        out
    }
}

pub struct Resolver<'a> {
    read: &'a dyn Fn(&str) -> Option<String>,
    cache: RefCell<HashMap<String, Option<Rc<TsPaths>>>>,
}

impl<'a> Resolver<'a> {
    pub fn new(read: &'a dyn Fn(&str) -> Option<String>) -> Self {
        Resolver { read, cache: RefCell::new(HashMap::new()) }
    }

    /// The repo path `spec`, imported from file `from`, refers to, if `exists` accepts one.
    pub fn resolve(&self, from: &str, spec: &str, exists: &dyn Fn(&str) -> bool) -> Option<String> {
        let bases = if spec.starts_with("./") || spec.starts_with("../") {
            vec![normalize(&join(parent(from), spec))]
        } else {
            self.tsconfig_for(parent(from)).map(|ts| ts.candidates(spec)).unwrap_or_default()
        };
        bases.iter().find_map(|b| with_extensions(b).into_iter().find(|p| exists(p)))
    }

    // Nearest tsconfig.json at or above `dir`.
    fn tsconfig_for(&self, dir: &str) -> Option<Rc<TsPaths>> {
        let mut d = dir.to_string();
        loop {
            if let Some(hit) = self.cache.borrow().get(&d) {
                return hit.clone();
            }
            if let Some(text) = (self.read)(&join(&d, "tsconfig.json")) {
                let parsed = TsPaths::parse(&d, &text).map(Rc::new);
                self.cache.borrow_mut().insert(d, parsed.clone());
                return parsed;
            }
            if d.is_empty() {
                return None;
            }
            d = parent(&d).to_string();
        }
    }
}

fn with_extensions(base: &str) -> Vec<String> {
    let mut v = vec![base.to_string()];
    // TS ESM code imports `./x.js` meaning `./x.ts`.
    if let Some(stem) = base.strip_suffix(".js").or_else(|| base.strip_suffix(".jsx")) {
        v.extend([".ts", ".tsx"].iter().map(|e| format!("{stem}{e}")));
    }
    v.extend(EXTS.iter().map(|e| format!("{base}{e}")));
    v.extend(EXTS.iter().map(|e| format!("{base}/index{e}")));
    v
}

/// tsconfig.json allows comments and trailing commas; serde_json doesn't.
pub fn strip_jsonc(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(c.len());
    let (mut i, mut in_str) = (0, false);
    while i < c.len() {
        if in_str {
            out.push(c[i]);
            if c[i] == '\\' && i + 1 < c.len() {
                out.push(c[i + 1]);
                i += 1;
            } else if c[i] == '"' {
                in_str = false;
            }
            i += 1;
        } else if c[i] == '"' {
            in_str = true;
            out.push('"');
            i += 1;
        } else if c[i] == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    let mut res = String::with_capacity(out.len());
    let (mut k, mut in_str) = (0, false);
    while k < out.len() {
        let ch = out[k];
        if in_str {
            res.push(ch);
            if ch == '\\' && k + 1 < out.len() {
                res.push(out[k + 1]);
                k += 1;
            } else if ch == '"' {
                in_str = false;
            }
        } else if ch == ',' && matches!(out[k + 1..].iter().find(|c| !c.is_whitespace()), Some('}' | ']')) {
        } else {
            in_str = ch == '"';
            res.push(ch);
        }
        k += 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::outline::{Lang, parse};

    fn names(i: &Import) -> Vec<(&str, &str)> {
        i.names.iter().map(|n| (n.imported.as_str(), n.local.as_str())).collect()
    }

    #[test]
    fn reads_import_statements() {
        let src = "import Def, { a, b as c } from './m';\nimport * as ns from \"@/lib/x\";\nimport type { T } from '../t';\nimport './side';\nimport {\n  d,\n} from './multi';\n";
        let got = imports(&parse(Lang::Ts, src).unwrap(), src);
        assert_eq!(got.len(), 5);
        assert_eq!(got[0].specifier, "./m");
        assert_eq!(names(&got[0]), vec![("default", "Def"), ("a", "a"), ("b", "c")]);
        assert_eq!(got[0].text, "import Def, { a, b as c } from './m';");
        assert_eq!((got[1].specifier.as_str(), names(&got[1])), ("@/lib/x", vec![("*", "ns")]));
        assert_eq!(names(&got[2]), vec![("T", "T")]);
        assert!(got[3].names.is_empty());
        assert_eq!((got[4].line, got[4].end_line, got[4].text.as_str()), (5, 7, "import {"));
    }

    fn read(p: &str) -> Option<String> {
        match p {
            "tsconfig.json" => Some("{ // root\n \"compilerOptions\": { \"baseUrl\": \".\", \"paths\": { \"@/*\": [\"src/*\"], }, }, }".into()),
            "apps/web/tsconfig.json" => Some("{ \"compilerOptions\": { \"paths\": { \"~/*\": [\"./*\"] } } }".into()),
            _ => None,
        }
    }

    fn resolve(from: &str, spec: &str, files: &[&str]) -> Option<String> {
        Resolver::new(&read).resolve(from, spec, &|p| files.contains(&p))
    }

    #[test]
    fn resolves_relative_alias_index_and_js_specifiers() {
        let files = ["src/billing/mills.ts", "src/billing/apply.ts", "src/billing/types.ts", "src/api/lib/index.ts", "apps/web/src/b.tsx"];
        assert_eq!(resolve("src/billing/apply.ts", "./mills", &files).as_deref(), Some("src/billing/mills.ts"));
        assert_eq!(resolve("src/api/route.ts", "../billing/apply", &files).as_deref(), Some("src/billing/apply.ts"));
        assert_eq!(resolve("src/api/route.ts", "./lib", &files).as_deref(), Some("src/api/lib/index.ts"));
        assert_eq!(resolve("src/billing/apply.ts", "./mills.js", &files).as_deref(), Some("src/billing/mills.ts"));
        assert_eq!(resolve("src/api/route.ts", "@/billing/types", &files).as_deref(), Some("src/billing/types.ts"));
        assert_eq!(resolve("apps/web/src/a.ts", "~/src/b", &files).as_deref(), Some("apps/web/src/b.tsx"));
        assert_eq!(resolve("src/api/route.ts", "react", &files), None);
    }

    #[test]
    fn strips_comments_and_trailing_commas_but_not_strings() {
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc("{\"a\": \"http://x/*y*/\", /* c */ \"b\": [1,2,],}")).unwrap();
        assert_eq!(v["a"], "http://x/*y*/");
        assert_eq!(v["b"], serde_json::json!([1, 2]));
    }
}
