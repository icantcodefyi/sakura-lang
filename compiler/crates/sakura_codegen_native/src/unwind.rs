//! `.eh_frame` for the compiled functions (plan M5 N2d): the call-frame information Cranelift
//! records, so debuggers and profilers unwind native Sakura frames, twins included. Nothing
//! unwinds through generated code at run time (the driver is `panic = "abort"`, ABI §1b), so
//! there is no personality routine or LSDA. On Mach-O the linker derives compact unwind from it.
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
use object::write::StandardSection;

use crate::debuginfo::Relocating;

/// The frame table being built: one CIE, then an FDE per function, its address a symbol index
/// into `funcs`.
pub struct Unwind {
    table: FrameTable,
    cie: Option<gimli::write::CieId>,
    funcs: Vec<FuncId>,
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
        }
    }

    /// Record the unwind information of `id`, just compiled into `ctx`.
    pub fn add(
        &mut self,
        isa: &dyn TargetIsa,
        ctx: &cranelift_codegen::Context,
        id: FuncId,
    ) -> Result<(), String> {
        let Some(cie) = self.cie else {
            return Ok(());
        };
        let code = ctx.compiled_code().ok_or("no compiled code to unwind")?;
        if let Some(UnwindInfo::SystemV(info)) =
            code.create_unwind_info(isa).map_err(|e| e.to_string())?
        {
            let symbol = self.funcs.len();
            self.funcs.push(id);
            self.table
                .add_fde(cie, info.to_fde(Address::Symbol { symbol, addend: 0 }));
        }
        Ok(())
    }

    /// Add the `.eh_frame` section (`__TEXT,__eh_frame` on Mach-O) to `product`.
    pub fn write(self, product: &mut ObjectProduct) -> Result<(), String> {
        if self.funcs.is_empty() {
            return Ok(());
        }
        let mut eh = EhFrame(Relocating::new());
        self.table
            .write_eh_frame(&mut eh)
            .map_err(|e| e.to_string())?;
        let section = product.object.section_id(StandardSection::EhFrame);
        product
            .object
            .set_section_data(section, eh.0.data.slice().to_vec(), 8);
        eh.0.relocate(product, section, &self.funcs, &HashMap::new(), false)
    }
}
