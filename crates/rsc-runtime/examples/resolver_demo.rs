//! Resolver demo — shows how many syscalls the runtime discovered and
//! prints the SSN + slide for a handful of well-known ones. No actual
//! syscall is invoked, so this runs safely on any architecture / OS.

fn main() {
    let count = rsc_runtime::count();
    println!("rsc-runtime resolved {count} syscalls from ntdll");
    println!();

    let names: &[&[u8]] = &[
        b"NtClose",
        b"NtAllocateVirtualMemory",
        b"NtFreeVirtualMemory",
        b"NtProtectVirtualMemory",
        b"NtQueryVirtualMemory",
        b"NtReadVirtualMemory",
        b"NtWriteVirtualMemory",
        b"NtCreateFile",
        b"NtReadFile",
        b"NtWriteFile",
        b"NtOpenProcess",
        b"NtOpenThread",
        b"NtQuerySystemInformation",
        b"NtOpenProcessToken",
        b"NtAdjustPrivilegesToken",
    ];

    println!("{:<32}  {:<10}  {:<5}  slide", "name", "hash", "ssn");
    let sep = "-".repeat(72);
    println!("{sep}");
    for &name in names {
        let h = rsc_runtime::rsc_hash(name);
        match rsc_runtime::resolve(h) {
            Some((ssn, slide)) => println!(
                "{:<32}  {:#010x}  {:<5}  {:#018x}",
                core::str::from_utf8(name).unwrap(),
                h,
                ssn,
                slide
            ),
            None => println!(
                "{:<32}  {:#010x}  {:<5}  {:<18}",
                core::str::from_utf8(name).unwrap(),
                h,
                "?",
                "(not found)"
            ),
        }
    }
}
