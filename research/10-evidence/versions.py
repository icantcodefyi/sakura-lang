#!/usr/bin/env python3
"""Record official release metadata available at the research cutoff (no installs)."""
import concurrent.futures
import datetime
import json
from pathlib import Path
import urllib.request
REPOS = ['koka-lang/koka', 'effekt-lang/effekt', 'ocaml/ocaml', 'matijapretnar/eff',
         'leanprover/lean4', 'unisonweb/unison', 'rescript-lang/rescript', 'elm/compiler',
         'purescript/purescript', 'gleam-lang/gleam', 'scala-js/scala-js',
         'oxc-project/oxc', 'biomejs/biome', 'roc-lang/roc', 'sharkdp/hyperfine',
         'rust-lang/rust', 'golang/go', 'python/cpython']
CUTOFF = '2026-10-18T23:59:59Z'
def get(repo):
    url = f'https://api.github.com/repos/{repo}/releases?per_page=30'
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Sakura-performance-research'})
        with urllib.request.urlopen(req, timeout=25) as res: releases = json.load(res)
        stable = [r for r in releases if not r['prerelease'] and not r['draft'] and r['published_at'] <= CUTOFF]
        if not stable: return {'repo': repo, 'query_url': url, 'status': 'no stable release in first 30; version unverified'}
        r = max(stable, key=lambda r: r['published_at'])
        return {'repo': repo, 'query_url': url, 'version': r['tag_name'], 'published_at': r['published_at'], 'url': r['html_url']}
    except Exception as exc: return {'repo': repo, 'query_url': url, 'error': str(exc)}
if __name__ == '__main__':
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: rows = list(pool.map(get, REPOS))
    data = {'retrieved_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'cutoff': CUTOFF, 'releases': rows}
    Path(__file__).with_name('versions.json').write_text(json.dumps(data, indent=2) + '\n')
    print(json.dumps(data, indent=2))
