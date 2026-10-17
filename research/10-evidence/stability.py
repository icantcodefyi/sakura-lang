#!/usr/bin/env python3
import datetime
import json
from pathlib import Path
import random
import statistics
import subprocess
HERE = Path(__file__).resolve().parent
if __name__ == '__main__':
    rows = []
    rng = random.Random(20261018)
    for repetition in range(3):
        jobs = [(c,v) for c in ['effects','contracts_plain'] for v in ['sakura','js']]
        rng.shuffle(jobs)
        for case,variant in jobs:
            p = subprocess.run(['bun',str(HERE/'stability.mjs'),case,variant],text=True,capture_output=True,check=True,timeout=60)
            row=json.loads(p.stdout);row['repetition']=repetition;rows.append(row)
    (HERE/'stability.json').write_text(json.dumps({'date_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'rows':rows},indent=2)+'\n')
    for case in ['effects','contracts_plain']:
        times={v:statistics.median(statistics.median(r['samples_ms']) for r in rows if r['name']==case and r['variant']==v) for v in ['sakura','js']}
        print(case,times,'ratio',times['sakura']/times['js'])
