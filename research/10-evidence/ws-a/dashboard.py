#!/usr/bin/env python3
"""Create the self-contained review table from the retained WS-A JSON evidence."""
import html
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def read(path):
    return json.loads(path.read_text())


def main():
    local = read(HERE / 'local-noise-summary.json')
    groups = [(f'Local / {engine}', [r for r in local['rows'] if r['engine'] == engine])
              for engine in ('node', 'bun')]
    groups += [(name, read(HERE / f'ci/{path}-summary.json')['rows']) for name, path in (
        ('Ubuntu / Node', 'ubuntu-node'), ('macOS / Node', 'macos-node'), ('macOS / Bun', 'macos-bun'))]
    rows = []
    for label, values in groups:
        worst = max(values, key=lambda r: r['max_increase_over_min'])
        rows.append(f'<tr><th scope="row">{html.escape(label)}</th><td>{worst["program"]}<br><small>n = {worst["size"]:,}</small></td>'
                    f'<td>{worst["min_ratio"]:.2f}–{worst["max_ratio"]:.2f}×</td>'
                    f'<td>{100 * worst["max_increase_over_min"]:.1f}%</td></tr>')
    record = HERE / 'candidate-record.json'
    candidate_path = ROOT / read(record)['path'] if record.exists() else None
    candidate_html = '<p>The full candidate measurement is still running.</p>'
    if candidate_path is not None and candidate_path.exists():
        candidate = read(candidate_path)
        indexed = {(r['program'], r['engine'], r['size'], r['variant']): r for r in candidate['results']}
        largest = {}
        for p, e, n, _v in indexed:
            largest[p, e] = max(n, largest.get((p, e), n))
        ranked = sorted(((indexed[p, e, n, 'sakura']['median_ms'] / indexed[p, e, n, 'js']['median_ms'], p, e, n)
                         for (p, e), n in largest.items()), reverse=True)[:5]
        top_rows = ''.join(f'<tr><th scope="row">{p}</th><td>{e}</td><td>{n:,}</td><td>{ratio:.2f}×</td></tr>'
                           for ratio, p, e, n in ranked)
        candidate_html = f'''<p class="section"><strong>Largest candidate ratios</strong><br>
        <small>Largest input per program/engine · one full run · candidate remains unblessed.</small></p>
        <table aria-label="Largest candidate Sakura to hand-JS ratios"><thead><tr><th scope="col">Program</th><th scope="col">Engine</th><th scope="col">Input</th><th scope="col">Ratio</th></tr></thead><tbody>{top_rows}</tbody></table>
        <p class="note">DeltaBlue has a known validation-work mismatch: the JS control performs two assertion loops omitted by Sakura. Its ratio describes the current suite, not a fully matched competitiveness result. Resolve or review this before human adoption.</p>'''
    latency_html = ''
    if (HERE / 'compile-current.json').exists():
        failures = [r for r in read(HERE / 'compile-current.json')['results'] if r['p95_ms'] > r['budget_ms']]
        if failures:
            latency_html = '<p class="note"><strong>Compile-latency miss:</strong> ' + '; '.join(
                f"{r['program']} p95 {r['p95_ms']:.1f} ms exceeds its {r['budget_ms']} ms budget" for r in failures) + '. Retained without changing the budget; 15 samples, so nearest-rank p95 is the maximum.</p>'
    page = '''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Sakura WS-A measurement evidence</title>
    <style>
    body{margin:0;color:var(--foreground,#222);font-family:var(--font-sans,system-ui,sans-serif);font-size:14px;line-height:1.5}
    p{margin:0 0 14px}.decision{font-size:17px}.note,small{color:var(--muted-foreground,#60666c);font-size:12px}
    .section{margin-top:24px}table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums;margin-bottom:10px;font-size:13px}
    th,td{padding:8px 6px;border-bottom:1px solid var(--border,#ddd);text-align:right;vertical-align:top}th:first-child,td:nth-child(2){text-align:left}thead th{font-size:12px;color:var(--muted-foreground,#60666c);font-weight:500}tbody th{font-weight:500}
    th:first-child{padding-left:0}td:last-child,th:last-child{padding-right:0}a{color:var(--accent-foreground,#2764a1)}
    @media(max-width:450px){table{font-size:12px}th,td{padding:8px 3px}.decision{font-size:15px}small{font-size:10px}}
    </style></head><body>
    <p class="decision"><strong>Keep the 5% warning / 10% confirmed block.</strong><br>Retain the 0.1 ms floor; these measurements do not support tighter thresholds.</p>
    <table aria-label="Worst observed unchanged-code ratio spread per series"><thead><tr><th scope="col">Series</th><th scope="col">Widest cell</th><th scope="col">Ratio range</th><th scope="col">Max / min − 1</th></tr></thead><tbody>''' + ''.join(rows) + '''</tbody></table>
    <p class="note">Five complete runs per cell. Local analysis retains four programs under heavy shared-host contention (1-minute load 4.76–62.10 on 12 CPUs); CI covers fib and pipeline within one runner allocation per job. Historical sieve data is excluded after the algorithm change at 5b10553. Empirical ranges, not confidence intervals. The two macOS jobs used different virtual hardware.</p>
    <p class="note">An unchanged-source macOS/Node comparison also triggered the independent two-size fib complexity heuristic. Tightening timing thresholds would not repair that gate.</p>
    ''' + candidate_html + latency_html + '''
    <p class="note">All raw samples, load snapshots, comparison verdicts, provenance and human-only blessing instructions are in the <a href="https://github.com/icantcodefyi/sakura-lang/blob/main/research/10-evidence/ws-a/README.md">WS-A report</a>.</p>
    </body></html>'''
    (HERE / 'measurement-summary.html').write_text(page + '\n')


if __name__ == '__main__':
    main()
