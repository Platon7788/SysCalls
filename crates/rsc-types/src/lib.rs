//! # rsc-types
//!
//! Parses the vendored `phnt` headers (user-mode NT API signatures,
//! winsiderss/phnt, MIT) and emits `db/phnt/phnt.toml` — the typed
//! signature overlay used by `rsc-cli merge`.
//!
//! Phase 4 scope:
//! * Parse `NTSYSCALLAPI NTSTATUS NTAPI NtXxx(...)` declarations.
//! * Peel off SAL annotations (`_In_`, `_Out_`, `_Inout_`, `_In_opt_`, ...)
//!   to learn parameter direction and optionality.
//! * Honor `#if (PHNT_VERSION >= PHNT_XXX)` gates as `min_phnt_version` tags.
//! * Skip `#ifdef _KERNEL_MODE` blocks.
//! * Normalize phnt's native types (`PHANDLE`, `PVOID`, ...) into canonical
//!   RSC forms consumed by `rsc_syscall!` / `canonical.toml`.

pub mod emit;
pub mod normalizer;
pub mod parser;
