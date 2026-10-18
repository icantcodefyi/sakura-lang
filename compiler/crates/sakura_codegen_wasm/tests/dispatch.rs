//! The dispatch loop's two forms (design §17.200 (3)): a function of 256 blocks keeps one
//! `br_table` over its selectors (a block and a resume case per block), one of 257 an outer
//! `br_table` over chunks of 256 selectors and an inner one per chunk.

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_lir::lir::*;
use wasmparser::{Operator, Parser, Payload};

/// A module holding `chain`, `n` blocks each jumping to the next, the last returning 42.
fn with_chain(n: u32) -> Vec<u8> {
    let src = "pub fn main() -> Unit ! {Console} { Console.print(\"x\") }\n";
    let snap = Snapshot::with_std([("file:///chain/src/main.sak".to_owned(), src.to_owned())]);
    let opts = BuildOptions {
        need_entry: true,
        ..Default::default()
    };
    let core = Analysis::lower_core(&snap, &opts).0.expect("lowers");
    let mut m = sakura_lir::from_core(&core, sakura_lir::Layout::Tagged).expect("LIR");
    let block = |insts, term| Block {
        params: vec![],
        regions: vec![],
        insts,
        term,
    };
    let mut blocks: Vec<Block> = (1..n)
        .map(|i| {
            let next = Edge {
                block: BlockId(i),
                args: vec![],
            };
            block(vec![], Term::Jump(next))
        })
        .collect();
    let x = Var(0);
    let answer = Inst::Const {
        dst: x,
        value: Const::I64(42),
    };
    blocks.push(block(vec![answer], Term::Return(x)));
    m.funcs.push(Func {
        name: "chain".into(),
        module: "main::main".into(),
        kind: FuncKind::Top { export: false },
        captures: vec![],
        vars: vec![VarDecl {
            name: "x".into(),
            repr: Repr::I64,
        }],
        blocks,
        ret: Repr::I64,
    });
    let errors = sakura_lir::validate::validate(&m, Default::default());
    assert!(errors.is_empty(), "{errors:?}");
    sakura_codegen_wasm::emit(&m, &Default::default()).expect("emits")
}

/// Per function body, the number of targets of each `br_table`, in order.
fn br_tables(wasm: &[u8]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let Payload::CodeSectionEntry(body) = payload.expect("parses") else {
            continue;
        };
        let mut tables = Vec::new();
        let mut ops = body.get_operators_reader().expect("operators");
        while !ops.eof() {
            if let Operator::BrTable { targets } = ops.read().expect("operator") {
                tables.push(targets.len() as usize);
            }
        }
        out.push(tables);
    }
    out
}

#[test]
fn past_256_blocks_the_dispatch_is_two_level() {
    if !sakura_codegen_wasm::available() {
        assert!(std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"));
        return;
    }
    // 256 blocks: one table over 512 selectors.
    let one = br_tables(&with_chain(256));
    assert!(one.contains(&vec![512]), "{one:?}");
    // 257 blocks: 514 selectors in chunks of 256, 256 and 2 under a table of 3 chunks.
    let two = br_tables(&with_chain(257));
    assert!(two.contains(&vec![3, 256, 256, 2]), "{two:?}");
    assert!(!two.iter().any(|t| t.contains(&514)), "{two:?}");
}
