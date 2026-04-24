//! # rsc-codegen
//!
//! Proc-macro crate for the `rsc_syscall!` macro. Expands a single NT-function
//! signature into a naked syscall stub that:
//!
//! 1. Calls `rsc_runtime::__rsc_resolve(hash)` once to obtain the SSN and a
//!    `syscall; ret` slide inside ntdll (resolver is itself lazy-populated).
//! 2. Executes the `syscall` from inside ntdll — the JUMPER pattern (see
//!    `DECISIONS.md` §D-07) — so stack-trace-based detection sees a
//!    legitimate caller.
//!
//! The naked-stub templates for both architectures live directly in
//! `expand_stub` below, gated by `#[cfg(target_arch = …)]`.
//! See §S-01 and §S-08 for ABI background.
//!
//! # Input grammar
//!
//! ```ignore
//! rsc_syscall! {
//!     fn NtAllocateVirtualMemory(
//!         process_handle: HANDLE,
//!         base_address: *mut PVOID,
//!         zero_bits: ULONG_PTR,
//!         region_size: *mut SIZE_T,
//!         allocation_type: ULONG,
//!         protect: ULONG,
//!     ) -> NTSTATUS;
//! }
//! ```
//!
//! # Requirements on the caller crate
//!
//! The invoking crate must have `rsc-runtime` in scope as `::rsc_runtime`
//! (or renamed via `extern crate rsc_runtime as _;` — not tested). The
//! generated code references `::rsc_runtime::rsc_hash` and
//! `::rsc_runtime::__rsc_resolve` by absolute path.

use proc_macro::TokenStream;
use proc_macro_error2::{abort, proc_macro_error};
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, ForeignItemFn};

/// Expands a function signature into an x64 naked syscall stub.
///
/// On x86 targets, emits a placeholder that returns `STATUS_NOT_IMPLEMENTED`
/// (Phase 2a — x86 / WoW64 stubs land in Phase 2b).
///
/// # Example
///
/// ```ignore
/// use rsc_codegen::rsc_syscall;
/// use rsc_runtime::{HANDLE, NTSTATUS};
///
/// rsc_syscall! {
///     fn NtClose(handle: HANDLE) -> NTSTATUS;
/// }
/// ```
#[proc_macro]
#[proc_macro_error]
pub fn rsc_syscall(input: TokenStream) -> TokenStream {
    let f = parse_macro_input!(input as ForeignItemFn);
    expand_stub(f).into()
}

fn expand_stub(f: ForeignItemFn) -> proc_macro2::TokenStream {
    let sig = &f.sig;
    let fn_name = &sig.ident;

    // Reject anything that isn't a simple NT-style signature.
    reject_unsupported(sig);

    // Collect bindings + types for the arg list we emit in the function
    // signature. We ignore the parameter names for hashing — only the
    // function name matters, and that's guaranteed to be the ident form.
    let args = sig
        .inputs
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let syn::FnArg::Typed(pt) = arg else {
                abort!(arg, "rsc_syscall! does not accept `self` parameters");
            };
            let ident = syn::Ident::new(&format!("_arg{}", i + 1), pt.pat.span());
            let ty = &pt.ty;
            quote! { #ident: #ty }
        })
        .collect::<Vec<_>>();

    let ret = &sig.output;

    // Byte-literal of the function name for compile-time hashing.
    let name_bytes = syn::LitByteStr::new(
        fn_name.to_string().as_bytes(),
        proc_macro2::Span::call_site(),
    );

    // Stdcall stack cleanup amount for x86 = 4 bytes per parameter slot.
    // (x64 doesn't need this — args are in regs / caller cleans.)
    let stdcall_cleanup: u32 = (sig.inputs.len() as u32).saturating_mul(4);

    quote! {
        // ---- x64 naked stub ---------------------------------------------
        #[cfg(target_arch = "x86_64")]
        #[unsafe(naked)]
        #[allow(non_snake_case)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "system" fn #fn_name ( #(#args),* ) #ret {
            // Stack at entry: rsp ≡ 8 (mod 16) per MS x64 ABI after the
            // caller's `call`. We push four arg-regs, then 0x28 to land
            // at rsp ≡ 0 before our own `call` (inside callee, ≡ 8 as
            // required). Two `call`s share the same shadow space —
            // resolver is a plain Rust fn, it'll spill only into the
            // 0x20 home slots below [rsp+0x28], never touching our
            // saved args.
            //
            // Slide comes from `__rsc_random_slide` — a different slide
            // is chosen per call so the return-to-ntdll RIP never settles
            // on a pattern; SSN still comes from `__rsc_resolve_ssn` which
            // needs the name hash.
            ::core::arch::naked_asm!(
                "push rcx",
                "push rdx",
                "push r8",
                "push r9",
                "sub rsp, 0x28",
                // Phase 1: random `syscall; ret` slide, stash in R11.
                "call {random_slide}",
                "mov r11, rax",
                // Phase 2: per-function SSN.
                "mov ecx, {hash}",
                "call {resolve_ssn}",
                // Restore original arg regs.
                "add rsp, 0x28",
                "pop r9",
                "pop r8",
                "pop rdx",
                "pop rcx",
                // Syscall ABI finalization.
                "mov r10, rcx",     // param1 is read from R10 by the kernel
                "jmp r11",          // jmp into `syscall; ret` inside ntdll
                hash = const ::rsc_runtime::rsc_hash(#name_bytes),
                random_slide = sym ::rsc_runtime::__rsc_random_slide,
                resolve_ssn = sym ::rsc_runtime::__rsc_resolve_ssn,
            )
        }

        // ---- x86 naked stub (WoW64 / native) ----------------------------
        //
        // x86 ABI is stdcall: args right-to-left on stack, callee cleans.
        // Entry layout: `[caller_ret, arg1, arg2, …, argN]`.
        //
        // WoW64 case (only supported path on modern Windows, where every
        // 32-bit process is WoW64): `fs:[0xC0]` = `Wow64SystemServiceCall`.
        // We resolve the SSN, put it in EAX, and `call ecx` — the gate
        // sees exactly the stack layout ntdll stubs produce when they do
        // `call edx` themselves, because our own `call ecx` push plus the
        // existing caller-ret slot mirror the two-level ret stack ntdll
        // expects. Gate returns, we `ret N` to clean stdcall args.
        //
        // Native x86 (non-WoW64, Win7 pre-WoW64 era — essentially extinct
        // on our baseline Win10 1809+) returns STATUS_NOT_IMPLEMENTED.
        #[cfg(target_arch = "x86")]
        #[unsafe(naked)]
        #[allow(non_snake_case)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "system" fn #fn_name ( #(#args),* ) #ret {
            ::core::arch::naked_asm!(
                // Resolve SSN (stdcall: single 4-byte arg, cleans up itself)
                "push {hash}",
                "call {resolve_ssn}",
                // EAX = SSN
                "mov ecx, fs:[0xC0]",
                "test ecx, ecx",
                "jnz 2f",
                // Native x86 path — unimplemented on our baseline.
                "mov eax, 0xC0000002",   // STATUS_NOT_IMPLEMENTED
                "ret {cleanup}",
                "2:",
                // WoW64 path: EAX=SSN, ECX=gate, args already on stack.
                "call ecx",
                "ret {cleanup}",
                hash = const ::rsc_runtime::rsc_hash(#name_bytes),
                resolve_ssn = sym ::rsc_runtime::__rsc_resolve_ssn,
                cleanup = const #stdcall_cleanup,
            )
        }
    }
}

/// Rejects features we don't (yet) support, with a span-accurate error.
fn reject_unsupported(sig: &syn::Signature) {
    if sig.asyncness.is_some() {
        abort!(sig.asyncness, "rsc_syscall! cannot be async");
    }
    if sig.unsafety.is_some() {
        abort!(
            sig.unsafety,
            "drop the `unsafe` keyword; rsc_syscall! emits an `unsafe extern \"system\"` fn already"
        );
    }
    if sig.abi.is_some() {
        abort!(
            sig.abi,
            "drop the explicit ABI; rsc_syscall! always uses `extern \"system\"`"
        );
    }
    if !sig.generics.params.is_empty() {
        abort!(
            sig.generics,
            "rsc_syscall! does not support generic syscall stubs"
        );
    }
    if sig.variadic.is_some() {
        abort!(
            sig.variadic,
            "variadic NT syscalls are not a thing — remove `...`"
        );
    }
    if sig.inputs.len() > 32 {
        abort!(
            sig.inputs,
            "NT syscalls with more than 32 parameters are not supported"
        );
    }
}
