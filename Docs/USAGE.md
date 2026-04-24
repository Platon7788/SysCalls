# USAGE — как пользоваться SysCalls (RSC)

Практическое руководство. Показывает полный flow: от клонирования до
написания red-team / research кода на Rust или C с прямыми syscall'ами.

---

## Оглавление

1. [Предварительные требования](#1-предварительные-требования)
2. [Quick start — один скрипт](#2-quick-start--один-скрипт)
3. [Что делает каждый шаг pipeline](#3-что-делает-каждый-шаг-pipeline)
4. [Использование из Rust](#4-использование-из-rust)
5. [Использование из C / C++](#5-использование-из-c--c)
6. [**Импорт в сторонний Rust-проект**](#6-импорт-в-сторонний-rust-проект) ⭐
7. [CLI reference — `rsc`](#7-cli-reference--rsc)
8. [Скрипты в `scripts/`](#8-скрипты-в-scripts)
9. [Recipes — частые сценарии](#9-recipes--частые-сценарии)
10. [Overrides — ручные правки БД](#10-overrides--ручные-правки-бд)
11. [Features и env vars](#11-features-и-env-vars)
12. [Troubleshooting](#12-troubleshooting)
13. [Для нового Windows update / новой версии ntdll](#13-для-нового-windows-update--новой-версии-ntdll)

---

## 1. Предварительные требования

| Компонент | Версия | Зачем |
|---|---|---|
| **Rust** | stable ≥ 1.88 (проверено на 1.95) | `#[unsafe(naked)]` + `naked_asm!` |
| **Windows** | Win10 1809+ (build ≥ 17763) | baseline, WoW64 gate |
| **Git** | любая свежая | phnt submodule |
| **Internet** (первый запуск) | — | скачать PDB с Microsoft Symbol Server |
| **MSVC** (только для C примера) | VS 2019/2022 x64 | `cl.exe` |
| **MinGW** (опционально) | GCC 10+ | альтернативный target |

Targets Rust:
```
rustup target add i686-pc-windows-msvc       # x86 / WoW64
rustup target add x86_64-pc-windows-gnu      # MinGW
rustup target add i686-pc-windows-gnu        # MinGW x86
```

---

## 2. Quick start

### Для consumer'а — просто использовать syscalls из своего проекта

```bash
git clone <repo>
cd my-project
# в Cargo.toml:
#   rsc-runtime = { path = "<где-то>/SysCalls/crates/rsc-runtime" }
cargo build        # Just Works™
```

**Никаких** collect/phnt/merge шагов. `db/canonical.toml` закоммичен в
репо (**Path A** baked distribution), так что consumer ничего не качает
и не парсит при сборке.

### Для maintainer'а — обновить baked DB под новую Windows

```bash
cd SysCalls
git submodule update --init vendor/phnt    # один раз, ~5 MB
scripts\refresh.bat                         # collect → phnt → merge → verify → stats
git diff db/                                # review
git add db/ && git commit -m "refresh canonical for <new Win build>"
```

`refresh.bat` делает:
1. `rsc-collector --force` — скачивает `ntdll.pdb` для текущей Windows → `db/auto/<Build>.toml`
2. `rsc-types` — парсит phnt → `db/phnt/phnt.toml`
3. `rsc merge` — **union всех `db/auto/*.toml`** (multi-Windows coverage) → `db/canonical.toml`
4. `rsc verify` — sanity checks
5. `rsc stats` — dashboard с per-build breakdown

### Release-артефакты после `scripts\build.bat` (или `cargo build --workspace --release`)

В `target/release/`:
- `rsc.dll` / `rsc.lib` / `rsc.dll.lib` — C-bindings (cdylib + static + import)
- `rsc.exe` — unified CLI (`rsc merge`, `rsc stats`, `rsc diff`, `rsc verify`)
- `rsc-collector.exe` / `rsc-types.exe` — maintainer tools

В `examples/consumer-template/target/release/` (после `cargo run --release` там):
- `rsc-consumer-template.exe` — 280 KB внешний consumer с полным набором 509 функций

---

## 3. Что делает каждый шаг pipeline

```
┌───────────────────────┐    download+parse    ┌─────────────────────────┐
│ Microsoft Symbol      │ ───────────────────► │ db/auto/<build>.toml    │
│ Server (ntdll.pdb)    │    rsc-collector     │   ~500 syscalls × SSN   │
└───────────────────────┘                      └─────────────────────────┘
                                                         │
┌───────────────────────┐    parse             ┌─────────▼───────────────┐
│ vendor/phnt/*.h       │ ───────────────────► │ db/phnt/phnt.toml       │
│ (pinned submodule)    │    rsc-types         │   ~780 typed signatures │
└───────────────────────┘                      └─────────────────────────┘
                                                         │
┌───────────────────────┐                                │
│ db/overrides.toml     │ ───────────────┐               │
│ (hand-edited)         │                │               │
└───────────────────────┘                ▼               ▼
                                    ┌─────────────────────────────┐
                                    │ rsc merge                   │
                                    │   overrides > phnt > auto   │
                                    └──────────┬──────────────────┘
                                               ▼
                                    ┌─────────────────────────────┐
                                    │ db/canonical.toml           │
                                    │   509 syscalls, rsc_hash    │
                                    └──────────┬──────────────────┘
                                               │ build.rs
                                               ▼
                                ┌──────────────────────────────────┐
                                │ rsc-runtime stubs                │
                                │  (509 × `rsc_syscall!` expanded) │
                                └──────────────────────────────────┘
```

---

## 4. Использование из Rust (in-tree)

Этот раздел — про запуск/правку кода **внутри** SysCalls workspace.
Если нужно подключить SysCalls как зависимость в *отдельный* проект —
перескакивай в §6.

### 4.1. Публичный API `rsc-runtime`

Три модуля + re-exports в корне:

| Путь | Что внутри |
|---|---|
| `rsc_runtime::syscalls::*`  | **509 NT-функций** (`NtClose`, `NtAllocateVirtualMemory`, `NtCreateFile`, …) — имена в PascalCase как в Windows SDK |
| `rsc_runtime::constants::*` | `MEM_COMMIT`, `PAGE_EXECUTE_READWRITE`, `STATUS_SUCCESS`, `OBJ_*`, `THREAD_*`, `PROCESS_*`, … |
| `rsc_runtime::types::*`     | `HANDLE`, `PVOID`, `SIZE_T`, `NT_CURRENT_PROCESS`, `NT_CURRENT_THREAD`, `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `CLIENT_ID` |
| `rsc_runtime::{NtStatus, NtStatusExt, RscResult}` | Type-safe обёртка `i32 NTSTATUS` → `Result<(), NtStatus>` |
| `rsc_runtime::{rsc_hash, resolve, count}`          | Низкоуровневый resolver: hash имени, `(ssn, slide)` lookup, счётчик |

### 4.2. Минимальный пример (alloc → touch → free)

```rust
use rsc_runtime::constants::{MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use rsc_runtime::error::STATUS_SUCCESS;
use rsc_runtime::syscalls::{NtAllocateVirtualMemory, NtFreeVirtualMemory};
use rsc_runtime::types::{NT_CURRENT_PROCESS, PVOID, SIZE_T};

fn main() {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = 0x1000;

    // SAFETY: out-параметры корректные, флаги валидны, handle — pseudo-handle.
    let status = unsafe {
        NtAllocateVirtualMemory(
            NT_CURRENT_PROCESS,
            &mut base,
            0,
            &mut size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    assert_eq!(status, STATUS_SUCCESS.code());
    println!("allocated {size} bytes at {base:p}");

    // SAFETY: page принадлежит нам, size покрывает запись.
    unsafe { core::ptr::write(base as *mut u64, 0xDEAD_BEEF_CAFE_BABE) };

    let mut zero: SIZE_T = 0;
    // SAFETY: base — активная аллокация; MEM_RELEASE требует size == 0.
    unsafe {
        NtFreeVirtualMemory(NT_CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE);
    }
}
```

### 4.3. Error handling через `RscResult`

Каждый NT-вызов возвращает сырой `i32 NTSTATUS`. Для идиоматичного Rust:

```rust
use rsc_runtime::{NtStatusExt, RscResult, HANDLE};
use rsc_runtime::syscalls::NtClose;

fn close_handle(h: HANDLE) -> RscResult<()> {
    // `.to_result()` вернёт Ok(()) на NT_SUCCESS, Err(NtStatus) — иначе.
    unsafe { NtClose(h) }.to_result()
}

fn main() {
    let bogus = 0xDEAD_BEEF_usize as HANDLE;
    match close_handle(bogus) {
        Ok(()) => unreachable!(),
        Err(e) => eprintln!(
            "NtClose: {:#010x} ({})",
            e.code() as u32,
            e.name().unwrap_or("STATUS_UNKNOWN"),
        ),
    }
}
```

`NtStatus` знает ~260 именованных кодов (`STATUS_INVALID_HANDLE`, `STATUS_ACCESS_VIOLATION`, …) и severity (`Success`/`Information`/`Warning`/`Error`).

### 4.4. Прямой resolve — SSN/slide без вызова

Для своих naked-стабов, benchmark'ов, hook-detection:

```rust
let h = rsc_runtime::rsc_hash(b"NtClose");
if let Some((ssn, slide)) = rsc_runtime::resolve(h) {
    println!("NtClose: ssn={ssn:#x}, slide @ ntdll = {slide:#018x}");
}
println!("total resolved: {}", rsc_runtime::count());
```

### 4.5. `#![no_std]` consumer

`rsc-runtime` сам по себе `#![no_std]` с **нулевыми** runtime deps. Пригоден для injectable DLL, drivers с user-mode thunk'ами, UEFI payloads (с поправкой на CRT target). Из `std`-crate тоже работает без специальных флагов — `no_std` в зависимости не форсит `no_std` в consumer'е.

### 4.6. Запустить тестовый пример прямо сейчас

```bash
cd examples/consumer-template
cargo run --release
# [*] rsc-runtime resolved 509 syscalls from this process's ntdll
# [+] allocated 4096 bytes at 0x...
# [+] wrote + read back sentinel 0xDEAD_BEEF_CAFE_BABE
# [+] NtQueryVirtualMemory: Protect=0x4, State=0x1000, RegionSize=0x1000
# [+] freed
# [*] NtClose(0xDEADBEEF) -> 0xc0000008 (expected STATUS_INVALID_HANDLE = 0xC0000008)
# [*] all template checks passed
```

Release binary: **~280 KB**, полный набор из 509 NT-функций.

---

## 5. Использование из C / C++

`rsc-c` собирает один и тот же код в **staticlib** (`rsc.lib` + `libsc_mingw.a`) и **cdylib** (`rsc.dll`). Заголовок `rsc.h` генерируется автоматически из canonical.toml — **509 прототипов**, все с префиксом `Rsc`.

### 5.1. Собрать биндинги

```bash
# из корня SysCalls
cargo build --release -p rsc-c
```

Артефакты:

```
target/release/rsc.dll         ← cdylib для dynamic linking
target/release/rsc.lib         ← import lib для rsc.dll (MSVC)
target/release/rsc.dll.lib     ← то же (alt name)
target/release/librsc.a        ← staticlib (MinGW GCC; MSVC тоже ест)
crates/rsc-c/include/rsc.h     ← прототипы (~509 функций, ~120 KB)
```

**Что связывать**: либо `rsc.lib` (static, symbols зальются в твой EXE) либо `rsc.dll.lib` (import, EXE тянет `rsc.dll` в runtime). Для injectable payload'ов почти всегда нужен **static** — меньше зависимостей при загрузке.

### 5.2. MSVC — минимальный C-пример

**`D:/MyTool/hello.c`:**

```c
#include <stdio.h>
#include <stdint.h>
#include "rsc.h"

int main(void) {
    RSC_HANDLE  proc = (RSC_HANDLE)(intptr_t)-1;   /* current process pseudo-handle */
    RSC_PVOID   base = NULL;
    RSC_SIZE_T  size = 0x1000;

    RSC_NTSTATUS st = RscNtAllocateVirtualMemory(
        proc, &base, 0, &size,
        RSC_MEM_COMMIT | RSC_MEM_RESERVE,
        RSC_PAGE_READWRITE);
    if (st != RSC_STATUS_SUCCESS) {
        fprintf(stderr, "alloc failed: 0x%08X\n", (unsigned)st);
        return 1;
    }
    printf("[+] allocated %zu bytes at %p\n", (size_t)size, base);

    *(uint32_t *)base = 0xCAFEBABE;
    printf("[+] wrote 0x%08X, read 0x%08X\n",
           0xCAFEBABEu, *(uint32_t *)base);

    RSC_SIZE_T zero = 0;
    RscNtFreeVirtualMemory(proc, &base, &zero, RSC_MEM_RELEASE);
    return 0;
}
```

**Сборка из "x64 Native Tools Command Prompt for VS"** (подставь свой путь к SysCalls):

```cmd
set RSC=C:\path\to\SysCalls
cl /nologo /W3 /O2 hello.c ^
    /I %RSC%\crates\rsc-c\include ^
    /link %RSC%\target\release\rsc.lib ntdll.lib kernel32.lib advapi32.lib userenv.lib ws2_32.lib
hello.exe
```

Нужный минимум системных libs — `ntdll.lib kernel32.lib advapi32.lib`. `userenv.lib` / `ws2_32.lib` добавляем если линкер жалуется на неразрешённые символы из `rsc.lib` (их тянет `ureq`/`windows-rs` зависимости `rsc-collector`, попавшие в staticlib-объединение — для MinGW можно использовать `librsc.a` где этих ссылок меньше).

### 5.3. MinGW / GCC

```cmd
set RSC=C:\path\to\SysCalls
gcc -O2 -Wall hello.c ^
    -I %RSC%\crates\rsc-c\include ^
    -L %RSC%\target\release ^
    -lrsc -lntdll -lkernel32 -ladvapi32 ^
    -o hello.exe
hello.exe
```

### 5.4. C++ обёртка с RAII

Минимальный класс `RscHandle` — auto-close, and `RscError` как исключение:

**`rsc_cxx.hpp`:**

```cpp
#pragma once
#include <stdexcept>
#include <string>
#include "rsc.h"

namespace rsc {

class Error : public std::runtime_error {
public:
    explicit Error(RSC_NTSTATUS st, const char* where)
        : std::runtime_error(format(st, where)), status_(st) {}
    RSC_NTSTATUS status() const noexcept { return status_; }
private:
    RSC_NTSTATUS status_;
    static std::string format(RSC_NTSTATUS st, const char* where) {
        char buf[128];
        std::snprintf(buf, sizeof(buf), "%s failed: 0x%08X", where, (unsigned)st);
        return buf;
    }
};

inline void check(RSC_NTSTATUS st, const char* where) {
    if (st < 0) throw Error(st, where);
}

class Handle {
public:
    Handle() = default;
    explicit Handle(RSC_HANDLE h) noexcept : h_(h) {}
    ~Handle() { reset(); }

    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
    Handle(Handle&& o) noexcept : h_(o.release()) {}
    Handle& operator=(Handle&& o) noexcept {
        if (this != &o) { reset(); h_ = o.release(); }
        return *this;
    }

    RSC_HANDLE get()  const noexcept { return h_; }
    RSC_HANDLE* ptr()       noexcept { return &h_; }
    RSC_HANDLE release()    noexcept { auto t = h_; h_ = nullptr; return t; }
    void reset() noexcept {
        if (h_ && h_ != (RSC_HANDLE)(intptr_t)-1 && h_ != (RSC_HANDLE)(intptr_t)-2) {
            RscNtClose(h_);
        }
        h_ = nullptr;
    }
private:
    RSC_HANDLE h_ = nullptr;
};

} // namespace rsc
```

**Использование (`hello.cpp`):**

```cpp
#include <cstdio>
#include "rsc_cxx.hpp"

int main() try {
    RSC_HANDLE me = (RSC_HANDLE)(intptr_t)-1;
    RSC_PVOID  base = nullptr;
    RSC_SIZE_T size = 0x1000;

    rsc::check(RscNtAllocateVirtualMemory(
        me, &base, 0, &size,
        RSC_MEM_COMMIT | RSC_MEM_RESERVE, RSC_PAGE_READWRITE),
        "NtAllocateVirtualMemory");

    std::printf("[+] RW page @ %p (%zu bytes)\n", base, (size_t)size);

    RSC_SIZE_T zero = 0;
    rsc::check(RscNtFreeVirtualMemory(me, &base, &zero, RSC_MEM_RELEASE),
               "NtFreeVirtualMemory");
    return 0;
} catch (const rsc::Error& e) {
    std::fprintf(stderr, "[!] %s (status %#010x)\n", e.what(), (unsigned)e.status());
    return 1;
}
```

Сборка:
```cmd
cl /nologo /EHsc /std:c++17 /O2 hello.cpp ^
    /I %RSC%\crates\rsc-c\include ^
    /link %RSC%\target\release\rsc.lib ntdll.lib kernel32.lib advapi32.lib
```

### 5.5. Включение вместе с `<windows.h>`

`rsc.h` написан так, что `RSC_HANDLE`, `RSC_NTSTATUS`, `RSC_PVOID`, … резолвятся **на SDK-типы** (`HANDLE`, `NTSTATUS`, `PVOID`) через `#ifdef _WINDEF_`. Порядок include имеет значение:

```c
#include <windows.h>    /* ДОЛЖНО быть до rsc.h */
#include <winsock2.h>   /* если нужен сокет — строго до windows.h */
#include "rsc.h"
```

Без `windows.h` заголовок автономен — тянет только `<stdint.h>` / `<stddef.h>`.

### 5.6. Готовая сборка-одной-командой

Для тех кто не хочет возиться с `cl.exe` вручную:

```cmd
scripts\build_c_example.bat
```

Собирает `examples/c/basic.c` против release `rsc.lib` и запускает его. Требует "x64 Native Tools Command Prompt for VS" в PATH.

---

## 6. Импорт в сторонний Rust-проект

Самый частый сценарий: есть свой проект (pentest tool, research script, injectable DLL, PoC) — хочется `use rsc_runtime::syscalls::*;` как любую другую библиотеку. Благодаря **Path A** (canonical.toml закоммичен в репо) consumer ничего не собирает и не качает — только `cargo build`.

### 6.1. TL;DR — одна строка в `Cargo.toml`

#### Git dep (рекомендуется для внешних проектов)

```toml
[dependencies]
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime" }
```

#### Path dep (если клонировал локально)

```toml
[dependencies]
rsc-runtime = { path = "D:/GitHub/Rust_Projects/SysCalls/crates/rsc-runtime" }
```

Любой вариант — **`cargo build` работает сразу**. Никаких PDB, phnt, env vars или network calls при consumer-build'е.

```rust
// src/main.rs
use rsc_runtime::constants::*;
use rsc_runtime::syscalls::*;
use rsc_runtime::types::*;

fn main() {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = 0x1000;
    let _ = unsafe {
        NtAllocateVirtualMemory(
            NT_CURRENT_PROCESS, &mut base, 0, &mut size,
            MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE,
        )
    };
    println!("got {size} bytes at {base:p}");
    // 508 остальных syscalls так же доступны
}
```

### 6.2. Готовый шаблон — копируй и используй

В репо есть полностью работающий consumer:

```
examples/consumer-template/
├── Cargo.toml        # path dep + standalone [workspace]
├── README.md         # инструкции
├── .gitignore
└── src/main.rs       # alloc / write / query / free / error path
```

**Запустить на месте:**
```bash
cd examples/consumer-template
cargo run --release
```

**Скопировать куда угодно:**
```bash
cp -r examples/consumer-template D:/MyTools/my-rsc-tool
cd D:/MyTools/my-rsc-tool
# Открыть Cargo.toml, поправить путь dep под свою клон-локацию SysCalls
#   или переделать на git dep (см. 6.1)
cargo run --release
```

Release binary — **~280 KB**.

### 6.3. Три способа подключения

#### (a) Git dep — best default для teams и CI

```toml
[dependencies]
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime" }

# Pinned version (рекомендуется для воспроизводимости):
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime", rev = "2cd5449" }

# Либо ветвь / tag:
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime", branch = "master" }
```

Cargo клонирует репо + subrepo'ы, собирает `rsc-runtime` с baked canonical.toml. **`vendor/phnt` submodule не требуется** для consumer'а — он нужен только `rsc-types` (tools-tier), а не `rsc-runtime`.

#### (b) Path dep — для локальной разработки с правками

```toml
[dependencies]
rsc-runtime = { path = "C:/Users/You/code/SysCalls/crates/rsc-runtime" }
```

Каждое изменение в `SysCalls/crates/rsc-runtime/` сразу подхватывается — удобно если ты одновременно хакаешь и сам runtime, и consumer.

#### (c) Vendored copy — для полной hermeticity

Клонируй SysCalls внутрь своего репо, добавь как submodule или просто скопируй:

```toml
[dependencies]
rsc-runtime = { path = "vendor/SysCalls/crates/rsc-runtime" }
```

Полезно если consumer хочет:
- детерминированные билды без доступа к GitHub;
- собственные `db/overrides.toml` для специфических кейсов;
- форк с замороженным API.

### 6.4. Consumer с собственным workspace

Если проект — уже Cargo workspace:

```toml
# <your-workspace>/Cargo.toml
[workspace.dependencies]
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime" }

# <your-workspace>/crates/my-tool/Cargo.toml
[dependencies]
rsc-runtime = { workspace = true }
```

Template `examples/consumer-template/` намеренно содержит пустой `[workspace]` — это делает его **standalone** (отдельный Cargo.lock, свой target/), чтобы не конфликтовать с parent SysCalls workspace при in-tree запуске. При копировании в свой workspace — **удали `[workspace]` блок** из `Cargo.toml`.

### 6.5. Gotchas

- **Windows-only**. На Linux/macOS crate пройдёт `cargo check`, но все функции будут no-op/panics — `rsc-runtime` намеренно не кросс-платформенный.
- **Multi-Windows coverage**: canonical.toml — union всех `db/auto/*.toml` в момент merge (сейчас Win10 19045 + Win11 26200 = 509 функций). Runtime резолвит SSN на **текущем** ntdll — значит функции, существующие на target OS, работают; функции, отсутствующие на target OS, дают `resolve() == None` (graceful, без crash). SSN-дрейф между Windows-версиями **автоматически** обрабатывается.
- **Build-time deps**: `rsc-runtime` тянет `rsc-codegen` (proc-macro) → `syn`/`quote`/`proc-macro2`. Это compile-time only — runtime binary остаётся ~280 KB без лишнего.
- **Target triples**: всё проверено на `x86_64-pc-windows-msvc` (native x64) и `i686-pc-windows-msvc` (WoW64). `-gnu` (MinGW) должен работать, но требует свежий GCC.
- **Расширить OS coverage**: запусти `scripts\refresh.bat` на свежей Windows VM — добавит новый `db/auto/<id>.toml`, следующий `rsc merge` склеит. Делать это нужно только **maintainer'у**; consumer'ам ничего не требуется.

### 6.6. Полный минимальный consumer

**`D:/MyProject/Cargo.toml`:**

```toml
[package]
name = "my-rsc-tool"
version = "0.1.0"
edition = "2021"
rust-version = "1.88"

[dependencies]
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime" }

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

**`D:/MyProject/src/main.rs`:**

```rust
use rsc_runtime::constants::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};
use rsc_runtime::error::STATUS_SUCCESS;
use rsc_runtime::syscalls::{NtAllocateVirtualMemory, NtFreeVirtualMemory};
use rsc_runtime::types::{NT_CURRENT_PROCESS, PVOID, SIZE_T};

fn main() {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = 0x1000;

    let st = unsafe {
        NtAllocateVirtualMemory(
            NT_CURRENT_PROCESS, &mut base, 0, &mut size,
            MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE,
        )
    };
    if st != STATUS_SUCCESS.code() {
        eprintln!("alloc failed: {:#010x}", st as u32);
        std::process::exit(1);
    }
    println!("RWX page @ {base:p} ({size} bytes)");

    // ... ваш shellcode / loader / tool logic ...

    let mut zero: SIZE_T = 0;
    unsafe {
        NtFreeVirtualMemory(NT_CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE);
    }
}
```

**Запуск:**
```bash
cd D:/MyProject
cargo run --release
```

Всё. 509 NT-функций (`NtClose`, `NtCreateFile`, `NtOpenProcess`, `NtReadVirtualMemory`, `NtWriteVirtualMemory`, `NtQuerySystemInformation`, `NtOpenProcessToken`, `NtAdjustPrivilegesToken`, `NtCreateThreadEx`, …) доступны под `rsc_runtime::syscalls::*`.

---

## 7. CLI reference — `rsc`

После `cargo build --release -p rsc-cli` появляется `target/release/rsc.exe`
(или запускайте через `cargo run --bin rsc --`).

### `rsc merge`

Склеивает три слоя БД в `db/canonical.toml`.

```
rsc merge [OPTIONS]

OPTIONS:
  --auto <PATH>        Явный auto snapshot. По умолчанию — последний в db/auto/.
  --auto-dir <PATH>    Каталог auto snapshots  [default: db/auto]
  --phnt <PATH>        [default: db/phnt/phnt.toml]
  --overrides <PATH>   [default: db/overrides.toml]
  --out <PATH>         [default: db/canonical.toml]
  -v, --verbose        Увеличить verbose
```

Пример:
```
rsc merge --auto db/auto/11_26100_2314.toml
```

### `rsc verify`

Проверки целостности canonical: дубликаты имён, коллизии hash,
SSN в диапазоне, пустые hash'и.

```
rsc verify [--canonical db/canonical.toml] [--strict]
```

`--strict` превращает warnings в ошибки (exit code 1).

### `rsc diff <FROM> <TO>`

Added / removed / changed функции между двумя build-snapshots.

```
rsc diff 10_19045_6466 11_26100_2314
```

Принимает и build-id, и явный путь `.toml`.

### `rsc stats`

Dashboard по canonical: total, coverage phnt, разбивка по категориям
(memory / process / thread / file / registry / token / sync / object /
system / other) и по источникам слоёв (auto / phnt / overrides).

```
rsc stats [--canonical db/canonical.toml]
```

---

## 8. Скрипты в `scripts/`

Все скрипты `cd` в корень репозитория — можно вызывать из любого каталога.

### Consumer workflow

| Скрипт | Что делает |
|---|---|
| `build.bat` | `cargo build --workspace --release` + показывает артефакты |
| `test.bat`  | `cargo test --workspace` |
| `clean.bat` | `cargo clean` + wipe `target/c_examples/`. **Не удаляет** `db/canonical.toml` (Path A) и `%APPDATA%\rsc\cache` (PDB кэш) |

### Maintainer DB workflow

| Скрипт | Что делает |
|---|---|
| `refresh.bat` | Полный refresh baked-DB: `rsc-collector --force` → `rsc-types` → `rsc merge` → `rsc verify` → `rsc stats`. Запускай после Windows update / bump phnt. Потом `git diff db/` + commit. |
| `merge.bat`   | Только пересобрать canonical (union всех `db/auto/*.toml` + phnt + overrides) + verify. Используй когда поменял только `overrides.toml` или `phnt.toml` — пропускает дорогой collect. |
| `stats.bat`   | Dashboard по `db/canonical.toml` — counts per category / source layer / Windows build. |

### C/C++ sanity

| Скрипт | Что делает |
|---|---|
| `build_c_example.bat` | Собрать `examples/c/basic.c` через `cl.exe` против release `rsc.lib`, запустить. Требует "x64 Native Tools Command Prompt for VS". Если `rsc.lib` отсутствует — авто-запустит `cargo build --workspace --release`. |

### Нужен single-shot без обёртки?

```cmd
cargo run --release -p rsc-collector -- --force       :: single PDB snapshot
cargo run --release -p rsc-types                       :: re-parse phnt
cargo run --release --bin rsc -- diff <from> <to>      :: сравнить два builds
cargo run --release --bin rsc -- verify                :: standalone verify
```

---

## 9. Recipes — частые сценарии

### 8.1. Выделить executable shellcode page и залить байты

```rust
use rsc_runtime::constants::*;
use rsc_runtime::syscalls::{
    NtAllocateVirtualMemory, NtProtectVirtualMemory,
};
use rsc_runtime::types::*;

unsafe fn map_shellcode(bytes: &[u8]) -> PVOID {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = bytes.len();
    let status = NtAllocateVirtualMemory(
        NT_CURRENT_PROCESS, &mut base, 0, &mut size,
        MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE,
    );
    assert_eq!(status, 0);

    // Копируем байты в RW страницу.
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), base as *mut u8, bytes.len());

    // Переводим в RX.
    let mut old = 0u32;
    let mut pbase = base;
    let mut psize = size;
    NtProtectVirtualMemory(
        NT_CURRENT_PROCESS, &mut pbase, &mut psize,
        PAGE_EXECUTE_READ, &mut old,
    );
    base
}
```

### 8.2. Запросить OS version через `NtQuerySystemInformation`

```rust
#[repr(C)]
struct RtlOsVersionInfoEx {
    dw_os_version_info_size: u32,
    dw_major: u32, dw_minor: u32, dw_build: u32,
    dw_platform: u32,
    sz_csd_version: [u16; 128],
    w_service_pack_major: u16, w_service_pack_minor: u16,
    w_suite_mask: u16,
    w_product_type: u8, w_reserved: u8,
}

unsafe fn get_os_version() -> RtlOsVersionInfoEx {
    let mut info: RtlOsVersionInfoEx = core::mem::zeroed();
    info.dw_os_version_info_size = core::mem::size_of::<RtlOsVersionInfoEx>() as u32;
    // SystemBasicInformation = 0; … в реальности ищите правильный class в phnt.
    let mut ret_len = 0usize;
    rsc_runtime::syscalls::NtQuerySystemInformation(
        0 /* class */, 
        &mut info as *mut _ as *mut _,
        core::mem::size_of::<RtlOsVersionInfoEx>() as u32,
        &mut ret_len as *mut _ as *mut _,
    );
    info
}
```

### 8.3. Открыть hProcess по PID

```rust
use rsc_runtime::constants::PROCESS_ALL_ACCESS;
use rsc_runtime::syscalls::NtOpenProcess;
use rsc_runtime::types::*;

#[repr(C)]
struct ClientId {
    unique_process: *mut core::ffi::c_void,
    unique_thread:  *mut core::ffi::c_void,
}

unsafe fn open_process(pid: usize) -> HANDLE {
    let mut h: HANDLE = core::ptr::null_mut();
    let mut obj_attr: [u8; 48] = core::mem::zeroed();
    // OBJECT_ATTRIBUTES.Length = 48 on x64 (24 on x86)
    *(obj_attr.as_mut_ptr() as *mut u32) = 48;

    let cid = ClientId {
        unique_process: pid as *mut _,
        unique_thread:  core::ptr::null_mut(),
    };

    NtOpenProcess(
        &mut h,
        PROCESS_ALL_ACCESS,
        obj_attr.as_mut_ptr() as *mut _,
        &cid as *const _ as *mut _,
    );
    h
}
```

### 8.4. Посмотреть что резолвится на текущей системе

```
cargo run --example resolver_demo -p rsc-runtime
```

---

## 10. Overrides — ручные правки БД

Файл `db/overrides.toml` — единственный редактируемый руками. Использует
структуру с `kind`-полем. Примеры:

### Исправить сигнатуру (phnt привирает)

```toml
[[override]]
kind = "fix_signature"
name = "NtSomeFunction"
return_type = "NTSTATUS"
params = [
    { name = "Handle", type = "HANDLE", direction = "in" },
    { name = "Size",   type = "usize",  direction = "in" },
]
reason = "phnt говорит ULONG, на Win11 24H2 реально ULONG_PTR"
verified_on = ["11_26100_2314"]
```

### Исключить функцию из сборки

```toml
[[override]]
kind = "exclude"
name = "NtDeprecatedCall"
reason = "Не используется; уменьшаем размер бинарника"
```

### Добавить свою (кастомный syscall)

```toml
[[override]]
kind = "add"
name = "NtMyCustomCall"
return_type = "NTSTATUS"
ssn_x64 = 0x1F7
ssn_x86 = 0x1F7
arity_x86 = 3
params = [
    { name = "Arg1", type = "HANDLE",       direction = "in" },
    { name = "Arg2", type = "*mut c_void",  direction = "in" },
    { name = "Arg3", type = "u32",          direction = "in" },
]
reason = "Undocumented syscall via RE, см. notes"
```

### Пофиксить arity

```toml
[[override]]
kind = "fix_arity"
name = "NtSomeOther"
arity_x64 = 7
arity_x86 = 7
reason = "Collector heuristic вернул 6, реально 7"
```

После правки: `rsc merge` + `cargo build`.

---

## 11. Env vars

`rsc-runtime` в v1.0 **не имеет cargo-features** — всё включено по умолчанию:
JUMPER через случайный slide в ntdll, essential NTSTATUS-имена,
стабильный seed `RSC_SEED = 0x52534300`. Исторические флаги
(`status-names-full`, `debug-breakpoints`, `random-seed`, `no-jumper`)
удалены в v1.0 cleanup — они никогда не были проводены в код.

### Env vars (tools, не runtime)

| Переменная | Кто читает | Что влияет |
|---|---|---|
| `RSC_CACHE_DIR`      | `rsc-collector` | Override `%APPDATA%\rsc\cache` (PDB кэш) |
| `RSC_CANONICAL_PATH` | `rsc-runtime/build.rs` | Где искать `canonical.toml` (CI-сценарий) |
| `RSC_LOG`            | все CLI-бинари | Фильтр `tracing` (`trace`, `debug`, `info`, …) |

---

## 12. Troubleshooting

### "Symbol Server HTTP 404"

Проверь что GUID в URL формируется правильно. Запусти с `-vv`:
```
cargo run -p rsc-collector -- --force -vv
```

Скорее всего Symbol Server ещё не опубликовал PDB для свежей Windows
Insider build'ы — подожди несколько часов после выхода.

### `canonical.toml not found at …`

Этот кейс теоретически невозможен при работе через клонированный репо
(canonical committed, Path A). Если всё-таки случилось — кто-то руками
удалил файл. Восстановить:
```
git checkout db/canonical.toml        # из последнего коммита
```
или пересобрать с нуля:
```
scripts\refresh.bat
```

### `expected ','` в сгенерированном `syscalls_generated.rs`

Это означает `canonical.toml` содержит C-keyword (`volatile`, `register`)
который не успели отфильтровать. Запусти заново:
```
cargo run -p rsc-types        # обновить phnt.toml с актуальным normalizer
cargo run --bin rsc -- merge  # перегенерировать canonical
```

### "NtFoo returned STATUS_INVALID_PARAMETER_1"

Обычно = неправильный тип первого параметра. Посмотри canonical:
```
rsc stats
grep -A 20 '"NtFoo"' db/canonical.toml
```

Если `opaque_signature = true` — функции нет в phnt, сигнатура из
`*mut c_void`-заглушек. Добавь `fix_signature` в overrides с реальными типами.

### Stack corruption / random crashes на x86

Проверь что arity для функции совпадает с реальной. Collector извлекает
arity из WoW64 stub'а (`ret N`), но для некоторых функций ntdll может
иметь нестандартный stub. `rsc stats` покажет opaque count — если ≠ 0,
проверь вручную.

### `integration test: alloc failed: 0xC00000EF`

`0xC00000EF` = `STATUS_INVALID_PARAMETER_1`. Было в истории: тип
`MEMORY_INFORMATION_CLASS` резолвился как `*mut c_void` вместо `u32`
(enum class). Нормализатор это уже исправил — проверь что `phnt.toml`
свежий.

### Live-test падает на x86 / i686

Проверь что fs:[0xC0] не 0 (это значит native x86 без WoW64). Наш
stub на этом пути возвращает `STATUS_NOT_IMPLEMENTED` (0xC0000002)
без вызова kernel. Поддерживается только WoW64 (32-bit процесс на
64-bit Windows — стандартный сценарий).

---

## 13. Для нового Windows update / мульти-Windows coverage

### Maintainer на одной машине — новый патч Windows

```bash
scripts\refresh.bat             # collect → phnt → merge → verify → stats
git diff db/                    # посмотреть что поменялось
git commit -am "refresh canonical for Win10 19045.6467"
```

`refresh.bat` делает union всех `db/auto/*.toml`, так что **старый
snapshot остаётся** + **новый добавляется**. Canonical получает
максимальное покрытие обоих builds.

### Maintainer с несколькими Windows (best coverage)

Собери snapshots на всех целевых Windows-системах (VM, dual-boot, test
machines):

```bash
# На Win10 22H2:
scripts\refresh.bat
# → db/auto/10_19045_6466.toml

# На Win11 24H2:
scripts\refresh.bat
# → db/auto/11_26100_2314.toml

# Merge автоматически делает union:
cargo run --bin rsc -- stats
# ## By Windows build (union across db/auto/*.toml)
#   10_19045_6466          492  (0 exclusive)
#   11_26100_2314          520  (28 exclusive)
```

Consumer получает **compile-time API с union'ом**: если код использует
Win11-only функцию но работает на Win10 — `resolve()` вернёт None,
можно handle'нуть (`STATUS_NOT_IMPLEMENTED` or fallback).

### Diff между builds

```bash
cargo run --bin rsc -- diff 10_19045_6466 11_26100_2314
```

Показывает `Added`/`Removed`/`Changed` функции — полезно для release
notes или понимания что ломается на upgrade.

### Обновить phnt submodule

```bash
cd vendor/phnt
git pull && git checkout <new-commit>
cd ../..
scripts\refresh.bat              # regen phnt + canonical
git diff vendor/phnt db/ && git commit
```

### CI-friendly

```yaml
# Fresh clone, canonical already baked:
- run: cargo test --workspace
- run: cargo clippy --workspace --all-targets -- -D warnings
- run: cargo build --workspace --release
```

**Без** `refresh.bat` в CI — consumer'ам не нужна сеть. `refresh` —
это maintainer workflow, запускается manually когда нужно перебакать.

---

## Связанные файлы

- `README.md` — обзор верхнего уровня
- `Docs/ARCHITECTURE.md` — как всё устроено внутри
- `Docs/API.md` — детальная спецификация публичного API
- `Docs/DATABASE.md` — схема TOML-БД
- maintainer conventions doc — правила кода и naming
- maintainer skills doc — low-level нюансы (ASM, WoW64 gate, etc.)
- `scripts/README.md` — краткий обзор батников
