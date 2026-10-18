//! Runtime defects a checked program cannot reach, provoked by mutating lowered LIR: the
//! interpreter's defence in depth (ABI §7.4, design §12.3 `D092`–`D096`).

mod common;

use common::big_stack;
use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_eval::lir as interp;
use sakura_lir::lir::*;

/// `body` is an operation callback: it holds its perform site's activation id, not a reference.
const CALLBACK: &str = "\
effect E { fn run(body: fn() -> Int) -> Int }

fn h() -> Handler<E> { handler E { fn run(body) { body() } } }

pub fn main() -> Unit ! {Console} {
  let r = with h() { E.run(fn() { 7 }) }
  Console.print(\"{r}\")
}
";

fn lowered(src: &str) -> Module {
    let snapshot = Snapshot::with_std([("file:///main.sak".to_owned(), src.to_owned())]);
    let opts = BuildOptions {
        need_entry: true,
        ..BuildOptions::default()
    };
    let (core, diags) = Analysis::lower_core(&snapshot, &opts);
    let core = core.unwrap_or_else(|| panic!("{diags:?}"));
    sakura_lir::from_core(&core, sakura_lir::Layout::Tagged).unwrap()
}

fn new_var(f: &mut Func, name: &str, repr: Repr) -> Var {
    f.vars.push(VarDecl {
        name: name.into(),
        repr,
    });
    Var(f.vars.len() as u32 - 1)
}

/// `D094 context_escaped`: a callback kept past its `with` and called after the activation it
/// names was popped. The escape check (§5.5, `V6002`/`V6003`) rejects such a program, so the
/// mutation keeps a second reference to the callback across `handle_pop` and calls it there.
#[test]
fn a_callback_outliving_its_perform_site_is_d094() {
    big_stack(|| {
        let mut m = lowered(CALLBACK);
        let unmutated = interp::run_main(&m, interp::Limits::default());
        assert_eq!(
            (unmutated.ending.summary(), unmutated.stdout.as_str()),
            ("ok".into(), "7\n")
        );
        let main = m.funcs.iter().position(|f| f.name == "main").unwrap();
        let f = &mut m.funcs[main];
        // The block that makes the callback and performs `E.run` with it.
        let site = f
            .blocks
            .iter()
            .position(|b| {
                b.insts
                    .iter()
                    .any(|i| matches!(i, Inst::MakeCallback { .. }))
            })
            .unwrap();
        let cb = f.blocks[site]
            .insts
            .iter()
            .find_map(|i| match i {
                Inst::MakeCallback { dst, .. } => Some(*dst),
                _ => None,
            })
            .unwrap();
        f.blocks[site].insts.push(Inst::Dup(cb));
        let Term::Perform { cont, .. } = &mut f.blocks[site].term else {
            panic!("the callback's block ends with its perform");
        };
        cont.args.push(cb);
        let after = cont.block.index();
        // The continuation pops the handler; instead of going on, it calls the kept callback.
        let kept = new_var(f, "kept", Repr::Ptr);
        let r = new_var(f, "r", Repr::Word);
        let v = new_var(f, "v", Repr::I64);
        let next = f.blocks[after].term.clone();
        let Term::Jump(Edge { block: rest, .. }) = next else {
            panic!("the continuation pops and jumps on");
        };
        f.blocks[after].params.push(kept);
        f.blocks[after].term = Term::Call {
            callee: Callee::Closure(kept),
            args: vec![],
            cont: Edge {
                block: BlockId(f.blocks.len() as u32),
                args: vec![],
            },
        };
        f.blocks.push(Block {
            params: vec![r],
            regions: vec![],
            insts: vec![Inst::Unbox { dst: v, src: r }, Inst::Drop(r)],
            term: Term::Jump(Edge {
                block: rest,
                args: vec![v],
            }),
        });

        let out = interp::run_main(&m, interp::Limits::default());
        assert_eq!(out.ending.summary(), "D094", "{:?}", out.ending);
        assert_eq!(out.stdout, "");
        assert!(out.stats.balanced(), "{:?}", out.stats);
    });
}
