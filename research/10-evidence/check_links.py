#!/usr/bin/env python3
"""Check report links without installing tooling or storing third-party page bodies."""
import concurrent.futures
import datetime
import hashlib
import json
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request

HERE = Path(__file__).resolve().parent
REPORT = HERE.parent / '10-performance.md'

def check(url):
    # GET also checks sites that reject HEAD. Only hashes/metadata are retained.
    request = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 SakuraResearch/1.0'})
    try:
        with urllib.request.urlopen(request, timeout=25) as response:
            body = response.read(4_000_000)
            return {'url': url, 'status': response.status, 'final_url': response.url,
                    'sha256_prefix_up_to_4mb': hashlib.sha256(body).hexdigest()}
    except Exception as exc:
        return {'url': url, 'error': str(exc)}

if __name__ == '__main__':
    text = REPORT.read_text()
    urls = sorted(set(re.findall(r'https://[^\s<>]+', text)))
    urls = [u.rstrip(').,;') if not '%28' in u else u.rstrip('.,;') for u in urls]
    # Inline markdown destinations end with an extra close parenthesis.
    urls = sorted(set(u[:-1] if u.endswith(')') else u for u in urls))
    local = []
    for target in re.findall(r'\]\(([^)]+)\)', text):
        if not target.startswith(('https://', 'http://', '#')):
            path = REPORT.parent / urllib.parse.unquote(target.split('#')[0])
            local.append({'target': target, 'exists': path.exists()})
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        remote = list(pool.map(check, urls))
    data = {'checked_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'local': local, 'remote': remote}
    (HERE / 'links.json').write_text(json.dumps(data, indent=2) + '\n')
    print('local links',len(local),'missing',[x for x in local if not x['exists']])
    print('remote links',len(remote),'errors',[x for x in remote if 'error' in x])
