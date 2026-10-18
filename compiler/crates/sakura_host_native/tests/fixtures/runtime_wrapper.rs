//! N0a-only staticlib wrapper: this is NOT the native runtime/profile.
//! N1a owns the real context, native export, std linkage and abort policy in sakura_rt.
extern crate sakura_rt;

// SAFETY: this test-only archive exports this uniquely named C symbol once. The context
// is deliberately unused: no native instance or runtime operation exists in N0a.
#[unsafe(no_mangle)]
pub extern "C" fn sakura_abi_version(_ctx: *mut std::ffi::c_void) -> u32 {
    sakura_rt::ABI_VERSION
}
