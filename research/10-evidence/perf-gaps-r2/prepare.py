"""Reproduce emitted-only diagnostic variants. Never edits compiler, std or benchmarks.
Run from repository root after building the CLI; optional SAKURA_CLI / PERF_TMP.
These ablations are NOT semantics-preserving compiler implementations.
"""
from pathlib import Path
import os, subprocess, shutil, re, json, hashlib, difflib
ROOT = Path.cwd()
OUT = Path(os.environ.get('PERF_TMP', '/tmp/sakura-perfgaps-r2'))
CLI = os.environ.get('SAKURA_CLI', '/Users/aniruddhdubge/.t3/scratch/sakura-targets/perfgaps/release/sakura')
EVIDENCE = ROOT / 'research/10-evidence/perf-gaps-r2'

def body(text, name, replacement):
    pat = re.compile(r'(?:export )?function ' + re.escape(name) + r'\([^\n]*\) \{')
    m = pat.search(text)
    assert m, name
    start, depth, end = m.end(), 1, m.end()
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[:start] + '\n' + replacement + '\n' + text[end-1:]

def edit(d, file, fn):
    p = d/file; before=p.read_text(); after=fn(before)
    assert before != after, (d,file)
    p.write_text(after)

def variant(p, name, transforms):
    d=OUT/p/name
    if d.exists(): shutil.rmtree(d)
    shutil.copytree(OUT/p/'base',d)
    for file,fn in transforms: edit(d,file,fn)
    return d

MAIN='main/main.js'; RT='sakura_runtime.js'; STD='std/list.js'
INPLACE='''  const off = xs.n - xs.tl;
  let leaf = xs.tail, index = i - off;
  if (i < off) {
    leaf = xs.root;
    for (let level = xs.shift; level > 0; level -= 5) leaf = leaf[(i >>> level) & 31];
    index = i & 31;
  }
  leaf[index] = x;
  xs.lb = -1; xs.lf = null;
  return xs;'''
NOINT=lambda s: re.sub(r'^\s*if \(!Number\.isSafeInteger\([^\n]+\n','\n',s,flags=re.M)
NOCONTRACT=lambda s: body(s,'set$d','  return $rt.list_set(xs, i, x);')
SET=(MAIN, lambda s: body(s,'set$d','  return $rt.list_set(bs, i, b);'))
REUSE=lambda s: s.replace('new Body(x_2, y_2, z_2, t_15, t_18, t_21, mass_3)', '(b_1.vx = t_15, b_1.vy = t_18, b_1.vz = t_21, b_1)').replace('new Body(x_3, y_3, z_3, t_24, t_27, t_30, mass_7)', '(r_2.vx = t_24, r_2.vy = t_27, r_2.vz = t_30, r_2)').replace('new Body(t_1, t_3, t_5, vx_1, vy_1, vz_1, mass)', '(b.x = t_1, b.y = t_3, b.z = t_5, b)')
CACHE=(RT,lambda s: body(s,'list_get','  return i >= 0 && i < xs.n ? Some(list_at(xs, i)) : None;'))
FUSED=(MAIN,lambda s: body(s,'at$d','  if (i < 0 || i >= v.n) return $rt.defect("D030", "vector index");\n  return $rt.list_at(v, i);'))
EVEN=(MAIN,lambda s: s.replace('((t_3 - t_3 % 2) / 2 + 0)','(t_3 / 2 + 0)'))

def flatten(s):
    # Only the two matrix-vector kernels; one O(n) copy per O(n²) multiplication.
    for name in ['mul_av$d','mul_atv$d']:
        a=s.index('export function '+name+'('); b=s.index('\n}\n',a)
        part=s[a:b].replace('  const xs = $rt.list([]);','  const flat = $rt.toArray(v);\n  const xs = $rt.list([]);').replace('at$d(v, j_1)','flat[j_1]')
        s=s[:a]+part+s[b:]
    return s

def simple_loops(s):
    for name,call in [('mul_av$d','a$d(i, j)'),('mul_atv$d','a$d(j, i)')]:
        s=body(s,name,f'''  let out = $rt.list([]);
  for (let i = 0; i < n; i++) {{
    let sum = 0.0;
    for (let j = 0; j < n; j++) sum = sum + {call} * at$d(v, j);
    out = $rt.list_push(out, sum);
  }}
  return out;''')
    return s

def flat_sieve(d):
    # Keep generated loops/Int checks and std contract/Option; use fresh private arrays.
    for name,code in {
        'list':'  return arr.slice();', 'list_len':'  return xs.length;',
        'list_push':'  xs.push(x); return xs;', 'list_at':'  return xs[i];',
        'list_get':'  return i >= 0 && i < xs.length ? Some(xs[i]) : None;',
        'list_set':'  xs[i] = x; return xs;'
    }.items(): edit(d,RT,lambda s,name=name,code=code: body(s,name,code))

def main():
    OUT.mkdir(exist_ok=True)
    manifest={'commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(), 'variants':{}, 'hashes':{}}
    for p in ['sieve','nbody','spectral_norm']:
        d=OUT/p/'base'; d.mkdir(parents=True,exist_ok=True)
        src=ROOT/f'bench/programs/{p}/sakura/main.sakura'
        subprocess.run([CLI,'build',str(src),'--out-dir',str(d)],check=True)
        for dump in ['direct','core']:
            r=subprocess.run([CLI,'build',str(src),'--dump',dump],check=True,capture_output=True,text=True)
            (EVIDENCE/f'{p}-{dump}.txt').write_text(r.stdout.rstrip()+'\n')
        (EVIDENCE/f'{p}-generated.js.txt').write_text((d/MAIN).read_text())
        lines=(d/'__main__.js').read_text().splitlines()
        kept=[l.replace('import { registerOps, runMain }','import { registerOps }') for l in lines if not l.startswith('runMain(') and '$entry' not in l]
        (d/'bench_entry.mjs').write_text('\n'.join(kept+['export * as program from "./main/main.js";','export { runTask } from "./sakura_runtime.js";'])+'\n')
        # Isolate the single public CPS return/gauge; task driver unchanged.
        variant(p,'no_boundary_gauge',[(RT,lambda s: body(s,'ret','  return k(v);'))])
    variant('sieve','unique_trie',[(RT,lambda s: body(s,'setAt',INPLACE))])
    variant('sieve','no_contract',[(STD,NOCONTRACT),(MAIN,lambda s: s.replace('                      $S.s = $c0;\n',''))])
    variant('sieve','no_int_checks',[(MAIN,NOINT)])
    variant('sieve','unique_no_contract',[(RT,lambda s: body(s,'setAt',INPLACE)),(STD,NOCONTRACT)])
    flat_sieve(variant('sieve','private_array',[]))
    variant('nbody','indexed_set',[SET])
    variant('nbody','reuse_records',[(MAIN,REUSE)])
    variant('nbody','indexed_reuse',[SET,(MAIN,REUSE)])
    variant('nbody','unique_list',[SET,(RT,lambda s: body(s,'setAt',INPLACE))])
    variant('nbody','unique_both',[SET,(RT,lambda s: body(s,'setAt',INPLACE)),(MAIN,REUSE)])
    # Same positions, fresh output vector, direct known callback: no higher-order/bind overhead.
    def map_direct(s):
        old='''  const f = advance$1.bind(null, dt);
  f.$d = advance$1$d.bind(null, dt);
  const r_6 = $std_list.map$d(bs, f);'''
        new='''  const builder = $rt.list_builder();
  for (let k = 0; k < $rt.list_len(bs); k++) $rt.list_builder_push(builder, advance$1$d(dt, $rt.list_at(bs, k)));
  const r_6 = $rt.list_builder_finish(builder);'''
        assert old in s
        return s.replace(old,new)
    variant('nbody','direct_map',[(MAIN,map_direct)])
    variant('nbody','unique_both_map',[SET,(RT,lambda s: body(s,'setAt',INPLACE)),(MAIN,REUSE),(MAIN,map_direct)])
    # Flatten class allocation to stable object literals, retaining copy-on-update.
    def literal(s):
        def rep(m):
            args=m[1].split(', '); assert len(args)==7
            return '({'+', '.join(f'{k}: {v}' for k,v in zip(['x','y','z','vx','vy','vz','mass'],args))+'})'
        return re.sub(r'new Body\(([^()]+)\)',rep,s)
    variant('nbody','object_literals',[(MAIN,literal)])
    variant('spectral_norm','cached_get',[CACHE])
    variant('spectral_norm','fused_at',[FUSED])
    variant('spectral_norm','no_int_checks',[(MAIN,NOINT)])
    variant('spectral_norm','even_division',[EVEN])
    variant('spectral_norm','flat_read',[(MAIN,flatten)])
    variant('spectral_norm','arithmetic',[(MAIN,NOINT),EVEN])
    variant('spectral_norm','fused_arithmetic',[FUSED,(MAIN,NOINT),EVEN])
    variant('spectral_norm','flat_arithmetic',[(MAIN,flatten),(MAIN,NOINT),EVEN])
    variant('spectral_norm','simple_loops',[(MAIN,simple_loops)])
    patches=[]
    for p in ['sieve','nbody','spectral_norm']:
        manifest['variants'][p]=sorted(d.name for d in (OUT/p).iterdir() if d.is_dir())
        for name in manifest['variants'][p]:
            for f in [MAIN,RT,STD,'bench_entry.mjs']:
                x=OUT/p/name/f
                if not x.exists(): continue
                manifest['hashes'][f'{p}/{name}/{f}']=hashlib.sha256(x.read_bytes()).hexdigest()
                base=OUT/p/'base'/f
                if name!='base' and base.exists():
                    patches.extend(difflib.unified_diff(base.read_text().splitlines(True),x.read_text().splitlines(True),fromfile=f'{p}/base/{f}',tofile=f'{p}/{name}/{f}', n=0))
    (EVIDENCE/'variants.patch').write_text(''.join(patches))
    (EVIDENCE/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps(manifest['variants'],indent=2))

if __name__=='__main__': main()
