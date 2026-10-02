//! Line numbering for unified diffs.

/// `+start` of a hunk header's text after `@@ `.
pub fn hunk_start(rest: &str) -> usize {
    rest.split_whitespace()
        .find_map(|t| t.strip_prefix('+'))
        .and_then(|t| t.split(',').next())
        .and_then(|t| t.parse().ok())
        .unwrap_or(0)
}

/// Added lines as (branch-side line number, text without the `+`).
pub fn added_lines(diff: &str) -> Vec<(usize, &str)> {
    let mut n = 0;
    let mut out = vec![];
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("@@ ") {
            n = hunk_start(rest);
        } else if l.starts_with('-') || l.starts_with('\\') {
        } else {
            if let Some(t) = l.strip_prefix('+') {
                out.push((n, t));
            }
            n += 1;
        }
    }
    out
}

/// Where each deleted line was, as the branch-side line that now follows it.
pub fn deleted_at(diff: &str) -> Vec<usize> {
    let mut n = 0;
    let mut out = vec![];
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("@@ ") {
            n = hunk_start(rest);
        } else if l.starts_with('-') {
            out.push(n);
        } else if !l.starts_with('\\') {
            n += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_added_lines_on_branch_side() {
        let d = "@@ -1,3 +1,4 @@\n a\n-b\n+B\n+C\n c\n@@ -10,2 +11,2 @@\n x\n+y\n\\ No newline at end of file";
        assert_eq!(added_lines(d), vec![(2, "B"), (3, "C"), (12, "y")]);
    }

    #[test]
    fn deletions_sit_at_the_branch_line_that_follows_them() {
        let d = "@@ -1,4 +1,3 @@\n a\n-b\n+B\n c\n-d\n@@ -10,3 +9,2 @@\n x\n-y\n z";
        assert_eq!(deleted_at(d), vec![2, 4, 10]);
    }
}
