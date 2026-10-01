//! Repo-relative paths, always `/`-separated and without a leading `./`.

pub fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() { rel.to_string() } else { format!("{dir}/{rel}") }
}

pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Resolves `.` and `..` without touching the filesystem (files may not exist on disk).
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = vec![];
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

/// (`dir/` with trailing slash or "", file name).
pub fn split(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((d, f)) => (format!("{d}/"), f.to_string()),
        None => (String::new(), path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_joins() {
        assert_eq!(normalize("src/api/../billing/./apply"), "src/billing/apply");
        assert_eq!(normalize("./x"), "x");
        assert_eq!(join("", "a.ts"), "a.ts");
        assert_eq!(join("src", "a.ts"), "src/a.ts");
        assert_eq!(parent("src/a.ts"), "src");
        assert_eq!(parent("a.ts"), "");
        assert_eq!(split("src/billing/apply.ts"), ("src/billing/".into(), "apply.ts".into()));
    }
}
