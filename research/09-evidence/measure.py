"""Reproduce research/09: independent WAT + hand-transcribed arithmetic obligations.
No compiler modification. Requires explicit paths to Node, Wasmtime, wasm-tools, Z3, cvc5.
Writes results and generated SMT/Wasm to --out; serial solver timings include process startup.
"""
import argparse, hashlib, json, platform, statistics, subprocess, time
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['node','wasmtime','wasm_tools','z3','cvc5','out']:p.add_argument('--'+n.replace('_','-'),required=True)
a=p.parse_args();out=Path(a.out);out.mkdir(parents=True,exist_ok=True)
here=Path(__file__).resolve().parent;root=here.parent.parent

def run(cmd, text=None, timeout=10):
 t=time.perf_counter_ns();r=subprocess.run(cmd,input=text,text=True,capture_output=True,timeout=timeout)
 if r.returncode:raise RuntimeError((cmd,r.returncode,r.stderr,r.stdout))
 return r.stdout.strip(),(time.perf_counter_ns()-t)/1e6
versions={n:run([getattr(a,n),'--version'])[0].splitlines()[0] for n in ['node','wasmtime','wasm_tools','z3','cvc5']}
wasm=out/'dispatch.wasm';run([a.wasm_tools,'parse',str(here/'dispatch.wat'),'-o',str(wasm)])
# The tail-call comparator is intentionally outside the M4 allowlist. Certify a separate module.
profile=out/'profile.wat';s=(here/'dispatch.wat').read_text()
start=s.index('  (func $tail');end=s.index('  (func (export "trampoline")',start)
s=s[:start]+s[end:];start=s.index('  (func (export "tail")');end=s.index('  (func (export "profile")',start)
s=s[:start]+s[end:];s=s.replace('(table 2 2 funcref)','(table 1 1 funcref)').replace('(elem (i32.const 0) $step $tail)','(elem (i32.const 0) $step)')
profile.write_text(s);pw=out/'profile.wasm';run([a.wasm_tools,'parse',str(profile),'-o',str(pw)])
features='-all,mutable-global,floats,multi-value,bulk-memory'
validation=run([a.wasm_tools,'validate','--features='+features,str(pw)])[0]
negative=subprocess.run([a.wasm_tools,'validate','--features='+features,str(wasm)],capture_output=True,text=True)
assert negative.returncode != 0 and 'tail' in negative.stderr
engine_profiles={
 'node':run([a.node,str(here/'run.mjs'),str(pw),'profile'])[0],
 'wasmtime':run([a.wasmtime,'run','--invoke','profile',str(pw)])[0],
}
bench=[]
# Warm one process per variant, then alternate 7 rounds. Each process executes 25 x 1M steps.
for rep in range(8):
 for engine in ['node','wasmtime']:
  for mode in (['trampoline','tail'] if rep%2 else ['tail','trampoline']):
   cmd=([a.node,str(here/'run.mjs'),str(wasm),mode,'25'] if engine=='node' else
        [a.wasmtime,'run','--invoke',mode,str(wasm),'25'])
   result,ms=run(cmd);assert result=='25000000',(engine,mode,result)
   if rep:bench.append(dict(engine=engine,mode=mode,rep=rep,ms_per_million=ms/25))
division=json.loads(run([a.node,str(here/'run.mjs'),str(wasm),'division','25',str(root/'compiler/runtime/js/sakura_runtime.js')])[0])
# AST shared by two exact encoders, but expected sat/unsat classifications set independently below.
def op(name,*xs):return (name,*xs)
def And(*xs):return op('and',*xs)
def le(x,y):return op('<=',x,y)
def ge(x,y):return op('>=',x,y)
def bounded(x,lo,hi):return And(ge(x,lo),le(x,hi))
lo=-(1<<63);hi=(1<<63)-1
cases=[]
def add(name,source,assume,claim,expected):cases.append(dict(name=name,source=source,assume=assume,claim=claim,expected=expected))
for i,x in enumerate([-1,0,50,100,101]):
 add(f'percent-{i}','examples/contracts/src/main.sakura:8',op('=', 'x',x),bounded('x',0,100),'unsat' if 0<=x<=100 else 'sat')
for i,x in enumerate([99,100,200,599,600]):
 add(f'status-{i}','std/response.sakura:16',op('=','x',x),bounded('x',100,599),'unsat' if 100<=x<=599 else 'sat')
for i,n in enumerate([0,1,10,100,1000]):
 result=op('ite',op('<','x',0),0,op('ite',op('>','x',n),n,'x'))
 add(f'clamp-{i}','std/int.sakura:37',True,bounded(result,0,n),'unsat')
for label,source in [('gen-list','std/gen.sakura:30'),('repeat','std/string.sakura:74'),('pow-domain','std/int.sakura:44'),('decimal','std/decimal.sakura:72')]:
 for i,n in enumerate([-1,0,1,20,100]):
  add(f'{label}-{i}',source,op('=','x',n),ge('x',0),'unsat' if n>=0 else 'sat')
for i,n in enumerate([-1,0,1,8,100]):
 add(f'agent-steps-{i}','std/agent.sakura:38',op('=','x',n),op('>','x',0),'unsat' if n>0 else 'sat')
for i,pct in enumerate([0,1,25,50,100]):
 product=op('*','x',pct)
 add(f'discount-overflow-{i}','examples/contracts/src/main.sakura:16',ge('x',0),bounded(product,lo,hi),'unsat' if pct<=1 else 'sat')
 # Normal-return postcondition: overflow checked separately; constant pct specialization.
 add(f'discount-post-{i}','examples/contracts/src/main.sakura:17',And(ge('x',0),bounded(product,lo,hi)),le(op('-', 'x', op('tdiv',product,100)),'x'),'unsat')
assert len(cases)==50

def enc(e,bv=False):
 if isinstance(e,bool):return str(e).lower()
 if isinstance(e,int):return f'(_ bv{e%(1<<128)} 128)' if bv else (str(e) if e>=0 else f'(- {-e})')
 if isinstance(e,str):return '((_ sign_extend 64) x)' if bv else e
 name,*args=e
 if bv:name={'<=':'bvsle','>=':'bvsge','<':'bvslt','>':'bvsgt','*':'bvmul','-':'bvsub','tdiv':'bvsdiv'}.get(name,name)
 return '('+name+' '+' '.join(enc(x,bv) for x in args)+')'
# BV uses 128-bit sign-extended intermediates for checked multiplication, never truncated products.
pre='''(define-fun abs_i ((x Int)) Int (ite (< x 0) (- x) x))
(define-fun tdiv ((a Int) (b Int)) Int
 (ite (= (< a 0) (< b 0)) (div (abs_i a) (abs_i b)) (- (div (abs_i a) (abs_i b)))))\n'''
rows=[]
for idx,c in enumerate(cases):
 for encoding in ['int','bv']:
  bv=encoding=='bv';s='(set-logic '+('QF_BV' if bv else 'QF_NIA')+')\n'
  s+='(declare-const x '+('(_ BitVec 64)' if bv else 'Int')+')\n'
  if not bv:s+=pre+'(assert '+enc(bounded('x',lo,hi))+')\n'
  s+='(assert '+enc(c['assume'],bv)+')\n(assert (not '+enc(c['claim'],bv)+'))\n(check-sat)\n'
  fn=out/f'{idx:02}-{c["name"]}-{encoding}.smt2';fn.write_text(s)
  for solver in ['z3','cvc5']:
   cmd=([a.z3,'-smt2','-in','timeout=1000','rlimit=1000000','smt.random_seed=0'] if solver=='z3' else
        [a.cvc5,'--lang=smt2','--tlimit-per=1000','--rlimit-per=1000000','--seed=0'])
   for rep in range(3):
    answer,ms=run(cmd,s,timeout=3)
    verdict=answer.splitlines()[0]
    assert verdict in [c['expected'],'unknown'],(c['name'],solver,encoding,answer,c['expected'])
    rows.append(dict(name=c['name'],source=c['source'],solver=solver,encoding=encoding,rep=rep,expected=c['expected'],verdict=verdict,ms=ms,sha256=hashlib.sha256(s.encode()).hexdigest()))
# Validate negative truncating division against both encodings/solvers, independently of timing corpus.
for solver in ['z3','cvc5']:
 constraints=[]
 for c in division['cases']:
  av,bv,q,r=(c[k] for k in ['a','b','q','r'])
  constraints.append(f'(not (and (= (tdiv {enc(av)} {enc(bv)}) {enc(q)}) (= (- {enc(av)} (* {enc(bv)} (tdiv {enc(av)} {enc(bv)}))) {enc(r)})))')
 check='(set-logic QF_NIA)\n'+pre+'(assert (or '+' '.join(constraints)+'))\n(check-sat)\n'
 cmd=[getattr(a,solver),'-in'] if solver=='z3' else [a.cvc5,'--lang=smt2']
 assert run(cmd,check)[0]=='unsat'
result=dict(platform=platform.platform(),versions=versions,profile=engine_profiles,negative_profile=negative.stderr,dispatch=bench,division=division,solver=rows)
(out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'versions':versions,'profile':engine_profiles},indent=2))
for engine in ['node','wasmtime']:
 for mode in ['trampoline','tail']:
  xs=[r['ms_per_million'] for r in bench if r['engine']==engine and r['mode']==mode]
  print(engine,mode,'median/min/max ms per 1M',statistics.median(xs),min(xs),max(xs))
for solver in ['z3','cvc5']:
 for encoding in ['int','bv']:
  rs=[r for r in rows if r['solver']==solver and r['encoding']==encoding];xs=sorted(r['ms'] for r in rs)
  print(solver,encoding,'median/p95/max ms',statistics.median(xs),xs[int(.95*(len(xs)-1))],max(xs),'unknown',sum(r['verdict']=='unknown' for r in rs))
