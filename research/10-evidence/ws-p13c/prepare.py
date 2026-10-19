import difflib
import re
import shutil
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
evidence = Path(__file__).resolve().parent
base = root / 'base'
lines = (base / '__main__.js').read_text().splitlines()
kept = [line.replace('import { registerOps, runMain }', 'import { registerOps }')
        for line in lines if not line.startswith('runMain(') and '$entry' not in line]
kept += ['export * as program from "./main/main.js";',
         'export { runTask } from "./sakura_runtime.js";']
(base / 'bench_entry.mjs').write_text('\n'.join(kept) + '\n')
source = (base / 'main/main.js').read_text()

def change_function(source, name, replacements):
    pattern = r'(export function ' + re.escape(name) + r'\([^\n]*\) \{\n)(.*?)(\n\})'
    match = re.search(pattern, source, re.S)
    assert match, name
    body = match[2]
    for old, new in replacements:
        assert body.count(old) == 1, (name, old)
        body = body.replace(old, new)
    return source[:match.start(2)] + body + source[match.end(2):]

owned = source
for name in ['put_var$d', 'put_con$d']:
    owned = change_function(owned, name, [('$std_list.set$d(', '$std_list.set$o(')])
for name in ['new_var$d', 'add_constraint']:
    owned = change_function(owned, name, [('$rt.list_push(', '$rt.list_push_owned(')])
flat = owned
flat = change_function(flat, 'planner$d', [
    ('const xs = $rt.list([]);', 'const xs = [];'),
    ('const xs_1 = $rt.list([]);', 'const xs_1 = [];')])
for name, field in [('new_var$d', 'vars'), ('add_constraint', 'cons')]:
    flat = change_function(flat, name, [
        (f'$rt.list_len({field})', f'{field}.length'),
        ('$rt.list_push_owned(', '$densePush(')])
for name in ['put_var$d', 'put_con$d']:
    flat = change_function(flat, name, [('$std_list.set$o(', '$denseSet(')])
for name in ['var_of$d', 'con_of$d']:
    flat = change_function(flat, name, [('$rt.list_get_expect(', '$denseGet(')])
std = (base / 'std/list.js').read_text()
body = re.search(r'export function set\$o\(xs, i, x, \$site\) \{\n(.*?)\n\}', std, re.S)[1]
contract_site = re.search(r'const \$c0 = (.*);', std)[1]
body = body.replace('$rt.list_len(xs)', 'xs.length').replace('$rt.list_set_owned(xs, i, x)', '(xs[i] = x, xs)').replace('$c0', contract_site)
flat += '\nfunction $denseSet(xs, i, x, $site) {\n' + body + '\n}\n'
flat += '''
function $densePush(xs, x) {
  xs.push(x);
  return xs;
}
function $denseGet(xs, i, message) {
  return i >= 0 && i < xs.length
    ? xs[i] : $rt.option_expect($rt.None, message);
}
'''
for arm, before, after in [('owned', source, owned), ('flat', owned, flat)]:
    shutil.copytree(base, root / arm, dirs_exist_ok=True)
    (root / arm / 'main/main.js').write_text(after)
    (root / (arm + '.patch')).write_text(''.join(difflib.unified_diff(
        before.splitlines(True), after.splitlines(True), n=0,
        fromfile='a/main/main.js', tofile='b/main/main.js')))
