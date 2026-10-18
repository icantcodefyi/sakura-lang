//! The linker: the `sakura_rt` wasm object (built by `build.rs`) merged with the compiled
//! program into one module (ABI §1, §9). The runtime's sections are re-encoded with its
//! function indices shifted past the program's imports, its `sakura_link.dispatch` import
//! resolved to the synthesised dispatcher, its `rt_*` exports resolved by name for compiled
//! code and dropped from the export list, and the ABI §9 exports synthesised.

use std::collections::HashMap;

use wasm_encoder::reencode::{self, Reencode};
use wasm_encoder::{
    CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection, Elements, EntityType,
    ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemorySection, MemoryType, Module, NameMap, NameSection, RefType, TableSection, TableType,
    TypeSection, ValType,
};
use wasmparser::{ExternalKind, Parser, Payload, TypeRef};

/// What the linker keeps of the runtime object.
pub struct Runtime<'a> {
    bytes: &'a [u8],
    types: Vec<wasmparser::FuncType>,
    /// `(module, name, type)` of every function import, in order.
    imports: Vec<(String, String, u32)>,
    /// Type index of every defined function.
    funcs: Vec<u32>,
    table_min: u64,
    memory: Option<wasmparser::MemoryType>,
    globals: Vec<(wasmparser::GlobalType, Vec<u8>)>,
    exports: HashMap<String, (ExternalKind, u32)>,
    /// Active element segments: offset and function indices (runtime numbering).
    elements: Vec<(u32, Vec<u32>)>,
    /// Active data segments: offset and bytes.
    data: Vec<(u32, &'a [u8])>,
    bodies: Vec<wasmparser::FunctionBody<'a>>,
    names: HashMap<u32, String>,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("the sakura_rt wasm object: {e}")
}

/// The `i32.const` value of a constant offset expression.
fn const_i32(e: &wasmparser::ConstExpr<'_>) -> Result<u32, String> {
    let mut r = e.get_operators_reader();
    match r.read().map_err(err)? {
        wasmparser::Operator::I32Const { value } => Ok(value as u32),
        op => Err(err(format!("unsupported offset expression {op:?}"))),
    }
}

impl<'a> Runtime<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Runtime<'a>, String> {
        let mut rt = Runtime {
            bytes,
            types: vec![],
            imports: vec![],
            funcs: vec![],
            table_min: 0,
            memory: None,
            globals: vec![],
            exports: HashMap::new(),
            elements: vec![],
            data: vec![],
            bodies: vec![],
            names: HashMap::new(),
        };
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(err)? {
                Payload::TypeSection(s) => {
                    for ty in s.into_iter_err_on_gc_types() {
                        rt.types.push(ty.map_err(err)?);
                    }
                }
                Payload::ImportSection(s) => {
                    for imports in s {
                        for item in imports.map_err(err)? {
                            let (_, i) = item.map_err(err)?;
                            match i.ty {
                                TypeRef::Func(t) => {
                                    rt.imports.push((i.module.into(), i.name.into(), t))
                                }
                                _ => return Err(err(format!("non-function import {}", i.name))),
                            }
                        }
                    }
                }
                Payload::FunctionSection(s) => {
                    for t in s {
                        rt.funcs.push(t.map_err(err)?);
                    }
                }
                Payload::TableSection(s) => {
                    for t in s {
                        rt.table_min = rt.table_min.max(t.map_err(err)?.ty.initial);
                    }
                }
                Payload::MemorySection(s) => {
                    for m in s {
                        rt.memory = Some(m.map_err(err)?);
                    }
                }
                Payload::GlobalSection(s) => {
                    for g in s {
                        let g = g.map_err(err)?;
                        let r = g.init_expr.get_binary_reader().range();
                        let init = rt.bytes[r.start as usize..r.end as usize].to_vec();
                        rt.globals.push((g.ty, init));
                    }
                }
                Payload::ExportSection(s) => {
                    for e in s {
                        let e = e.map_err(err)?;
                        rt.exports.insert(e.name.into(), (e.kind, e.index));
                    }
                }
                Payload::ElementSection(s) => {
                    for e in s {
                        let e = e.map_err(err)?;
                        let wasmparser::ElementKind::Active {
                            table_index: None | Some(0),
                            offset_expr,
                        } = e.kind
                        else {
                            return Err(err("unsupported element segment"));
                        };
                        let wasmparser::ElementItems::Functions(fs) = e.items else {
                            return Err(err("unsupported element items"));
                        };
                        let fs = fs.into_iter().collect::<Result<Vec<_>, _>>().map_err(err)?;
                        rt.elements.push((const_i32(&offset_expr)?, fs));
                    }
                }
                Payload::DataSection(s) => {
                    for d in s {
                        let d = d.map_err(err)?;
                        let wasmparser::DataKind::Active {
                            memory_index: 0,
                            offset_expr,
                        } = d.kind
                        else {
                            return Err(err("unsupported data segment"));
                        };
                        rt.data.push((const_i32(&offset_expr)?, d.data));
                    }
                }
                Payload::CodeSectionEntry(body) => rt.bodies.push(body),
                Payload::CustomSection(c) => {
                    if let wasmparser::KnownCustom::Name(names) = c.as_known() {
                        for n in names {
                            if let Ok(wasmparser::Name::Function(map)) = n {
                                for naming in map.into_iter().flatten() {
                                    rt.names.insert(naming.index, naming.name.into());
                                }
                            }
                        }
                    }
                }
                Payload::StartSection { .. } => return Err(err("a start function")),
                _ => {}
            }
        }
        Ok(rt)
    }

    /// The index of the runtime's function type `params -> results`.
    pub fn find_type(&self, params: &[ValType], results: &[ValType]) -> Option<u32> {
        self.types
            .iter()
            .position(|t| {
                t.params()
                    .iter()
                    .map(|&v| val(v))
                    .eq(params.iter().copied())
                    && t.results()
                        .iter()
                        .map(|&v| val(v))
                        .eq(results.iter().copied())
            })
            .map(|i| i as u32)
    }
}

fn val(t: wasmparser::ValType) -> ValType {
    match t {
        wasmparser::ValType::I32 => ValType::I32,
        wasmparser::ValType::I64 => ValType::I64,
        wasmparser::ValType::F32 => ValType::F32,
        wasmparser::ValType::F64 => ValType::F64,
        _ => ValType::I32,
    }
}

/// A function the program adds: its type, name and body.
pub struct Added {
    pub name: String,
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
    pub body: Function,
}

/// Index spaces of the linked module, for the code generator.
pub struct Layout {
    /// Final index of runtime export `rt_*` / `run` / … by name.
    pub rt: HashMap<String, u32>,
    /// Index of the first added function.
    pub first_added: u32,
    /// Table index of code 0 (the runtime's own entries come first).
    pub table_base: u32,
    /// Index of the program's first global (after the runtime's).
    pub first_global: u32,
    /// Index of the program's passive data segment.
    pub data_index: u32,
    /// Index of the program's first function import (after the runtime's kept ones).
    pub first_import: u32,
    /// Final index of every runtime function (runtime numbering, imports first).
    map: Vec<u32>,
    resolve_host: bool,
}

impl<'a> Runtime<'a> {
    /// The runtime's function imports from `module`, in order: `(name, params, results)`.
    pub fn imports_of(&self, module: &str) -> Vec<(String, Vec<ValType>, Vec<ValType>)> {
        self.imports
            .iter()
            .filter(|(m, _, _)| m == module)
            .map(|(_, name, t)| {
                let t = &self.types[*t as usize];
                (
                    name.clone(),
                    t.params().iter().map(|&v| val(v)).collect(),
                    t.results().iter().map(|&v| val(v)).collect(),
                )
            })
            .collect()
    }

    /// The index spaces of the linked module. The program adds `new_imports` function imports
    /// after the runtime's kept ones. `sakura_link.dispatch` resolves to the first added
    /// function; with `resolve_host` the `sakura_host` imports resolve to the next added
    /// functions, in import order (an embedding that implements them in the module).
    pub fn layout(&self, resolve_host: bool, new_imports: u32) -> Result<Layout, String> {
        let resolved = |m: &str| m == "sakura_link" || (resolve_host && m == "sakura_host");
        let kept = self.imports.iter().filter(|(m, _, _)| !resolved(m)).count() as u32;
        let first_added = kept + new_imports + self.funcs.len() as u32;
        let (mut next_kept, mut next_host) = (0, 1);
        let mut map = vec![];
        for (m, name, _) in &self.imports {
            map.push(match m.as_str() {
                "sakura_link" if name == "dispatch" => first_added,
                "sakura_link" => return Err(err(format!("unknown link import {name}"))),
                m if resolved(m) => {
                    next_host += 1;
                    first_added + next_host - 1
                }
                _ => {
                    next_kept += 1;
                    next_kept - 1
                }
            });
        }
        for i in 0..self.funcs.len() as u32 {
            map.push(kept + new_imports + i);
        }
        let mut rt = HashMap::new();
        for (name, &(kind, i)) in &self.exports {
            if kind == ExternalKind::Func {
                rt.insert(name.clone(), map[i as usize]);
            }
        }
        Ok(Layout {
            rt,
            first_added,
            first_import: kept,
            table_base: self.table_min as u32,
            first_global: self.globals.len() as u32,
            data_index: self.data.len() as u32,
            map,
            resolve_host,
        })
    }
}

struct Remap<'l> {
    map: &'l [u32],
    /// Index of the first saturating-truncation helper (`SAT_HELPERS` order).
    sat: u32,
}

/// The eight `trunc_sat` conversions in `Remap::body`'s helper order: (float is f64, result is
/// i64, signed).
const SAT_HELPERS: [(bool, bool, bool); 8] = [
    (false, false, true),
    (false, false, false),
    (true, false, true),
    (true, false, false),
    (false, true, true),
    (false, true, false),
    (true, true, true),
    (true, true, false),
];

/// A helper `(f32|f64) -> (i32|i64)` with `trunc_sat` semantics in MVP instructions: NaN is 0,
/// out-of-range values saturate, everything else truncates toward zero.
fn sat_helper(f64_in: bool, i64_out: bool, signed: bool) -> Added {
    use wasm_encoder::{BlockType, Ieee64};
    let mut f = Function::new([(1, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(0);
    if !f64_in {
        s.f64_promote_f32();
    }
    s.local_set(1);
    let ret = if i64_out { ValType::I64 } else { ValType::I32 };
    let k = |s: &mut wasm_encoder::InstructionSink<'_>, v: i64| {
        if i64_out {
            s.i64_const(v);
        } else {
            s.i32_const(v as i32);
        }
    };
    let bits = if i64_out { 64 } else { 32 };
    // The bounds as exact doubles: the first value past the range on each side.
    let (lo, hi, min, max): (f64, f64, i64, i64) = match (signed, i64_out) {
        (true, false) => (
            -2147483649.0,
            2147483648.0,
            i32::MIN as i64,
            i32::MAX as i64,
        ),
        (false, false) => (-1.0, 4294967296.0, 0, u32::MAX as i64),
        (true, true) => (
            -9223372036854777856.0,
            9223372036854775808.0,
            i64::MIN,
            i64::MAX,
        ),
        (false, true) => (-1.0, 18446744073709551616.0, 0, -1),
    };
    let _ = bits;
    s.local_get(1).local_get(1).f64_ne();
    s.if_(BlockType::Empty);
    k(&mut s, 0);
    s.return_().end();
    s.local_get(1).f64_const(Ieee64::new(hi.to_bits())).f64_ge();
    s.if_(BlockType::Empty);
    k(&mut s, max);
    s.return_().end();
    s.local_get(1).f64_const(Ieee64::new(lo.to_bits())).f64_le();
    s.if_(BlockType::Empty);
    k(&mut s, min);
    s.return_().end();
    s.local_get(1);
    match (signed, i64_out) {
        (true, false) => s.i32_trunc_f64_s(),
        (false, false) => s.i32_trunc_f64_u(),
        (true, true) => s.i64_trunc_f64_s(),
        (false, true) => s.i64_trunc_f64_u(),
    };
    s.end();
    Added {
        name: format!(
            "sakura$trunc_sat_{}_{}{}",
            if f64_in { "f64" } else { "f32" },
            if i64_out { "i64" } else { "i32" },
            if signed { "_s" } else { "_u" }
        ),
        params: vec![if f64_in { ValType::F64 } else { ValType::F32 }],
        results: vec![ret],
        body: f,
    }
}

impl Remap<'_> {
    /// Re-encode a runtime function body, lowering what LLVM emits inside libcore functions
    /// (whose bitcode keeps its own target features under LTO) to the ABI §1 profile:
    /// sign extension becomes a shift pair, saturating truncation a call of a helper, and the
    /// padded `call_indirect` table index of the reference-types encoding is written plainly.
    fn body(
        &mut self,
        code: &mut CodeSection,
        body: &wasmparser::FunctionBody<'_>,
    ) -> Result<(), String> {
        use wasmparser::Operator as O;
        let mut f = self.new_function_with_parsed_locals(body).map_err(rerr)?;
        let mut r = body.get_operators_reader().map_err(err)?;
        while !r.eof() {
            let op = r.read().map_err(err)?;
            let mut s = f.instructions();
            let shift32 = |s: &mut wasm_encoder::InstructionSink<'_>, n: i32| {
                s.i32_const(n).i32_shl().i32_const(n).i32_shr_s();
            };
            let shift64 = |s: &mut wasm_encoder::InstructionSink<'_>, n: i64| {
                s.i64_const(n).i64_shl().i64_const(n).i64_shr_s();
            };
            let sat = |s: &mut wasm_encoder::InstructionSink<'_>, k: u32| {
                s.call(self.sat + k);
            };
            match op {
                O::I32Extend8S => shift32(&mut s, 24),
                O::I32Extend16S => shift32(&mut s, 16),
                O::I64Extend8S => shift64(&mut s, 56),
                O::I64Extend16S => shift64(&mut s, 48),
                O::I64Extend32S => shift64(&mut s, 32),
                O::I32TruncSatF32S => sat(&mut s, 0),
                O::I32TruncSatF32U => sat(&mut s, 1),
                O::I32TruncSatF64S => sat(&mut s, 2),
                O::I32TruncSatF64U => sat(&mut s, 3),
                O::I64TruncSatF32S => sat(&mut s, 4),
                O::I64TruncSatF32U => sat(&mut s, 5),
                O::I64TruncSatF64S => sat(&mut s, 6),
                O::I64TruncSatF64U => sat(&mut s, 7),
                op => {
                    let i = self.instruction(op).map_err(rerr)?;
                    f.instruction(&i);
                }
            }
        }
        code.function(&f);
        Ok(())
    }
}

impl Reencode for Remap<'_> {
    type Error = String;

    fn function_index(&mut self, f: u32) -> Result<u32, reencode::Error<String>> {
        self.map
            .get(f as usize)
            .copied()
            .ok_or_else(|| reencode::Error::UserError(err(format!("function {f}"))))
    }
}

/// What the program contributes to the linked module.
pub struct Program {
    /// Function imports after the runtime's: module, name, params, results.
    pub imports: Vec<(String, String, Vec<ValType>, Vec<ValType>)>,
    /// Added functions; the first must be the dispatcher `(code, env, frame) -> step`.
    pub funcs: Vec<Added>,
    /// Funcref table entries after the runtime's: compiled code, then native codes (final
    /// function indices).
    pub table: Vec<u32>,
    /// Globals after the runtime's: type and initial `i32`.
    pub globals: Vec<(ValType, bool, i64)>,
    /// The passive data segment (the static block).
    pub data: Vec<u8>,
    /// Exports: name, kind, final index.
    pub exports: Vec<(String, ExportKind, u32)>,
    pub max_heap_pages: u64,
    pub custom: Vec<(String, Vec<u8>)>,
}

fn rerr(e: reencode::Error<String>) -> String {
    match e {
        reencode::Error::UserError(s) => s,
        e => err(e),
    }
}

/// Assembles the linked module.
pub fn link(rt: &Runtime<'_>, layout: &Layout, p: Program) -> Result<Vec<u8>, String> {
    let helpers: Vec<Added> = SAT_HELPERS
        .iter()
        .map(|&(f, i, s)| sat_helper(f, i, s))
        .collect();
    let mut module = Module::new();
    let mut types = TypeSection::new();
    for t in &rt.types {
        types.ty().function(
            t.params().iter().map(|&v| val(v)),
            t.results().iter().map(|&v| val(v)),
        );
    }
    // One type per added function (deduplicated).
    let mut added_types: Vec<(Vec<ValType>, Vec<ValType>)> = vec![];
    let mut type_of = vec![];
    let mut import_types = vec![];
    let keys = p
        .imports
        .iter()
        .map(|(_, _, ps, rs)| (ps.clone(), rs.clone()))
        .chain(
            p.funcs
                .iter()
                .chain(&helpers)
                .map(|f| (f.params.clone(), f.results.clone())),
        );
    for key in keys {
        let i = match added_types.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                types
                    .ty()
                    .function(key.0.iter().copied(), key.1.iter().copied());
                added_types.push(key);
                added_types.len() - 1
            }
        };
        let t = rt.types.len() as u32 + i as u32;
        if import_types.len() < p.imports.len() {
            import_types.push(t);
        } else {
            type_of.push(t);
        }
    }
    module.section(&types);

    let mut imports = ImportSection::new();
    for (m, name, t) in &rt.imports {
        if m != "sakura_link" && !(layout.resolve_host && m == "sakura_host") {
            imports.import(m, name, EntityType::Function(*t));
        }
    }
    for (k, (m, name, _, _)) in p.imports.iter().enumerate() {
        imports.import(m, name, EntityType::Function(import_types[k]));
    }
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    for &t in &rt.funcs {
        funcs.function(t);
    }
    for &t in &type_of {
        funcs.function(t);
    }
    module.section(&funcs);

    let size = layout.table_base as u64 + p.table.len() as u64;
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        table64: false,
        minimum: size,
        maximum: Some(size),
        shared: false,
    });
    module.section(&tables);

    let mut memories = MemorySection::new();
    let m = rt.memory.ok_or_else(|| err("no memory"))?;
    memories.memory(MemoryType {
        minimum: m.initial,
        maximum: Some(p.max_heap_pages.max(m.initial)),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);

    let mut globals = GlobalSection::new();
    let mut remap = Remap {
        map: &layout.map,
        sat: layout.first_added + p.funcs.len() as u32,
    };
    for (ty, init) in &rt.globals {
        let ty = remap.global_type(*ty).map_err(rerr)?;
        let reader = wasmparser::ConstExpr::new(wasmparser::BinaryReader::new(init, 0));
        let init = remap.const_expr(reader).map_err(rerr)?;
        globals.global(ty, &init);
    }
    for &(ty, mutable, v) in &p.globals {
        let init = match ty {
            ValType::I64 => ConstExpr::i64_const(v),
            _ => ConstExpr::i32_const(v as i32),
        };
        globals.global(
            GlobalType {
                val_type: ty,
                mutable,
                shared: false,
            },
            &init,
        );
    }
    module.section(&globals);

    let mut exports = ExportSection::new();
    for (name, kind, i) in &p.exports {
        exports.export(name, *kind, *i);
    }
    module.section(&exports);

    let mut elems = ElementSection::new();
    for (offset, fs) in &rt.elements {
        let fs: Vec<u32> = fs.iter().map(|&f| layout.map[f as usize]).collect();
        elems.active(
            Some(0),
            &ConstExpr::i32_const(*offset as i32),
            Elements::Functions(fs.into()),
        );
    }
    elems.active(
        Some(0),
        &ConstExpr::i32_const(layout.table_base as i32),
        Elements::Functions(p.table.as_slice().into()),
    );
    module.section(&elems);

    module.section(&DataCountSection {
        count: rt.data.len() as u32 + 1,
    });

    let mut code = CodeSection::new();
    for body in &rt.bodies {
        remap.body(&mut code, body)?;
    }
    for f in p.funcs.iter().chain(&helpers) {
        code.function(&f.body);
    }
    module.section(&code);

    let mut data = DataSection::new();
    for (offset, bytes) in &rt.data {
        data.active(
            0,
            &ConstExpr::i32_const(*offset as i32),
            bytes.iter().copied(),
        );
    }
    data.passive(p.data.iter().copied());
    module.section(&data);

    let mut names = NameSection::new();
    let mut fnames = NameMap::new();
    let mut entries: Vec<(u32, String)> = vec![];
    for (&i, n) in &rt.names {
        if let Some(&f) = layout.map.get(i as usize) {
            entries.push((f, n.clone()));
        }
    }
    for (k, f) in p.funcs.iter().chain(&helpers).enumerate() {
        entries.push((layout.first_added + k as u32, f.name.clone()));
    }
    entries.sort();
    entries.dedup_by_key(|e| e.0);
    for (i, n) in &entries {
        fnames.append(*i, n);
    }
    names.functions(&fnames);
    module.section(&names);
    for (name, bytes) in &p.custom {
        module.section(&wasm_encoder::CustomSection {
            name: name.into(),
            data: bytes.into(),
        });
    }
    Ok(module.finish())
}
