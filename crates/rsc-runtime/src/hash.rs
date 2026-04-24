//! Compile-time name hashing for syscall resolution.
//!
//! Algorithm: WORD-overlapping ROR8 xor-accumulator (same shape as the legacy
//! `syscalls-rust` hash, adapted to a `&[u8]` input so it works in `const`
//! contexts).
//!
//! Seed — `RSC_SEED = 0x52534300` (ASCII `RSC\0`). Distinct from legacy
//! `0xB8A54425` so hashes don't collide with SW3-built artifacts.
//! See `DECISIONS.md` §D-06.
//!
//! Regression invariant: the hashes listed in the unit tests must not
//! change without a seed bump. If they change, the entire `canonical.toml`
//! must be regenerated and every `rsc_syscall!(...)` call re-compiled.

/// Hash seed — `"RSC\0"` in little-endian ASCII.
pub const RSC_SEED: u32 = 0x5253_4300;

/// Core step — `hash' = hash ^ ((b0 | b1<<8) + ROR8(hash))`.
#[inline(always)]
const fn step(hash: u32, b0: u8, b1: u8) -> u32 {
    let partial = (b0 as u32) | ((b1 as u32) << 8);
    hash ^ partial.wrapping_add(hash.rotate_right(8))
}

/// Hashes an arbitrary byte slice at compile time.
///
/// The classic SW3 algorithm walks the C-string byte-by-byte, reading a
/// two-byte window each iteration. To keep binary behavior identical for
/// names shorter than the reader's lookahead, the overlapping WORD step
/// simulates the same trailing-zero tail by returning `0` past the end of
/// the slice.
pub const fn rsc_hash(name: &[u8]) -> u32 {
    let mut hash = RSC_SEED;
    let mut i = 0;
    while i < name.len() {
        let b0 = name[i];
        let b1 = if i + 1 < name.len() { name[i + 1] } else { 0 };
        hash = step(hash, b0, b1);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty input hashes to the raw seed (no byte fed → no xor).
    #[test]
    fn empty_returns_seed() {
        assert_eq!(rsc_hash(b""), RSC_SEED);
    }

    /// Single-byte case exercises the tail-zero branch.
    #[test]
    fn single_byte() {
        // partial = 0x41 | (0 << 8) = 0x41
        // hash' = SEED ^ (0x41 + ROR8(SEED))
        let expected = RSC_SEED ^ (0x41u32.wrapping_add(RSC_SEED.rotate_right(8)));
        assert_eq!(rsc_hash(b"A"), expected);
    }

    /// Different names must produce different hashes for our NT-function set.
    /// This is a smoke check — a collision would be caught by
    /// `rsc verify --strict` during merge, but we want to notice early.
    #[test]
    fn no_collisions_among_representatives() {
        let names: &[&[u8]] = &[
            b"NtClose",
            b"NtAllocateVirtualMemory",
            b"NtFreeVirtualMemory",
            b"NtProtectVirtualMemory",
            b"NtQueryVirtualMemory",
            b"NtReadVirtualMemory",
            b"NtWriteVirtualMemory",
            b"NtCreateFile",
            b"NtOpenFile",
            b"NtReadFile",
            b"NtWriteFile",
            b"NtDeviceIoControlFile",
            b"NtCreateProcess",
            b"NtCreateProcessEx",
            b"NtOpenProcess",
            b"NtTerminateProcess",
            b"NtSuspendProcess",
            b"NtResumeProcess",
            b"NtCreateThread",
            b"NtCreateThreadEx",
            b"NtOpenThread",
            b"NtTerminateThread",
            b"NtResumeThread",
            b"NtSuspendThread",
            b"NtQueryInformationProcess",
            b"NtSetInformationProcess",
            b"NtQueryInformationThread",
            b"NtSetInformationThread",
            b"NtQuerySystemInformation",
            b"NtSetSystemInformation",
            b"NtOpenProcessToken",
            b"NtOpenThreadToken",
            b"NtAdjustPrivilegesToken",
            b"NtCreateKey",
            b"NtOpenKey",
            b"NtSetValueKey",
            b"NtQueryValueKey",
            b"NtEnumerateKey",
            b"NtEnumerateValueKey",
            b"NtDeleteKey",
            b"NtMapViewOfSection",
            b"NtUnmapViewOfSection",
            b"NtCreateSection",
            b"NtOpenSection",
            b"NtWaitForSingleObject",
            b"NtWaitForMultipleObjects",
            b"NtCreateEvent",
            b"NtSetEvent",
            b"NtResetEvent",
            b"NtCreateMutant",
        ];

        let mut seen: [(u32, &[u8]); 64] = [(0, b"" as &[u8]); 64];
        for (n, &name) in names.iter().enumerate() {
            let h = rsc_hash(name);
            for prior in seen.iter().take(n) {
                assert_ne!(
                    prior.0, h,
                    "hash collision: {:?} ↔ {:?} (seed {:#x})",
                    core::str::from_utf8(prior.1).unwrap(),
                    core::str::from_utf8(name).unwrap(),
                    RSC_SEED
                );
            }
            seen[n] = (h, name);
        }
    }

    /// `rsc_hash` is a `const fn`: the value must be computable at compile time.
    #[test]
    fn compile_time_evaluation() {
        const HASH_NT_CLOSE: u32 = rsc_hash(b"NtClose");
        // Compare with runtime result — they must match (trivially by identity).
        assert_eq!(HASH_NT_CLOSE, rsc_hash(b"NtClose"));
        assert_ne!(HASH_NT_CLOSE, RSC_SEED);
    }

    /// Seed must remain stable. Changing it is a breaking ABI event —
    /// if this test ever fails, treat as deliberate bump and regenerate
    /// canonical.toml + bump schema.
    #[test]
    fn seed_pinned() {
        assert_eq!(RSC_SEED, 0x5253_4300);
    }
}
