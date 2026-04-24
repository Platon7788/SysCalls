//! JUMPER pattern: locate `syscall; ret` / `sysenter; ret` slides inside
//! `ntdll`, plus a lightweight PRNG for per-call random dispatch.
//!
//! Dispatching *into* ntdll (rather than executing `syscall` from our own
//! code region) hides the return address behind a legitimate RIP, frustrating
//! stack-trace-based EDR / anti-cheat heuristics. See `DECISIONS.md` §D-07
//! and §S-13.

use core::sync::atomic::{AtomicU32, Ordering};

// --- Architecture-specific constants --------------------------------------

#[cfg(target_arch = "x86_64")]
mod arch {
    /// Offset from the start of a standard 64-bit `Nt*` stub to its
    /// `syscall; ret` tail: `mov r10, rcx` (3) + `mov eax, imm32` (5)
    /// + `test byte[..]` (10) = 0x12.
    pub const DISTANCE_TO_SYSCALL: usize = 0x12;
    pub const SYSCALL_TAIL: [u8; 3] = [0x0F, 0x05, 0xC3]; // syscall; ret
}

#[cfg(target_arch = "x86")]
mod arch {
    /// Offset from the start of a 32-bit `Nt*` stub to its `sysenter; ret`
    /// tail on native x86.
    pub const DISTANCE_TO_SYSCALL: usize = 0x0F;
    pub const SYSCALL_TAIL: [u8; 3] = [0x0F, 0x34, 0xC3]; // sysenter; ret
}

pub use arch::*;

/// HalosGate search window — matches `syscalls-rust` legacy behavior.
pub const HALOS_STEP: usize = 0x20;
pub const HALOS_LIMIT: usize = 512;

/// Finds a `syscall; ret` slide for the given `Nt*` function.
///
/// First tries the canonical offset; if the function is hooked, slides up
/// and down by 0x20-byte windows (HalosGate) looking for an intact stub
/// of a neighbor syscall.
///
/// Returns `0` if no usable slide was found — caller must treat this as a
/// resolution failure.
///
/// # Safety
///
/// `fn_addr` must point inside a mapped image; the search walks up to
/// `HALOS_STEP * HALOS_LIMIT` bytes in either direction, which must remain
/// within readable memory. Ntdll's `.text` segment easily satisfies this.
pub(crate) unsafe fn find_syscall_slide(fn_addr: *const u8) -> usize {
    if fn_addr.is_null() {
        return 0;
    }

    // SAFETY: bounded by HALOS_LIMIT and the caller's guarantee.
    unsafe {
        let direct = fn_addr.add(DISTANCE_TO_SYSCALL);
        if matches_tail(direct) {
            return direct as usize;
        }

        for n in 1..HALOS_LIMIT {
            let up = fn_addr.add(DISTANCE_TO_SYSCALL + n * HALOS_STEP);
            if matches_tail(up) {
                return up as usize;
            }
            if DISTANCE_TO_SYSCALL >= n * HALOS_STEP {
                let down = fn_addr.add(DISTANCE_TO_SYSCALL - n * HALOS_STEP);
                if matches_tail(down) {
                    return down as usize;
                }
            }
        }
    }
    0
}

#[inline]
unsafe fn matches_tail(p: *const u8) -> bool {
    // SAFETY: caller ensures `p .. p+3` is readable.
    unsafe {
        *p == SYSCALL_TAIL[0]
            && *p.add(1) == SYSCALL_TAIL[1]
            && *p.add(2) == SYSCALL_TAIL[2]
    }
}

// --- xorshift32 PRNG -------------------------------------------------------

/// Global xorshift32 state. Seeded on first use from the RDTSC counter —
/// non-cryptographic randomness is enough to break trivial stack-trace
/// pattern matching. Atomic-CAS so concurrent callers don't clobber each
/// other; with weakly-ordered loads we don't even care about torn reads.
static PRNG_STATE: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "x86_64")]
#[inline]
fn rdtsc_seed() -> u32 {
    let lo: u32;
    let hi: u32;
    // SAFETY: `rdtsc` is always available on x64 Windows user mode unless
    // explicitly disabled; we read both halves for entropy.
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    lo ^ hi
}

#[cfg(target_arch = "x86")]
#[inline]
fn rdtsc_seed() -> u32 {
    let lo: u32;
    let hi: u32;
    // SAFETY: `rdtsc` is available on all Pentium+ CPUs; we ignore
    // `CR4.TSD` (always 0 in user mode on modern Windows).
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    lo ^ hi
}

/// Returns a fresh xorshift32 value. Seeds itself lazily from `rdtsc` the
/// first time it's called.
#[inline]
pub(crate) fn next_random() -> u32 {
    let mut state = PRNG_STATE.load(Ordering::Relaxed);
    if state == 0 {
        // Seed lazily. Multiple threads may race — each will pick its own
        // seed and the winner publishes; that's fine.
        state = rdtsc_seed();
        if state == 0 {
            state = 0xDEAD_BEEF;
        }
    }
    // xorshift32 (Marsaglia)
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    PRNG_STATE.store(state, Ordering::Relaxed);
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halosgate_constants_are_stable() {
        #[cfg(target_arch = "x86_64")]
        {
            assert_eq!(DISTANCE_TO_SYSCALL, 0x12);
            assert_eq!(SYSCALL_TAIL, [0x0F, 0x05, 0xC3]);
        }
        #[cfg(target_arch = "x86")]
        {
            assert_eq!(DISTANCE_TO_SYSCALL, 0x0F);
            assert_eq!(SYSCALL_TAIL, [0x0F, 0x34, 0xC3]);
        }
    }

    #[test]
    fn matches_tail_recognizes_sequence() {
        let buf = [SYSCALL_TAIL[0], SYSCALL_TAIL[1], SYSCALL_TAIL[2], 0xAA];
        // SAFETY: buffer stays alive for the duration of the call.
        assert!(unsafe { matches_tail(buf.as_ptr()) });
        let wrong = [0xAA, 0xBB, 0xCC];
        assert!(!unsafe { matches_tail(wrong.as_ptr()) });
    }

    #[test]
    fn prng_produces_varied_output() {
        let mut samples = [0u32; 16];
        for slot in &mut samples {
            *slot = next_random();
        }
        // Not all equal — trivially unlikely to fail if the PRNG works.
        let all_same = samples.iter().all(|&v| v == samples[0]);
        assert!(!all_same);
    }
}
