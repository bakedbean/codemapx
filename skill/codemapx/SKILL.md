---
name: codemapx
description: Use when the work on a branch is finished and the user wants to review it as a map (they run /codemapx), or when they want to review someone's pull request as a map (they run /codemapx <pr number or url>). Collects facts about the branch, writes annotations.json explaining how the changes connect, and validates it so `codemapx` can show it.
---

# codemapx: map this branch for review

You wrote the code on this branch, so you know why each edit was needed. The user doesn't, and they're about to review it. Your job is to explain how the changes connect. Code supplies the facts: files, diffs, line numbers, candidate links. You supply the judgement and the prose.

If the user passed a PR number or URL, follow **Reviewing a pull request** below instead of step 1, then continue from step 2.

## Steps

1. Commit your work first. The map covers committed changes only.
2. Run `codemapx collect` in the worktree (add `--base <ref>` if the branch isn't based on `origin/main` or `main`). It prints a map dir. Read the facts without diffs first, e.g. `jq 'del(.files[].diff)' <dir>/facts.json`, then look at individual files' diffs as you need them.
3. If `<dir>/annotations.json` already exists, it was carried over from an earlier commit. Update it rather than starting over: set `head` to the new facts head and fix every problem validate reports. Carried link and dropped ids were remapped to the new candidate ids; a `gone:<from>-><to>` id names a pair that no longer exists, so remove it.
4. Write `<dir>/annotations.json` (format below).
5. Run `codemapx validate`. Fix every problem it lists and run it again until it prints `0 problem(s)`. **Never edit `facts.json`.**
6. Tell the user: "Map ready: run `codemapx`." Mention `codemapx html -o map.html` if they want to share it.

## Reviewing a pull request

Here someone else wrote the code, so you don't know why each edit was needed. Work it out from the PR title and body, the commit messages, linked issues, and the code itself. Where the reason isn't clear, say so in the `what` or `reason` (e.g. "Unclear why this was needed; the PR doesn't say.") rather than guessing. Those gaps are useful to the reviewer.

1. Read the PR: `gh pr view <pr> --json number,title,body,baseRefName,headRefName,state,url,closingIssuesReferences`. If the URL names a different repo from this worktree's (`gh repo view --json nameWithOwner`), stop and tell the user. If the PR is merged, tell the user the map would be empty and stop.
2. Check that the worktree is safe to overwrite. `git status --porcelain` must be empty. Then run `git fetch origin <baseRefName> "+pull/<number>/head:refs/remotes/origin/pr/<number>"` and `git log --oneline HEAD --not "origin/<baseRefName>" "origin/pr/<number>"`. If that lists commits, they exist only on this branch and the reset below would discard them, so show them to the user and ask before going on. A PR that was force-pushed since the last checkout also shows up here, as its old commits.
3. Move the current branch to the PR: `git reset --hard "origin/pr/<number>"`. If HEAD is detached, stop and ask the user to check out a branch first. Don't create, switch or rename branches: the branch's name belongs to whatever set up the worktree, and renaming it before or after mapping is fine. Rerun steps 2–3 to pick up new pushes.
4. Continue from step 2 of **Steps**, passing `--base "origin/<baseRefName>"` to `codemapx collect`. Don't commit anything.
5. For `context`, include the unchanged files you read to understand the PR. Use `missing` for anything the PR references that doesn't exist.

## What to write

- `"columns"`: groups ordered cause → effect, left to right. Shared contracts first, then the code that uses them, then entry points, then `Tests` and `Docs`. Name the Tests and Docs columns exactly that; narrow terminals collapse them. Every changed file goes in exactly one column, and so does every context card.
- `"files"`: one `what` per changed file, 1–3 sentences for someone who hasn't seen the code. Say what changed and why it was needed, not how it's written. The optional `outline` maps outline entry names from facts.json to one line each.
- `"links"` and `"dropped"`: every candidate in facts.json goes in exactly one of them.
  - Keep a link when an edit in `from` made the edit in `to` necessary, and give a one-line `reason`.
  - Drop it with a `why` when the import is incidental, e.g. a type-only import of something unchanged.
- `"context"`: unchanged files you read during the session that shaped the edits, such as another writer of the same rows or the caller whose contract you kept. Only include ones that matter.
- `"context_links"`: link each context card to the file it shaped. `evidence` must quote a real line of the file at HEAD, as `{ "path", "line", "quote" }`, and the validator checks the quote against that line.
- `"missing"`: things the branch references but nobody built yet, e.g. a job nothing enqueues. Give each an `id`, a `name`, a `why`, and `near` (a changed file). Ids must be unique and must not equal any file path.
- `"trail"`: the reading order, every changed file once, plus any context cards worth reading in sequence. Start where the story starts, which is usually the contract that everything else follows from.
- `"title"` and `"summary"`: one line each, for the header.
- `"head"`: copy it from facts.json.

## Example (abridged)

```json
{
  "version": 1,
  "head": "<facts.json head>",
  "title": "Apply regenerated fees",
  "summary": "Adds an apply step that writes regenerated fee rows.",
  "columns": [
    { "name": "Shared contracts", "files": ["src/billing/types.ts", "src/billing/mills.ts"] },
    { "name": "Apply", "files": ["src/billing/apply.ts", "src/jobs/fee-writer.ts"] },
    { "name": "Tests", "files": ["src/billing/mills.test.ts"] }
  ],
  "files": {
    "src/billing/mills.ts": { "what": "Adds millsToDecimal, the reverse of toMills, so amounts are written back exactly.", "outline": { "millsToDecimal": "Throws on non-integer mills." } }
  },
  "links": [{ "candidate": "c2", "reason": "apply formats amounts with millsToDecimal." }],
  "dropped": [{ "candidate": "c4", "why": "Type-only import of the unchanged Fee type." }],
  "context": [{ "path": "src/jobs/fee-writer.ts", "what": "Another writer of fee rows; apply must produce the same shape." }],
  "context_links": [{
    "from": "src/jobs/fee-writer.ts", "to": "src/billing/apply.ts",
    "reason": "Both write fee rows, so apply mirrors writeFee's fields.",
    "evidence": { "path": "src/jobs/fee-writer.ts", "line": 3, "quote": "export function writeFee(fee: Fee): void {" }
  }],
  "missing": [{ "id": "enqueue", "name": "Enqueue regeneration", "why": "Nothing calls the route yet.", "near": "src/api/route.ts" }],
  "trail": ["src/billing/types.ts", "src/billing/mills.ts", "src/billing/apply.ts", "src/jobs/fee-writer.ts"]
}
```
