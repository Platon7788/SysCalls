//! Recovers the System Service Number (SSN) and, where possible, the
//! parameter count (arity) of a syscall stub from its raw bytes in
//! `ntdll.dll`.
//!
//! # x64 Zw* stub (typical, unhooked)
//!
//! ```text
//! 4C 8B D1                mov r10, rcx
//! B8 xx xx xx xx          mov eax, <SSN>      <-- 4 bytes at [4..8]
//! F6 04 25 xx xx xx xx 01 test byte [flag], 1
//! 74 03                   je  skip
//! 0F 05                   syscall
//! C3                      ret
//! skip:
//! CD 2E                   int 2Eh             (legacy fallback)
//! C3                      ret
//! ```
//!
//! Arity cannot be read from the byte stream on x64 — the stub doesn't
//! encode it. We leave `arity_x64 = None` and rely on the phnt overlay
//! (Phase 4) to fill types during merge.
//!
//! # x86 Zw* stub (WoW64)
//!
//! ```text
//! B8 xx xx xx xx          mov eax, <SSN>
//! BA yy yy yy yy          mov edx, Wow64SystemServiceCall_addr
//! FF D2                   call edx
//! C2 NN 00                ret <NN>             <-- NN = arity*4
//! ```
//!
//! We can read both SSN (from `mov eax`) and arity (from `ret NN / 4`).

use tracing::{trace, warn};

pub struct StubInfo {
    pub ssn: Option<u32>,
    pub arity: Option<u32>,
    /// Hex of the first ≤ 20 stub bytes, for debugging / drift detection.
    pub bytes_hex: String,
}

const STUB_SNAPSHOT_LEN: usize = 20;

/// Disassembles an x64 `Zw*` stub starting at `bytes[0]`.
pub fn disasm_x64(bytes: &[u8]) -> StubInfo {
    let snap = &bytes[..bytes.len().min(STUB_SNAPSHOT_LEN)];
    let bytes_hex = hex_upper(snap);

    // Classic prologue check. Anything else means the stub is hooked; we
    // still try to recover SSN by looking for `B8 xx xx xx xx` anywhere in
    // the first 16 bytes.
    let ssn = if bytes.len() >= 8 && bytes[..4] == [0x4C, 0x8B, 0xD1, 0xB8] {
        Some(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
    } else {
        search_ssn_by_mov_eax(bytes)
    };

    match ssn {
        None => warn!(bytes = %bytes_hex, "x64 stub prologue unrecognized"),
        Some(n) => trace!(ssn = format!("{n:#x}"), bytes = %bytes_hex, "x64 stub"),
    }

    StubInfo { ssn, arity: None, bytes_hex }
}

/// Disassembles an x86 `Zw*` stub (WoW64 layout).
pub fn disasm_x86(bytes: &[u8]) -> StubInfo {
    let snap = &bytes[..bytes.len().min(STUB_SNAPSHOT_LEN)];
    let bytes_hex = hex_upper(snap);

    // SSN: first five bytes are `B8 xx xx xx xx` in the canonical layout.
    let ssn = if bytes.len() >= 5 && bytes[0] == 0xB8 {
        Some(u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]))
    } else {
        search_ssn_by_mov_eax(bytes)
    };

    // Arity: scan forward for `C2 NN 00` (near ret with 16-bit immediate).
    // Must skip over `BA yy yy yy yy FF D2` middle section.
    let mut arity = None;
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == 0xC2 && bytes[i + 2] == 0x00 {
            let nn = bytes[i + 1];
            if nn.is_multiple_of(4) && nn <= 60 {
                arity = Some(u32::from(nn) / 4);
                break;
            }
        }
    }

    match ssn {
        None => warn!(bytes = %bytes_hex, "x86 stub SSN not recoverable"),
        Some(n) => trace!(
            ssn = format!("{n:#x}"),
            arity = ?arity,
            bytes = %bytes_hex,
            "x86 stub"
        ),
    }

    StubInfo { ssn, arity, bytes_hex }
}

/// Fallback: look for a `mov eax, imm32` (`B8 xx xx xx xx`) somewhere
/// in the first 16 bytes. Useful if the stub prologue has been hot-patched
/// but the `mov eax` itself is intact.
fn search_ssn_by_mov_eax(bytes: &[u8]) -> Option<u32> {
    let end = bytes.len().min(16);
    for i in 0..end.saturating_sub(5) {
        if bytes[i] == 0xB8 {
            return Some(u32::from_le_bytes([
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
            ]));
        }
    }
    None
}

fn hex_upper(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x64_canonical_stub() {
        // mov r10, rcx; mov eax, 0x18; test ...; je ...; syscall; ret; int 2eh; ret
        let stub = [
            0x4C, 0x8B, 0xD1, 0xB8, 0x18, 0x00, 0x00, 0x00, 0xF6, 0x04, 0x25, 0x08, 0x03, 0xFE,
            0x7F, 0x01, 0x74, 0x03, 0x0F, 0x05, 0xC3, 0xCD, 0x2E, 0xC3,
        ];
        let info = disasm_x64(&stub);
        assert_eq!(info.ssn, Some(0x18));
        assert_eq!(info.arity, None); // x64 doesn't encode arity
    }

    #[test]
    fn x86_canonical_stub() {
        // mov eax, 0x10; mov edx, 0x770000; call edx; ret 4
        let stub = [
            0xB8, 0x10, 0x00, 0x00, 0x00, 0xBA, 0x00, 0x00, 0x77, 0x00, 0xFF, 0xD2, 0xC2, 0x04,
            0x00,
        ];
        let info = disasm_x86(&stub);
        assert_eq!(info.ssn, Some(0x10));
        assert_eq!(info.arity, Some(1));
    }

    #[test]
    fn x86_six_args() {
        // NtAllocateVirtualMemory: 6 args → ret 24 (= 0x18)
        let stub = [
            0xB8, 0x18, 0x00, 0x00, 0x00, 0xBA, 0, 0, 0, 0, 0xFF, 0xD2, 0xC2, 0x18, 0x00,
        ];
        let info = disasm_x86(&stub);
        assert_eq!(info.arity, Some(6));
    }

    #[test]
    fn hex_is_uppercase() {
        assert_eq!(hex_upper(&[0x4C, 0x8B, 0xD1]), "4C8BD1");
    }
}
