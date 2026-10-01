#!/usr/bin/env python3
"""Render a change map to a single HTML page from web/filemap.tpl.html.

Usage: scripts/render-html.py <config.json> <diffs.json> <title> > change-map.html
"""
import json
import pathlib
import sys

TEMPLATE = pathlib.Path(__file__).resolve().parent.parent / 'web' / 'filemap.tpl.html'


def main():
    config_path, diffs_path, title = sys.argv[1:4]
    # Escape "</" so embedded JSON can't close the <script> tag.
    config = json.dumps(json.load(open(config_path))).replace('</', '<\\/')
    diffs = open(diffs_path).read().replace('</', '<\\/')
    page = TEMPLATE.read_text().replace('/*TITLE*/', title).replace('/*CONFIG*/', config).replace('/*DIFFS*/', diffs)
    sys.stdout.write(page)


if __name__ == '__main__':
    main()
