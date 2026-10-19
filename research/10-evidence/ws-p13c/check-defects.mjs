import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const results = [];
for (const arm of ['base','owned','flat']) {
  const {program: p, runTask} = await import(pathToFileURL(resolve(process.argv[2], arm, "bench_entry.mjs")).href);
  const planner = runTask(p.planner, []).value;
  for (const [name,args,code] of [
    ['put_var',[planner,0,undefined],'D020'],
    ['put_con',[planner,-1,undefined],'D020'],
    ['var_of',[planner,0],'D030'],
    ['con_of',[planner,-1],'D030'],
  ]) {
    const result = runTask(p[name],args);
    assert.equal(result.code,code,JSON.stringify(result));
    results.push({arm,name,code:result.code});
  }
}
console.log(JSON.stringify(results));
