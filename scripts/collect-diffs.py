#!/usr/bin/env python3
"""Write a branch's per-file diffs against its merge-base as JSON: {path: {add, del, diff}}.

Usage: scripts/collect-diffs.py <repo-or-worktree> [base-ref=origin/main] > diffs.json
"""
import json
import subprocess
import sys


def git(repo, *args):
    return subprocess.check_output(['git', '-C', repo, *args]).decode()


def main():
    repo = sys.argv[1]
    base_ref = sys.argv[2] if len(sys.argv) > 2 else 'origin/main'
    base = git(repo, 'merge-base', 'HEAD', base_ref).strip()
    out = {}
    for line in git(repo, 'diff', '--numstat', base).splitlines():
        add, dele, path = line.split('\t')
        body = git(repo, 'diff', '-U3', base, '--', path).split('\n')
        start = next(i for i, l in enumerate(body) if l.startswith('@@'))
        out[path] = {'add': int(add), 'del': int(dele), 'diff': '\n'.join(body[start:]).rstrip('\n')}
    json.dump(out, sys.stdout)


if __name__ == '__main__':
    main()
