//! DWARF for the compiled functions (plan M5 N2d): one compile unit with a subprogram per
//! function and a line table from each instruction's Sakura source position (`Func::locs`), so
//! debuggers and profilers show `file:line` for Sakura frames. On Mach-O the sections stay in
//! the object, where the linker's debug map points debuggers (`dsymutil` collects them); on
//! ELF they are linked into the executable.
//!
//! Written with `cranelift_codegen::gimli`, as the `.eh_frame` is ([`crate::unwind`]).

use std::collections::HashMap;

use cranelift_codegen::gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, LineProgram, LineString, Range, RangeList,
    Sections, Writer,
};
use cranelift_codegen::gimli::{self, Encoding, Format, LineEncoding, RunTimeEndian, SectionId};
use cranelift_codegen::ir::SourceLoc;
use cranelift_module::FuncId;
use cranelift_object::ObjectProduct;
use object::write::Relocation;
use object::{BinaryFormat, RelocationEncoding, RelocationFlags, RelocationKind, SectionKind};
use sakura_lir::lir::Loc;

/// A compiled function, its code size and where each run of its code comes from: `(code
/// offset, SourceLoc bits)`.
type Compiled = (FuncId, u64, Vec<(u32, u32)>);

/// The source positions instructions name ([`SourceLoc`] `i` is `locs[i]`) and each compiled
/// function's code ranges by position.
#[derive(Default)]
pub struct Lines {
    locs: Vec<Loc>,
    ids: HashMap<(u32, u32), u32>,
    funcs: Vec<Compiled>,
}

impl Lines {
    /// The [`SourceLoc`] of `loc`.
    pub fn srcloc(&mut self, loc: Loc) -> SourceLoc {
        let next = self.locs.len() as u32;
        let id = *self.ids.entry((loc.file, loc.offset)).or_insert_with(|| {
            self.locs.push(loc);
            next
        });
        SourceLoc::new(id)
    }

    /// Record where `id`'s code, just compiled into `ctx`, comes from.
    pub fn add(&mut self, ctx: &cranelift_codegen::Context, id: FuncId) -> Result<(), String> {
        let code = ctx.compiled_code().ok_or("no compiled code")?;
        let rows = code
            .buffer
            .get_srclocs_sorted()
            .iter()
            .filter(|s| !s.loc.is_default())
            .map(|s| (s.start, s.loc.bits()))
            .collect();
        self.funcs.push((id, code.code_buffer().len() as u64, rows));
        Ok(())
    }

    /// Add the DWARF sections to `product`. `sources` and `texts` are each source's URI and
    /// text by `Pos::file`; positions in a source without its text get no row.
    pub fn write(
        self,
        product: &mut ObjectProduct,
        sources: &[String],
        texts: &[String],
    ) -> Result<(), String> {
        if self.funcs.iter().all(|(_, _, rows)| rows.is_empty()) {
            return Ok(());
        }
        let encoding = Encoding {
            format: Format::Dwarf32,
            version: 4,
            address_size: 8,
        };
        let mut dwarf = DwarfUnit::new(encoding);
        // The unit is named for the program's first file of its own (std sources are
        // `sakura:///` URIs), in its directory.
        let main = sources
            .iter()
            .find(|u| u.starts_with("file://"))
            .or(sources.first())
            .map_or("main.sak".into(), |u| path_of(u));
        let (dir, name) = match main.rsplit_once('/') {
            Some((dir, name)) if !dir.is_empty() => (dir.to_string(), name.to_string()),
            _ => (".".to_string(), main.clone()),
        };
        dwarf.unit.line_program = LineProgram::new(
            encoding,
            LineEncoding::default(),
            LineString::String(dir.clone().into_bytes()),
            None,
            LineString::String(name.clone().into_bytes()),
            None,
        );
        let starts: Vec<Option<Vec<u32>>> = texts
            .iter()
            .map(|t| (!t.is_empty()).then(|| line_starts(t)))
            .collect();
        let mut files = HashMap::new();
        let mut ranges = vec![];
        let root = dwarf.unit.root();
        for (i, (_, len, rows)) in self.funcs.iter().enumerate() {
            let begin = Address::Symbol {
                symbol: i,
                addend: 0,
            };
            ranges.push(Range::StartLength {
                begin,
                length: *len,
            });
            let program = &mut dwarf.unit.line_program;
            program.begin_sequence(Some(begin));
            for &(start, loc) in rows {
                let Loc { file, offset } = self.locs[loc as usize];
                let Some(lines) = starts.get(file as usize).and_then(Option::as_ref) else {
                    continue;
                };
                let line = lines.partition_point(|&s| s <= offset);
                let column = offset - lines[line - 1] + 1;
                let dir = program.default_directory();
                let file_id = *files.entry(file).or_insert_with(|| {
                    let name = sources
                        .get(file as usize)
                        .map_or(format!("source{file}"), |u| path_of(u));
                    program.add_file(LineString::String(name.into_bytes()), dir, None)
                });
                let row = program.row();
                row.address_offset = u64::from(start);
                row.file = file_id;
                row.line = line as u64;
                row.column = u64::from(column);
                program.generate_row();
            }
            program.end_sequence(*len);
        }
        let ranges = dwarf.unit.ranges.add(RangeList(ranges));
        let cu = dwarf.unit.get_mut(root);
        cu.set(
            gimli::DW_AT_producer,
            AttributeValue::String(b"sakura (Cranelift)".to_vec()),
        );
        cu.set(gimli::DW_AT_name, AttributeValue::String(name.into_bytes()));
        cu.set(
            gimli::DW_AT_comp_dir,
            AttributeValue::String(dir.into_bytes()),
        );
        cu.set(
            gimli::DW_AT_low_pc,
            AttributeValue::Address(Address::Constant(0)),
        );
        cu.set(gimli::DW_AT_ranges, AttributeValue::RangeListRef(ranges));
        let macho = product.object.format() == BinaryFormat::MachO;
        for (i, (id, len, _)) in self.funcs.iter().enumerate() {
            let mut name = product
                .object
                .symbol(product.function_symbol(*id))
                .name
                .clone();
            // Mach-O symbols carry a leading `_` the source name does not.
            if macho && name.first() == Some(&b'_') {
                name.remove(0);
            }
            let sub = dwarf.unit.add(root, gimli::DW_TAG_subprogram);
            let sub = dwarf.unit.get_mut(sub);
            sub.set(gimli::DW_AT_name, AttributeValue::String(name));
            sub.set(
                gimli::DW_AT_low_pc,
                AttributeValue::Address(Address::Symbol {
                    symbol: i,
                    addend: 0,
                }),
            );
            sub.set(gimli::DW_AT_high_pc, AttributeValue::Udata(*len));
        }
        let mut sections = Sections::new(Relocating::new());
        dwarf.write(&mut sections).map_err(|e| e.to_string())?;
        let funcs: Vec<FuncId> = self.funcs.iter().map(|(id, _, _)| *id).collect();
        let mut ids = HashMap::new();
        if macho {
            sections.for_each_mut(|_, w| w.resolve(product, &funcs))?;
        }
        sections.for_each(|id, w| -> Result<(), String> {
            if w.data.len() > 0 {
                let (segment, name) = if macho {
                    (b"__DWARF".to_vec(), id.name().replacen('.', "__", 1))
                } else {
                    (vec![], id.name().to_string())
                };
                let section =
                    product
                        .object
                        .add_section(segment, name.into_bytes(), SectionKind::Debug);
                product
                    .object
                    .set_section_data(section, w.data.slice().to_vec(), 1);
                ids.insert(id, section);
            }
            Ok(())
        })?;
        sections.for_each(|id, w| -> Result<(), String> {
            match ids.get(&id) {
                Some(&section) => w.relocate(product, section, &funcs, &ids, macho),
                None => Ok(()),
            }
        })
    }
}

/// A source's path for the line table: a `file://` URI's path, else the URI (`sakura:///std/…`).
fn path_of(uri: &str) -> String {
    let Some(path) = uri.strip_prefix("file://") else {
        return uri.to_string();
    };
    // Percent-decoding of the bytes `file://` URIs escape.
    let mut out = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(b) = bytes.next() {
        let hex = |c: Option<u8>| c.and_then(|c| (c as char).to_digit(16));
        if b == b'%' {
            let mut ahead = bytes.clone();
            if let (Some(h), Some(l)) = (hex(ahead.next()), hex(ahead.next())) {
                out.push((h * 16 + l) as u8);
                bytes = ahead;
                continue;
            }
        }
        out.push(b);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The byte offset of each line's start.
fn line_starts(text: &str) -> Vec<u32> {
    std::iter::once(0)
        .chain(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i as u32 + 1),
        )
        .collect()
}

/// What a relocation names: a compiled function (by index) or the start of a section.
#[derive(Clone, Copy)]
enum Target {
    Func(usize),
    Section(SectionId),
}

/// A section's bytes with the relocations the linker resolves: `(offset, target, addend, kind,
/// size in bits)`.
#[derive(Clone)]
pub struct Relocating {
    pub data: EndianVec<RunTimeEndian>,
    relocs: Vec<(u64, Target, i64, RelocationKind, u8)>,
}

impl Relocating {
    pub fn new() -> Relocating {
        Relocating {
            data: EndianVec::new(RunTimeEndian::Little),
            relocs: vec![],
        }
    }

    /// Write each function address as the object file has it and drop its relocation. Debuggers
    /// read Mach-O DWARF from the object, through the linker's debug map, without applying its
    /// relocations. The address is the symbol's value: `__text` is the object's first section, at
    /// 0, as `emit` defines the functions before any data (checked here).
    fn resolve(&mut self, product: &mut ObjectProduct, funcs: &[FuncId]) -> Result<(), String> {
        let text = product
            .object
            .section_id(object::write::StandardSection::Text);
        if format!("{text:?}") != "SectionId(0)" {
            return Err("`__text` is not the object's first section".into());
        }
        let mut relocs = std::mem::take(&mut self.relocs);
        for &(offset, target, addend, _, size) in &relocs {
            if let Target::Func(i) = target {
                let value = product
                    .object
                    .symbol(product.function_symbol(funcs[i]))
                    .value;
                self.data
                    .write_udata_at(offset as usize, value.wrapping_add_signed(addend), size / 8)
                    .map_err(|e| e.to_string())?;
            }
        }
        relocs.retain(|r| !matches!(r.1, Target::Func(_)));
        self.relocs = relocs;
        Ok(())
    }

    /// Add the relocations to `section` of `product`; `funcs` maps function indices to their
    /// [`FuncId`]s and `sections` the DWARF sections to theirs.
    pub fn relocate(
        &self,
        product: &mut ObjectProduct,
        section: object::write::SectionId,
        funcs: &[FuncId],
        sections: &HashMap<SectionId, object::write::SectionId>,
        macho: bool,
    ) -> Result<(), String> {
        for &(offset, target, addend, kind, size) in &self.relocs {
            let symbol = match target {
                Target::Func(i) => product.function_symbol(funcs[i]),
                // Mach-O debug sections are not merged by the linker: offsets are final.
                Target::Section(_) if macho => continue,
                Target::Section(s) => {
                    let s = *sections.get(&s).ok_or("a reference to an empty section")?;
                    product.object.section_symbol(s)
                }
            };
            product
                .object
                .add_relocation(
                    section,
                    Relocation {
                        offset,
                        symbol,
                        addend,
                        flags: RelocationFlags::Generic {
                            kind,
                            encoding: RelocationEncoding::Generic,
                            size,
                        },
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Writer for Relocating {
    type Endian = RunTimeEndian;

    fn endian(&self) -> RunTimeEndian {
        self.data.endian()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn write(&mut self, bytes: &[u8]) -> gimli::write::Result<()> {
        self.data.write(bytes)
    }

    fn write_at(&mut self, offset: usize, bytes: &[u8]) -> gimli::write::Result<()> {
        self.data.write_at(offset, bytes)
    }

    fn write_address(&mut self, address: Address, size: u8) -> gimli::write::Result<()> {
        match address {
            Address::Symbol { symbol, addend } => {
                self.relocs.push((
                    self.len() as u64,
                    Target::Func(symbol),
                    addend,
                    RelocationKind::Absolute,
                    size * 8,
                ));
                self.write_udata(0, size)
            }
            Address::Constant(_) => self.data.write_address(address, size),
        }
    }

    fn write_offset(
        &mut self,
        val: usize,
        section: SectionId,
        size: u8,
    ) -> gimli::write::Result<()> {
        self.relocs.push((
            self.len() as u64,
            Target::Section(section),
            val as i64,
            RelocationKind::Absolute,
            size * 8,
        ));
        self.write_udata(val as u64, size)
    }

    fn write_offset_at(
        &mut self,
        offset: usize,
        val: usize,
        section: SectionId,
        size: u8,
    ) -> gimli::write::Result<()> {
        self.relocs.push((
            offset as u64,
            Target::Section(section),
            val as i64,
            RelocationKind::Absolute,
            size * 8,
        ));
        self.write_udata_at(offset, val as u64, size)
    }

    fn write_eh_pointer(
        &mut self,
        address: Address,
        eh_pe: gimli::DwEhPe,
        size: u8,
    ) -> gimli::write::Result<()> {
        match address {
            Address::Symbol { symbol, addend }
                if eh_pe == gimli::DwEhPe(gimli::DW_EH_PE_pcrel.0 | gimli::DW_EH_PE_sdata4.0) =>
            {
                self.relocs.push((
                    self.len() as u64,
                    Target::Func(symbol),
                    addend,
                    RelocationKind::Relative,
                    32,
                ));
                self.write_udata(0, 4)
            }
            Address::Symbol { .. } => Err(gimli::write::Error::UnsupportedPointerEncoding(eh_pe)),
            Address::Constant(_) => self.data.write_eh_pointer(address, eh_pe, size),
        }
    }
}
