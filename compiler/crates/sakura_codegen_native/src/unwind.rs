//! `.eh_frame` for the compiled functions (plan M5 N2d): the call-frame information Cranelift
//! records, so debuggers and profilers unwind native Sakura frames, twins included. Nothing
//! unwinds through generated code at run time (the driver is `panic = "abort"`, ABI §1b), so
//! there is no personality routine or LSDA. On Mach-O the linker derives compact unwind from it.
//! On Windows x64 the same records are `.pdata` and `.xdata` (plan m5-native.md row C), which
//! the system unwinder needs to walk past any frame that is not a leaf.
//!
//! The section is written with `cranelift_codegen::gimli`, the `gimli` Cranelift's unwind
//! records are made of.

use std::collections::HashMap;

use cranelift_codegen::gimli;
use cranelift_codegen::gimli::write::{Address, EhFrame, FrameTable};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::isa::unwind::UnwindInfo;
use cranelift_module::FuncId;
use cranelift_object::ObjectProduct;
use object::write::{Relocation, StandardSection};
use object::{RelocationFlags, SectionKind};

use crate::debuginfo::Relocating;

/// The frame table being built: one CIE, then an FDE per function, its address a symbol index
/// into `funcs`.
pub struct Unwind {
    table: FrameTable,
    cie: Option<gimli::write::CieId>,
    funcs: Vec<FuncId>,
    /// Windows x64: per function, its code size and its `UNWIND_INFO` bytes.
    windows: Vec<(FuncId, u32, Vec<u8>)>,
}

impl Unwind {
    pub fn new(isa: &dyn TargetIsa) -> Unwind {
        let mut table = FrameTable::default();
        let cie = isa.create_systemv_cie().map(|mut cie| {
            // Position-independent: each FDE names its function relative to itself.
            cie.fde_address_encoding =
                gimli::DwEhPe(gimli::DW_EH_PE_pcrel.0 | gimli::DW_EH_PE_sdata4.0);
            table.add_cie(cie)
        });
        Unwind {
            table,
            cie,
            funcs: vec![],
            windows: vec![],
        }
    }

    /// Record the unwind information of `id`, just compiled into `ctx`.
    pub fn add(
        &mut self,
        isa: &dyn TargetIsa,
        ctx: &cranelift_codegen::Context,
        id: FuncId,
    ) -> Result<(), String> {
        let code = ctx.compiled_code().ok_or("no compiled code to unwind")?;
        match code.create_unwind_info(isa).map_err(|e| e.to_string())? {
            Some(UnwindInfo::SystemV(info)) => {
                let Some(cie) = self.cie else {
                    return Ok(());
                };
                let symbol = self.funcs.len();
                self.funcs.push(id);
                self.table
                    .add_fde(cie, info.to_fde(Address::Symbol { symbol, addend: 0 }));
            }
            Some(UnwindInfo::WindowsX64(info)) => {
                let mut bytes = vec![0; info.emit_size()];
                info.emit(&mut bytes);
                self.windows
                    .push((id, code.code_buffer().len() as u32, bytes));
            }
            _ => {}
        }
        Ok(())
    }

    /// Add the `.eh_frame` section (`__TEXT,__eh_frame` on Mach-O), or on Windows `.pdata` and
    /// `.xdata`, to `product`. On x86_64 Mach-O it returns the FDE fields [`patch`] fills in.
    pub fn write(self, product: &mut ObjectProduct) -> Result<Vec<Pending>, String> {
        if !self.windows.is_empty() {
            return write_windows(&self.windows, product).map(|()| vec![]);
        }
        if self.funcs.is_empty() {
            return Ok(vec![]);
        }
        let mut eh = EhFrame(Relocating::new());
        self.table
            .write_eh_frame(&mut eh)
            .map_err(|e| e.to_string())?;
        let section = product.object.section_id(StandardSection::EhFrame);
        product
            .object
            .set_section_data(section, eh.0.data.slice().to_vec(), 8);
        // ld64 takes no pc-relative (`X86_64_RELOC_SIGNED`) relocation in `__eh_frame`, and
        // `object` writes x86_64's that way (arm64's are a subtractor pair it accepts).
        let pending = if product.object.format() == object::BinaryFormat::MachO
            && product.object.architecture() == object::Architecture::X86_64
        {
            eh.0.take_func_relocs()
                .into_iter()
                .map(|(offset, f, addend)| {
                    let name = product
                        .object
                        .symbol(product.function_symbol(self.funcs[f]))
                        .name
                        .clone();
                    (offset, name, addend)
                })
                .collect()
        } else {
            vec![]
        };
        eh.0.relocate(product, section, &self.funcs, &HashMap::new(), false)?;
        Ok(pending)
    }
}

/// An `.eh_frame` field left unrelocated: its offset, the function symbol it names and the
/// addend, a 32-bit pc-relative value.
pub type Pending = (u64, Vec<u8>, i64);

/// Write each pending field of the emitted Mach-O `object` as the function's address plus the
/// addend minus the field's own, both in the object's address space: ld64 reads an FDE's
/// function from that value when the field has no relocation, as for an assembler's local
/// reference.
pub fn patch(mut object: Vec<u8>, pending: &[Pending]) -> Result<Vec<u8>, String> {
    use object::{Object, ObjectSection, ObjectSymbol};
    if pending.is_empty() {
        return Ok(object);
    }
    let file = object::File::parse(object.as_slice()).map_err(|e| e.to_string())?;
    let eh = file
        .section_by_name("__eh_frame")
        .ok_or("no __eh_frame to patch")?;
    let (at, base) = (
        eh.file_range().ok_or("__eh_frame has no data")?.0,
        eh.address(),
    );
    let addresses: HashMap<&[u8], u64> = file
        .symbols()
        .filter_map(|s| Some((s.name_bytes().ok()?, s.address())))
        .collect();
    let mut writes = vec![];
    for (offset, name, addend) in pending {
        let target = addresses
            .get(name.as_slice())
            .ok_or("an FDE names a missing symbol")?;
        let value = (*target as i64 + addend - (base + offset) as i64) as i32;
        writes.push((at + offset, value));
    }
    drop(file);
    for (at, value) in writes {
        let at = at as usize;
        object[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(object)
}

/// `.xdata`, each function's `UNWIND_INFO` at a 4-byte boundary, and `.pdata`, a
/// `RUNTIME_FUNCTION` per function (its start, its end and its `UNWIND_INFO`, each an
/// image-relative address `link.exe` fills in).
fn write_windows(
    funcs: &[(FuncId, u32, Vec<u8>)],
    product: &mut ObjectProduct,
) -> Result<(), String> {
    let (mut xdata, mut pdata, mut relocs) = (vec![], vec![], vec![]);
    for &(id, len, ref info) in funcs {
        xdata.resize(xdata.len().next_multiple_of(4), 0);
        let at = xdata.len() as i64;
        xdata.extend_from_slice(info);
        for (symbol, addend) in [(Some(id), 0), (Some(id), i64::from(len)), (None, at)] {
            relocs.push((pdata.len() as u64, symbol, addend));
            pdata.extend_from_slice(&[0; 4]);
        }
    }
    let object = &mut product.object;
    let x = object.add_section(vec![], b".xdata".to_vec(), SectionKind::ReadOnlyData);
    object.set_section_data(x, xdata, 4);
    let p = object.add_section(vec![], b".pdata".to_vec(), SectionKind::ReadOnlyData);
    object.set_section_data(p, pdata, 4);
    let xdata_symbol = product.object.section_symbol(x);
    for (offset, func, addend) in relocs {
        let symbol = func.map_or(xdata_symbol, |id| product.function_symbol(id));
        product
            .object
            .add_relocation(
                p,
                Relocation {
                    offset,
                    symbol,
                    addend,
                    flags: RelocationFlags::Coff {
                        typ: object::pe::IMAGE_REL_AMD64_ADDR32NB,
                    },
                },
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
