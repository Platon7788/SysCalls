# API — SysCalls (RSC)

Спецификация публичного API. **Документ обновляется по мере реализации фаз.** Сейчас — проектный draft.

> **Статус**: 📋 Draft (ничего не реализовано, это план API). Актуализируется после Phase 2, Phase 6, Phase 7.

## 1. Rust API — `rsc-runtime`

### 1.1 Re-exports

Публичный API `rsc-runtime` в v1.0 (из `crates/rsc-runtime/src/lib.rs`):

```rust
// Hash
pub use hash::{rsc_hash, RSC_SEED};

// Syscall table / resolver
pub use table::{count, resolve, __rsc_random_slide, __rsc_resolve_ssn};

// Errors
pub use error::{NtStatus, NtStatusExt, RscResult};

// Модули с типами и константами
pub mod types;      // HANDLE, PVOID, NTSTATUS, UNICODE_STRING, ...
pub mod constants;  // STATUS_*, PAGE_*, MEM_*, PROCESS_*, THREAD_*, ...

// Auto-generated syscall stubs (509 штук)
pub mod syscalls;   // nt_allocate_virtual_memory, nt_close, ...
```

`__rsc_random_slide` и `__rsc_resolve_ssn` — `extern "system"` ABI-
обёртки, которые вызываются внутри naked stubs из `rsc-codegen`. Они
являются частью стабильного контракта между proc-macro и runtime, но
обычному consumer-коду не нужны.

### 1.2 Пример использования

```rust
use rsc_runtime::{
    nt_allocate_virtual_memory, nt_free_virtual_memory,
    HANDLE, PVOID, NTSTATUS, SIZE_T,
    MEM_COMMIT, MEM_RESERVE, MEM_RELEASE, PAGE_READWRITE,
    STATUS_SUCCESS,
};
use core::ptr;

fn main() {
    const CURRENT_PROCESS: HANDLE = -1isize as HANDLE;

    let mut base: PVOID = ptr::null_mut();
    let mut size: SIZE_T = 0x1000;

    // SAFETY: passing valid mutable references to out-params.
    let status: NTSTATUS = unsafe {
        nt_allocate_virtual_memory(
            CURRENT_PROCESS,
            &mut base,
            0,
            &mut size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };

    assert_eq!(status, STATUS_SUCCESS);
    println!("Allocated {} bytes at {:p}", size, base);

    // Free
    let mut zero: SIZE_T = 0;
    let status = unsafe {
        nt_free_virtual_memory(CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE)
    };
    assert_eq!(status, STATUS_SUCCESS);
}
```

### 1.3 RscResult helper

```rust
use rsc_runtime::{RscResult, NtStatusExt};

fn allocate_or_fail(size: SIZE_T) -> RscResult<PVOID> {
    let mut base: PVOID = ptr::null_mut();
    let mut sz = size;
    unsafe {
        nt_allocate_virtual_memory(
            -1isize as HANDLE,
            &mut base,
            0,
            &mut sz,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    }
    .to_result_with(|| base)  // NtStatusExt helper
}
```

### 1.4 Сигнатуры (пример)

```rust
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
pub unsafe extern "system" fn nt_allocate_virtual_memory(
    process_handle: HANDLE,
    base_address: *mut PVOID,
    zero_bits: ULONG_PTR,
    region_size: *mut SIZE_T,
    allocation_type: u32,
    protect: u32,
) -> NTSTATUS {
    // naked_asm! expanded by rsc_syscall! macro
}
```

## 2. C API — `rsc-c` (rsc.h)

### 2.1 Типы

```c
#ifndef RSC_H
#define RSC_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Avoid conflicts with windows.h
#ifndef _WINDEF_
typedef void*    RSC_HANDLE;
typedef void*    RSC_PVOID;
typedef int32_t  RSC_NTSTATUS;
typedef uintptr_t RSC_SIZE_T;
typedef uintptr_t RSC_ULONG_PTR;
typedef uint8_t  RSC_BOOLEAN;
typedef uint8_t  RSC_UCHAR;
typedef uint16_t RSC_USHORT;
typedef uint32_t RSC_ULONG;
typedef uint64_t RSC_ULONG64;
#else
#define RSC_HANDLE    HANDLE
#define RSC_PVOID     PVOID
#define RSC_NTSTATUS  NTSTATUS
// ...
#endif

typedef struct _RSC_UNICODE_STRING {
    RSC_USHORT Length;
    RSC_USHORT MaximumLength;
    uint16_t* Buffer;
} RSC_UNICODE_STRING, *RSC_PUNICODE_STRING;

typedef struct _RSC_OBJECT_ATTRIBUTES {
    RSC_ULONG Length;
    RSC_HANDLE RootDirectory;
    RSC_PUNICODE_STRING ObjectName;
    RSC_ULONG Attributes;
    RSC_PVOID SecurityDescriptor;
    RSC_PVOID SecurityQualityOfService;
} RSC_OBJECT_ATTRIBUTES, *RSC_POBJECT_ATTRIBUTES;

// ... more types
```

### 2.2 Константы

```c
#define RSC_STATUS_SUCCESS           ((RSC_NTSTATUS)0x00000000)
#define RSC_STATUS_ACCESS_DENIED     ((RSC_NTSTATUS)0xC0000022)
#define RSC_STATUS_INVALID_HANDLE    ((RSC_NTSTATUS)0xC0000008)
// ...

#define RSC_PAGE_READWRITE           0x04
#define RSC_PAGE_EXECUTE_READ        0x20
#define RSC_PAGE_EXECUTE_READWRITE   0x40

#define RSC_MEM_COMMIT               0x1000
#define RSC_MEM_RESERVE              0x2000
#define RSC_MEM_RELEASE              0x8000

#define RSC_PROCESS_ALL_ACCESS       0x001FFFFF
#define RSC_THREAD_ALL_ACCESS        0x001FFFFF

// ...
```

### 2.3 Функции

```c
RSC_NTSTATUS RscNtAllocateVirtualMemory(
    RSC_HANDLE     ProcessHandle,
    RSC_PVOID*     BaseAddress,
    RSC_ULONG_PTR  ZeroBits,
    RSC_SIZE_T*    RegionSize,
    RSC_ULONG      AllocationType,
    RSC_ULONG      Protect
);

RSC_NTSTATUS RscNtFreeVirtualMemory(
    RSC_HANDLE  ProcessHandle,
    RSC_PVOID*  BaseAddress,
    RSC_SIZE_T* RegionSize,
    RSC_ULONG   FreeType
);

RSC_NTSTATUS RscNtClose(RSC_HANDLE Handle);

// ... ~500 functions
```

### 2.4 Пример

```c
#include <rsc.h>
#include <stdio.h>

int main() {
    RSC_HANDLE proc = (RSC_HANDLE)-1;
    RSC_PVOID base = NULL;
    RSC_SIZE_T size = 0x1000;

    RSC_NTSTATUS status = RscNtAllocateVirtualMemory(
        proc, &base, 0, &size,
        RSC_MEM_COMMIT | RSC_MEM_RESERVE, RSC_PAGE_READWRITE
    );

    if (status == RSC_STATUS_SUCCESS) {
        printf("Allocated %zu bytes at %p\n", (size_t)size, base);

        RSC_SIZE_T zero = 0;
        RscNtFreeVirtualMemory(proc, &base, &zero, 0x8000);
    }
    return 0;
}
```

Сборка MSVC:
```
cl test.c /I<rsc-include> /link rsc.lib ntdll.lib kernel32.lib
```

Сборка MinGW:
```
gcc test.c -L<rsc-lib> -lrsc_mingw -lntdll -lkernel32
```

## 3. CLI API — `rsc`

### 3.1 Subcommands

```
rsc <COMMAND> [OPTIONS]

Commands:
  collect       Collect auto-layer snapshot for current Windows build
  phnt-parse    Parse vendor/phnt headers into db/phnt/phnt.toml
  merge         Merge auto + phnt + overrides → canonical.toml
  verify        Drift / integrity checks
  diff          Compare two builds
  stats         Coverage and category stats
  help          Show help

Global flags:
  --db-dir <PATH>         Override db/ location
  --verbose, -v           Increase log verbosity (repeat: -vv, -vvv)
  --quiet, -q             Suppress non-error output
  --help, -h              Print help
  --version               Print version
```

### 3.2 `rsc collect`

```
rsc collect [--force] [--arch x64|x86|both] [--build-id <id>]

  --force            Re-download PDB and regenerate even if file exists
  --arch <ARCH>      Collect only specified arch (default: both on x64 OS)
  --build-id <ID>    Override auto-detected build (e.g. "11_26100_2314")
                     (useful for testing with committed snapshots)
```

### 3.3 `rsc merge`

```
rsc merge [--baseline <build-id>] [--output <path>]

  --baseline <ID>    Which auto/*.toml to use as base (default: latest)
  --output <PATH>    Output path (default: db/canonical.toml)
```

### 3.4 `rsc verify`

```
rsc verify [--strict]

  --strict           Exit code 1 on any warning (default: only errors)
```

Returns:
- `0` — OK
- `1` — validation error (or any warning with --strict)
- `2` — CLI error (bad args, missing files)

### 3.5 `rsc diff`

```
rsc diff <BUILD_A> <BUILD_B> [--format toml|md|text]

Examples:
  rsc diff 10_19045_6466 11_26100_2314
  rsc diff 10_19045_6466 11_26100_2314 --format md > CHANGES.md
```

### 3.6 `rsc stats`

```
rsc stats [--format toml|text]

Outputs counts by category, coverage by phnt, opaque signatures, etc.
```

## 4. Macro API — `rsc-codegen`

### 4.1 `rsc_syscall!`

Используется внутри `rsc-runtime`, экспортируется из `rsc-codegen`. Пользователь обычно НЕ вызывает напрямую — всё генерируется в `build.rs`.

```rust
// Синтаксис (draft):
rsc_syscall!(
    name: NtAllocateVirtualMemory,
    arity: 6,
    params: (
        HANDLE,
        *mut PVOID,
        ULONG_PTR,
        *mut SIZE_T,
        u32,
        u32,
    ),
    return: NTSTATUS,
);
```

Разворачивается в:
```rust
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
pub unsafe extern "system" fn nt_allocate_virtual_memory(
    _1: HANDLE, _2: *mut PVOID, _3: ULONG_PTR, _4: *mut SIZE_T, _5: u32, _6: u32,
) -> NTSTATUS {
    core::arch::naked_asm!(
        // ... x64 template with const SSN=rsc_hash("NtAllocateVirtualMemory")
    )
}

#[cfg(target_arch = "x86")]
#[unsafe(naked)]
pub unsafe extern "system" fn nt_allocate_virtual_memory(
    _1: HANDLE, _2: *mut PVOID, _3: ULONG_PTR, _4: *mut SIZE_T, _5: u32, _6: u32,
) -> NTSTATUS {
    core::arch::naked_asm!(
        // ... x86 + WoW64 template with const SSN=rsc_hash("NtAllocateVirtualMemory")
    )
}
```

## 5. Build-time API — `rsc-runtime/build.rs`

Читает `RSC_CANONICAL_PATH` (или default `../../db/canonical.toml`) и эмитит файл `$OUT_DIR/syscalls_generated.rs` с вызовами макроса.

Env vars:
- `RSC_CANONICAL_PATH` — путь к canonical.toml

## 6. Семантическое версионирование

- `0.x.y` — до первого релиза: ломающие изменения разрешены в любой версии
- `1.0.0` — первый стабильный релиз: MAJOR bump для API-breaking

**Что считается breaking**:
- Удаление / переименование публичной функции
- Изменение сигнатуры (типов параметров)
- Изменение ABI
- Изменение формата `canonical.toml` schema_version
- Изменение `RSC_SEED` (хеши больше не совпадают)

**Не breaking**:
- Добавление новой syscall функции
- Добавление новой константы
- Внутренние refactoring
- Обновление phnt submodule (если все подписи совместимы)

## 7. Стабильность

| Компонент | Стабильность API | Когда становится стабильным |
|---|---|---|
| Rust типы (HANDLE, NTSTATUS, ...) | planned stable @ v1.0 | Phase 9 |
| Syscall функции (nt_*) | planned stable @ v1.0 | Phase 9 |
| NtStatus / RscResult | planned stable @ v1.0 | Phase 9 |
| rsc_syscall! macro | **unstable** | Internal use |
| canonical.toml format | schema_version | versioned |
| C API (RSC_*) | planned stable @ v1.0 | Phase 9 |
| CLI commands | planned stable @ v1.0 | Phase 9 |

Проект в Phase 0-8 — **всё API может меняться**.
