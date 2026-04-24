# ROADMAP — SysCalls (RSC)

> **Это главный документ прогресса.** AI-ассистент ОБЯЗАН обновлять метки `[ ]` → `[x]` в момент реального завершения задач. Фазы строго последовательны — не начинай N+1 пока N не закрыта (за исключением параллельно-допустимых отмеченных 🟡).

## Легенда меток

- `[ ]` — задача не начата
- `[~]` — в работе
- `[x]` — завершено
- `[-]` — отменено / неактуально
- 🔴 — блокирующая задача (без неё следующая фаза невозможна)
- 🟡 — параллельно-допустимая (можно делать одновременно с другой)
- 🟢 — опциональная / nice-to-have

---

## Phase 0 — Scaffolding (каркас проекта) ✅

**Цель**: рабочий Cargo workspace с пустыми крейтами, которые собираются.
**Выход**: `cargo build --workspace` → OK.

- [x] 🔴 Создать `Cargo.toml` (workspace root) с `[workspace] members = [...]`
- [x] 🔴 Создать `.gitignore` (target/, db/canonical.toml, cache/, *.pdb, *.cab, *.exe, *.dll, *.lib, *.a, *.pdb)
- [x] 🔴 Инициализировать каталоги всех 6 крейтов: `crates/{rsc-runtime,rsc-codegen,rsc-collector,rsc-types,rsc-cli,rsc-c}`
- [x] 🔴 Минимальный `Cargo.toml` + `src/lib.rs` / `src/main.rs` в каждом крейте (hello-world уровня)
- [x] 🔴 `cargo build --workspace` → успешная сборка (dev 0.72s, release 1.49s)
- [x] 🔴 Создать каталоги `db/{auto,phnt}`, `vendor/`, `examples/`, `tests/`
- [x] 🟡 Создать `README.md` (короткий, ссылка на Docs/)
- [ ] 🟡 Добавить phnt submodule: `git submodule add <phnt-repo-url> vendor/phnt` (pinned commit) — **отложено**, см. `vendor/README.md` (требует network + решение pin-commit; не блокирует дальнейшие фазы, нужен только в Phase 4)
- [x] 🟡 Заполнить `Docs/modules/*.md` — stubs написаны (сделано в pre-0)
- [x] 🔴 Обновить `Docs/CURRENT_STATE.md`: Phase 0 = done
- [x] 🔴 Обновить dev log: запись о завершении Phase 0

**Критерии завершения Phase 0**:
- [x] `cargo build --workspace` успешен на чистом клоне
- [x] `cargo check --workspace --target i686-pc-windows-msvc` успешен
- [x] `cargo clippy --workspace --all-targets -- -D warnings` зелёный
- [x] Все каталоги из `ARCHITECTURE.md` §4 существуют
- [ ] `git status` чистый (кроме в gitignore) — **после первого commit** (не блокирует Phase 1)

---

## Phase 1 — Runtime foundation (rsc-runtime без syscall'ов) ✅

**Цель**: базовая инфраструктура runtime — PEB, hash, table — но без самих syscall функций.
**Выход**: `cargo test -p rsc-runtime` проходит на unit-level.
**Факт**: 21/21 зелёные, включая live-тесты на реальном ntdll.

### 1.1 Типы и константы
- [x] 🔴 `src/types.rs` — основные типы: `HANDLE`, `PVOID`, `NTSTATUS`, `SIZE_T`, `ULONG_PTR`, `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`, `CLIENT_ID`, `MEMORY_BASIC_INFORMATION`
- [x] 🔴 `src/error.rs` — `NtStatus`, `RscResult<T>`, `NtStatusExt trait` + severity decoding (4 severity класса)
- [x] 🟡 `src/constants.rs` — `STATUS_*`, `PAGE_*`, `MEM_*`, `PROCESS_*`, `THREAD_*`, `OBJ_*`

### 1.2 Hash
- [x] 🔴 `src/hash.rs` — `const fn rsc_hash(name: &[u8]) -> u32`
  - ROR8 WORD-overlapping + `RSC_SEED = 0x52534300` (ASCII "RSC\0")
- [x] 🔴 Regression тесты: 50+ имён NT-функций без коллизий, seed pinned, compile-time evaluation

### 1.3 PEB parsing
- [x] 🔴 `src/peb.rs`:
  - [x] `unsafe fn get_peb() -> *mut Peb` — `gs:[0x60]` (x64) / `fs:[0x30]` (x86) via `core::arch::asm!`
  - [x] `unsafe fn find_ntdll() -> Option<*mut u8>` — walk InLoadOrderModuleList, case-insensitive UTF-16 match, bounded ≤ 1024
- [x] 🟡 `src/pe.rs` — `ImageDosHeader`, `ImageNtHeaders` (x64 + x86 ветви через `#[cfg]`), `ImageExportDirectory`, `find_export_directory()` с magic-checks

### 1.4 Syscall table
- [x] 🔴 `src/table.rs`:
  - [x] `struct SyscallEntry { hash: u32, ssn: u32, fn_addr: usize, syscall_addr: usize }` — layout pinned (24B x64, 16B x86)
  - [x] `static TABLE: Lazy { cell: UnsafeCell<...>, initialized: AtomicBool }`
  - [x] `AtomicBool INITIALIZED` — Acquire/Release publish pattern (D-08)
  - [x] `unsafe fn populate_impl()` — filter `Zw*`, hash as `Nt*`, sort by `fn_addr` (insertion sort, O(n²) приемлемо для ~500 entries)
  - [x] `pub fn resolve(hash) -> Option<(u32, usize)>` — linear search, lazy init
  - [x] `pub fn pick_random_slide() -> usize` + `pub fn count() -> u32`
- [x] 🔴 `src/jumper.rs` — `unsafe fn find_syscall_slide()` (HalosGate ±512 × 0x20) + `next_random()` (xorshift32 seeded от rdtsc)

### 1.5 ASM шаблоны (без вызова конкретных функций)
- [x] 🔴 `src/asm_x64.rs` — documented template (x64 Microsoft ABI, syscall ABI, JUMPER)
- [x] 🔴 `src/asm_x86.rs` — documented template + WoW64 gate (dummy ret, param staging)

### 1.6 Проверка
- [x] 🔴 `cargo build -p rsc-runtime` OK
- [x] 🔴 `cargo test -p rsc-runtime` — **21 тест зелёных**
- [x] 🔴 `cargo clippy --workspace --all-targets -- -D warnings` — 0 warnings
- [x] 🔴 `cargo check --workspace --target i686-pc-windows-msvc` — OK
- [x] 🔴 Live-тесты (`live_resolve_nt_close`, `live_count_reasonable`) — реальный ntdll текущего процесса резолвится, находятся ≥ 100 функций
- [x] 🔴 Обновить `Docs/CURRENT_STATE.md`, dev log, `Docs/modules/rsc_runtime.md`

### Статистика
- **LoC:** 1748 строк Rust в `crates/rsc-runtime/src/` (11 файлов)
- **Тесты:** 21/21 зелёные (6 hash + 9 error + 3 jumper + 4 table + 2 live on real ntdll)
- **Размеры release cdylib/staticlib:** скелет, эффективные замеры — после Phase 6

---

## Phase 2 — Codegen (proc-macro) ✅

**Цель**: `rsc_syscall!(fn NtName(...) -> NTSTATUS)` разворачивается в рабочий naked стаб.
**Выход**: `cargo run --example one_syscall` — **live alloc/write/read/free через три разных syscall**.

### 2.1 Макрос
- [x] 🔴 `crates/rsc-codegen/Cargo.toml` с `proc-macro = true` — уже в Phase 0
- [x] 🔴 `syn` 2.x + `quote` + `proc-macro2` + `proc-macro-error2` активированы
- [x] 🔴 Парсер входа макроса через `syn::ForeignItemFn`: `fn Name(args...) -> Ret;`
- [x] 🔴 Compile-time hash — `const ::rsc_runtime::rsc_hash(#name_bytes)` прямо в `naked_asm!` operand
- [x] 🔴 Генерация x64 варианта: `#[cfg(target_arch = "x86_64")] #[unsafe(naked)]`
- [x] 🔴 Генерация x86 placeholder: `#[cfg(target_arch = "x86")]` возвращает `STATUS_NOT_IMPLEMENTED` (0xC0000002)
- [x] 🟡 `reject_unsupported()` — `abort!` с понятными spans для `async`, `unsafe`, custom ABI, generics, variadic

### 2.2 ASM внутри макроса
- [x] 🔴 x64 шаблон:
  - `push rcx/rdx/r8/r9` + `sub rsp, 0x28` (shadow + 16-align)
  - Two calls: `__rsc_resolve_slide` (→ r11), `__rsc_resolve_ssn` (→ eax)
  - Restore: `add rsp, 0x28` + `pop r9/r8/rdx/rcx`
  - Syscall ABI: `mov r10, rcx` + `jmp r11`
  - Rationale split: `extern "system" fn -> 16-byte struct` на x64 Rust ABI использует hidden pointer (не RAX:RDX). Два отдельных integer-returning функции дают предсказуемый `RAX` / `EAX`.
- [-] 🟡 x86/WoW64 шаблон — **отложен на Phase 2b** (placeholder возвращает STATUS_NOT_IMPLEMENTED, x86 build компилируется)

### 2.3 Ручная проверка макроса
- [x] 🔴 `crates/rsc-runtime/examples/one_syscall.rs`:
  - `rsc_syscall!` для `NtClose`, `NtAllocateVirtualMemory`, `NtFreeVirtualMemory`
  - Диагностика: count() + hash/ssn/slide для каждой
  - `NtClose(0xDEADBEEF)` → `0xC0000008` (STATUS_INVALID_HANDLE) ✅
  - `NtAllocateVirtualMemory(4096, MEM_COMMIT|RESERVE, PAGE_RW)` → успех ✅
  - Write/read pattern 0xDEADBEEF → верифицирован ✅
  - `NtFreeVirtualMemory` → успех ✅
- [x] 🔴 `cargo run --example one_syscall -p rsc-runtime` на x86_64 Windows — **работает в debug и release**
- [-] 🟡 i686 / WoW64 — **отложен на Phase 2b** (placeholder вернёт NOT_IMPLEMENTED, macro emit на x86 компилируется)
- [x] 🔴 Тест с 6-параметрами (NtAllocateVirtualMemory) — stack args 5/6 кооректно прокидываются kernel'у

### 2.4 Проверка
- [x] 🔴 `cargo test -p rsc-runtime` — **22 passed**, включая новый `split_resolvers_match_public_resolve`
- [x] 🔴 `cargo build --workspace --release` — OK
- [x] 🔴 `cargo clippy --workspace --all-targets -- -D warnings` — 0 warnings
- [x] 🔴 `cargo check --workspace --target i686-pc-windows-msvc` — OK (macro emit placeholder для x86)
- [x] 🔴 Обновить `Docs/modules/rsc_codegen.md`
- [x] 🔴 Обновить dev log, `CURRENT_STATE.md`

### Что НЕ входит в Phase 2a (отложено на Phase 2b / будущее)

- x86 native + WoW64 ASM stub (сейчас — `STATUS_NOT_IMPLEMENTED` placeholder)
- `trybuild` тесты для fail cases (spec тесты proc-macro ошибок)
- Feature `debug-breakpoints` (int3 в stub)
- Feature `no-jumper` (прямой syscall из стаба)
- `pick_random_slide` в качестве источника slide per-call (сейчас — deterministic через resolve)

### Статистика Phase 2

- **LoC delta:** +188 строк в rsc-codegen + ~50 строк в table.rs + 134 строк example = +372 строки
- **Release build artifacts:** example skeleton, stubs встроены как naked функции
- **Live verified на реальной Win11**: 473 syscall entries собрано, 3 ручных стаба работают

---

## Phase 3 — PDB Collector ✅

**Цель**: `cargo run -p rsc-collector` собирает `db/auto/<build>.toml` для текущей Windows.
**Выход**: `db/auto/10_19045_6466.toml` (110 KB) с **492 функциями** из обоих ntdll.

### 3.1 Version detection + UBR
- [x] 🔴 `src/version.rs`:
  - Registry `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` → Major/Minor/Build/UBR
  - Формат ID: `{Major}_{Build}_{UBR}`, label `"10 (build 19045.6466)"`

### 3.2 Symbol server client
- [x] 🔴 `src/pe.rs` — parse ntdll PE manually (no dep):
  - DOS + NT + OptionalHeader (x86/x64 auto-detect via magic 0x10B/0x20B)
  - DataDirectory[DEBUG] → IMAGE_DEBUG_DIRECTORY entries
  - CodeView RSDS: GUID + Age + PDB filename
  - RVA → file offset через section table
  - SHA-256 целого файла для meta
- [x] 🔴 `src/symsrv.rs`:
  - [x] URL: `https://msdl.microsoft.com/download/symbols/{pdb_name}/{GUID}{AGE}/{pdb_name}`
  - [x] Cache: `%APPDATA%\rsc\cache\{pdb_name}\{GUID}{AGE}\{pdb_name}` (override via RSC_CACHE_DIR)
  - [x] `ureq` 3.x GET с User-Agent `Microsoft-Symbol-Server/10.0.0.0`, 5-min timeout
  - [x] CAB detection (magic `MSCF`) → `expand.exe -F:*`
  - [x] Atomic write (`.tmp` → rename)
  - [x] `std::sync::LazyLock` для cache root
  - [x] **Ключевой фикс: GUID в URL в "network byte order"** (Data1/2/3 reversed от file LE)

### 3.3 PDB reader
- [x] 🔴 **OQ-2 решён**: DbgHelp FFI primary (через `windows` crate 0.62)
- [x] 🔴 `src/pdb_reader.rs`:
  - `SymSetOptions(CASE_INSENSITIVE | UNDNAME | DEBUG | FAIL_CRITICAL_ERRORS)`
  - `SymInitializeW` + `SymLoadModuleExW` (wide-char API)
  - `SymEnumSymbolsW("Zw*")` с callback + `thread_local!` sink pattern
  - RAII `PdbSession` с `Drop` → `SymCleanup`
  - Trim trailing `\0` в symbol names (DbgHelp inconsistency)

### 3.4 Stub disassembly
- [x] 🔴 `src/stub_disasm.rs`:
  - x86 ntdll: `B8 xx xx xx xx ... C2 YY 00` → SSN + arity = YY/4
  - x64 ntdll: `4C 8B D1 B8 xx xx xx xx ...` → SSN; arity = None (Phase 4 phnt overlay)
  - Fallback `search_ssn_by_mov_eax` в первых 16 байтах (для hooked / forwarder prologue)
  - 4 unit-теста: canonical x64, canonical x86, 6-arg x86, hex upper
- [x] 🟡 Обработка захученных (CFG/hot-patch prefix `E9` → fallback scan) — работает

### 3.5 Два ntdll (native + WoW64)
- [x] 🔴 Native: `C:\Windows\System32\ntdll.dll` (x64 на 64-bit Windows)
- [x] 🔴 WoW64: `C:\Windows\SysWOW64\ntdll.dll` — PDB имя `wntdll.pdb` (не `ntdll.pdb`!)
- [x] 🔴 Оба собираются за один run, upsert в общий snapshot с полями `ssn_x64` / `ssn_x86`

### 3.6 Emit TOML
- [x] 🔴 `src/emit.rs` — `AutoSnapshot` + `NtdllMeta` + `SyscallEntry` serialize per DATABASE.md
- [x] 🔴 `Option` поля skip'ятся при serialize (чистый TOML)
- [x] 🔴 Atomic write `db/auto/<build>.toml`
- [x] 🔴 Entries сортируются по `name` (diff-friendly)
- [x] 🔴 2 unit-теста: upsert merging + skip_serializing_if_none

### 3.7 CLI & UX
- [x] 🔴 `main.rs` + clap derive: `--force`, `--arch x64|x86|both`, `--build-id`, `--db-dir`, `-v/-vv/-vvv`
- [x] 🔴 Idempotent: skip если файл есть (без `--force`)
- [x] 🔴 `tracing-subscriber` logging, `RSC_LOG` env override
- [x] 🔴 `CollectError` enum + `core::error::Error` + `anyhow::Context` на границах
- [x] 🔴 Прогресс-вывод: download URL, PDB ready, symbols enumerated count, snapshot written

### 3.8 Проверка
- [x] 🔴 Запуск на Win10 22H2 (build 19045.6466): файл создан, **473 x64 + 492 x86 WoW64** entries
- [x] 🔴 SSN совпадают с legacy / известными values (NtAllocateVirtualMemory=24, NtClose=15)
- [x] 🔴 Второй запуск без флагов → skip с WARN
- [x] 🔴 `--force` → регенерация
- [x] 🔴 `cargo test -p rsc-collector` — **9/9 passed**
- [x] 🔴 `cargo test --workspace` — **31/31 passed** (22 runtime + 9 collector)
- [x] 🔴 `cargo clippy --workspace --all-targets -- -D warnings` — 0 warnings
- [x] 🔴 `cargo build --workspace --release` — OK
- [x] 🔴 `cargo check --workspace --target i686-pc-windows-msvc` — OK
- [x] 🔴 Обновить `Docs/modules/rsc_collector.md`, dev log, `CURRENT_STATE.md`

### Статистика Phase 3

- **LoC**: 1454 строк Rust в `rsc-collector` (8 модулей)
- **Производительность**: на Win10 22H2 полный cycle (оба ntdll + Symbol Server + DbgHelp + TOML) < 10 сек
- **PDB cache (первый run)**: `%APPDATA%\rsc\cache\ntdll.pdb\...\ntdll.pdb` (1.5-2 MB) + `wntdll.pdb\...\wntdll.pdb`
- **Output**: 110 KB TOML, 492 entries, отсортированы по имени
- **Modern Rust 1.95 used**: `LazyLock`, `let ... else`, `without_provenance_mut`, `is_multiple_of`, `core::error::Error`

---

## Phase 4 — phnt parser (rsc-types) ✅

**Цель**: `cargo run -p rsc-types` парсит vendor/phnt → `db/phnt/phnt.toml`.

### 4.1 Vendor phnt
- [ ] 🔴 Добавить git submodule: `git submodule add https://github.com/winsiderss/phnt vendor/phnt`
- [ ] 🔴 Pinned commit hash зафиксирован в `.gitmodules` или `Cargo.lock`-подобном файле

### 4.2 Parser
- [ ] 🔴 `src/parser.rs`:
  - Regex для `NTSYSCALLAPI NTSTATUS NTAPI NtFunctionName(...)`
  - Извлечение параметров с типами и направлением (`_In_`, `_Out_`, `_Inout_`)
  - Обработка multi-line signatures
- [ ] 🟡 Обработка версионных макросов (`#if (PHNT_VERSION >= ...)`)

### 4.3 Normalizer
- [ ] 🔴 `src/normalizer.rs`:
  - phnt types → canonical имена: `PHANDLE` → `*mut HANDLE`, `PVOID` → `*mut c_void`, etc.
  - Таблица перекодирования (hardcoded, editable)

### 4.4 Emit
- [ ] 🔴 Emit `db/phnt/phnt.toml` (схема в `DATABASE.md`)

### 4.5 Проверка
- [ ] 🔴 Парсер извлекает ≥ 500 функций из phnt
- [ ] 🔴 Сверка: каждая функция из `auto.toml` имеет match в `phnt.toml` (отчёт несовпадений)
- [ ] 🔴 Обновить `Docs/modules/rsc_types.md`

---

## Phase 5 — Merge & CLI ✅

**Цель**: `rsc merge` производит `canonical.toml`, `rsc verify` ловит дрейф.

### 5.1 Merger
- [ ] 🔴 `crates/rsc-cli/src/commands/merge.rs`:
  - Загрузить все `db/auto/*.toml`
  - Выбрать baseline (последний / текущий / указанный) — default: latest
  - Наложить `phnt.toml` (типы)
  - Наложить `overrides.toml`
  - Emit `db/canonical.toml`
- [ ] 🔴 Правила приоритета строго документированы в `DATABASE.md`
- [ ] 🔴 Логи: warning если функция в auto, но не в phnt (и не в overrides)

### 5.2 Verify
- [ ] 🔴 `commands/verify.rs`:
  - [ ] Проверить что phnt submodule на зафиксированном commit
  - [ ] Проверить что все функции из auto имеют тип в phnt или override
  - [ ] Проверить что overrides не ссылаются на несуществующие функции
  - [ ] `--strict` → exit 1 при любом warning

### 5.3 Diff
- [ ] 🔴 `commands/diff.rs`:
  - `rsc diff 10_19045_6466 11_26100_2314` → добавленные/удалённые/изменённые функции
- [ ] 🟡 Markdown-форматированный вывод для release notes

### 5.4 Stats
- [ ] 🔴 `commands/stats.rs`:
  - Количество функций в canonical
  - Разбивка по категориям (Memory, Process, ...)
  - % покрытия типами из phnt

### 5.5 Collect command (wrapper)
- [ ] 🔴 `commands/collect.rs` — делегирует `rsc-collector`

### 5.6 Проверка
- [ ] 🔴 `rsc merge` → canonical.toml создан без ошибок
- [ ] 🔴 `rsc verify` → OK на чистом репо
- [ ] 🔴 Обновить `Docs/modules/rsc_cli.md`

---

## Phase 6 — Wire it all: rsc-runtime reads canonical ✅

**Цель**: `rsc-runtime/build.rs` читает `canonical.toml` и эмитит все ~500 `rsc_syscall!(...)` вызовов.
**Выход**: полноценная библиотека с всеми NT функциями, `cargo test -p rsc-runtime` все тесты зелёные.

### 6.1 build.rs
- [ ] 🔴 Прочитать `../../db/canonical.toml` (path via env var + default)
- [ ] 🔴 Для каждой функции emit `rsc_syscall!(Name, arity, (types) -> ret);`
- [ ] 🔴 Записать в `$OUT_DIR/syscalls_generated.rs`
- [ ] 🔴 В `src/syscalls.rs`: `include!(concat!(env!("OUT_DIR"), "/syscalls_generated.rs"));`
- [ ] 🔴 `println!("cargo:rerun-if-changed=../../db/canonical.toml");`

### 6.2 Полный набор
- [ ] 🔴 `cargo build -p rsc-runtime --target x86_64-pc-windows-msvc` — OK
- [ ] 🔴 `cargo build -p rsc-runtime --target i686-pc-windows-msvc` — OK
- [ ] 🔴 `cargo build -p rsc-runtime --target x86_64-pc-windows-gnu` — OK
- [ ] 🔴 `cargo build -p rsc-runtime --target i686-pc-windows-gnu` — OK

### 6.3 Портируем legacy тест
- [ ] 🔴 `examples/legacy_smoke.rs` — аналог `test_syscalls.rs` из старого проекта: alloc + write + read + protect + query + free + sleep
- [ ] 🔴 Запуск x64 native → успех
- [ ] 🔴 Запуск x86 (WoW64) → успех

### 6.4 Проверка
- [ ] 🔴 Обновить `CURRENT_STATE.md`, dev log

---

## Phase 7 — C bindings (rsc-c) ✅

**Цель**: `rsc.h`, `rsc.dll`, `rsc.lib` собраны, C-тест работает.

### 7.1 Порт build.rs
- [ ] 🔴 Скопировать `c-bindings/build.rs` в `crates/rsc-c/build.rs`
- [ ] 🔴 Адаптировать regex под новый `src/syscalls.rs` (macro-based, не именованные функции в lib.rs!)
  - Альтернатива: читать `canonical.toml` напрямую (чище) — **предпочтительно**
- [ ] 🔴 Prefix `SW3_` → `RSC_` везде
- [ ] 🔴 Generate `include/rsc.h`

### 7.2 Крейт
- [ ] 🔴 `[lib] crate-type = ["staticlib", "cdylib"]`
- [ ] 🔴 `src/lib.rs`: re-export `rsc-runtime` + `include!(OUT_DIR/c_wrappers.rs)`

### 7.3 C tests
- [ ] 🔴 Минимальный `examples/c_tests/basic.c` — alloc + free через RSC_NtAllocateVirtualMemory
- [ ] 🔴 Build script для x64 и x86 MSVC
- [ ] 🔴 Запуск → успех

### 7.4 Артефакты
- [ ] 🔴 `rsc.dll`, `rsc.lib` в `crates/rsc-c/lib/` (или target/release)
- [ ] 🟡 MinGW вариант `rsc_mingw.a`

### 7.5 Проверка
- [ ] 🔴 Обновить `Docs/modules/rsc_c.md`
- [ ] 🔴 Обновить `API.md` — публичный C API

---

## Phase 8 — Quality & Testing ✅

**Цель**: надёжная suite тестов + CI.

### 8.1 Unit tests
- [ ] 🔴 `rsc-runtime`: hash, peb, table, jumper — unit tests
- [ ] 🔴 `rsc-codegen`: expansion verification (trybuild)
- [ ] 🔴 `rsc-collector`: mock PDB, stub disasm — unit tests
- [ ] 🔴 `rsc-types`: parser fixtures
- [ ] 🔴 `rsc-cli`: merge logic

### 8.2 Integration tests
- [ ] 🔴 `tests/integration.rs`: alloc + write + read (живой syscall)
- [ ] 🔴 `tests/runtime_smoke.rs`: вызов 20+ разных функций из всех категорий
- [ ] 🔴 `tests/drift.rs`: перепарсить текущую ntdll, сверить с последним `auto.toml`

### 8.3 Multi-version validation
- [ ] 🟢 Запустить collector на 3+ Windows build (VM / Docker / snapshot), убедиться что canonical генерируется корректно
- [ ] 🟢 Committed PDB snapshots в `vendor/pdb-snapshots/` для CI reproducibility

### 8.4 CI
- [ ] 🟡 GitHub Actions workflow:
  - build x64 MSVC + GNU
  - build x86 MSVC + GNU
  - test --workspace
  - rsc verify --strict (дрейф-чек на committed canonical)

### 8.5 Проверка
- [ ] 🔴 `cargo test --workspace` — зелёные
- [ ] 🔴 `rsc verify --strict` — OK
- [ ] 🔴 Обновить `CURRENT_STATE.md`: Phase 8 done

---

## Phase 9 — Documentation polish & release ✅

**Цель**: готовый к использованию публичный проект.

### 9.1 Docs
- [ ] 🔴 Все файлы `Docs/modules/*.md` полностью заполнены
- [ ] 🔴 `README.md` с примерами использования (Rust + C)
- [ ] 🟡 `CHANGELOG.md` версионированный
- [ ] 🟢 Миграция с SW3 → RSC (`Docs/MIGRATION.md`)

### 9.2 rustdoc
- [ ] 🟡 Inline doc comments на публичные функции runtime
- [ ] 🟡 `cargo doc --workspace --no-deps` собирается

### 9.3 Examples
- [ ] 🔴 `examples/basic_syscall.rs` — простейший пример
- [ ] 🔴 `examples/memory_alloc.rs` — полный alloc/free цикл
- [ ] 🔴 `examples/process_enum.rs` — перечисление процессов
- [ ] 🟡 Ещё 3-5 реалистичных сценариев

### 9.4 Release
- [ ] 🟢 Semver bump → v0.1.0
- [ ] 🟢 Optional: публикация на crates.io (при желании автора)
- [ ] 🔴 Обновить `CURRENT_STATE.md`: проект готов

---

## Phase 10 — Maintenance mode (ongoing)

**Цель**: устойчивая модель обновлений.

- [x] 🟡 **Path A bake + Win11 merge + v1.0 cleanup** (2026-04-24):
      `db/canonical.toml` закоммичен (zero-setup consumer), Win11 26200.8246
      snapshot смёржен союзом с Win10 19045.6466 (509 функций), удалены
      `rsc_hash_cstr` + `asm_x64.rs` / `asm_x86.rs` + все feature flags +
      unused workspace deps (`static_assertions`, `trybuild`, `insta`) +
      env var `RSC_SEED_OVERRIDE`. См. запись в dev log.
- [ ] 🟡 Новый Windows patch → `rsc collect --force` → commit diff в `db/auto/`
- [ ] 🟡 Обновление phnt submodule → `rsc phnt-parse` → verify → merge
- [ ] 🟡 Ежеквартально: проверка покрытия новых функций
- [ ] 🟢 Bench против legacy lib (опционально)

---

## Сводная таблица фаз

| Phase | Название | Блокирует | Est. effort |
|---|---|---|---|
| 0 | Scaffolding | 1,3,4,7 | 0.5 дня |
| 1 | Runtime foundation | 2 | 2 дня |
| 2 | Codegen | 6 | 2-3 дня |
| 3 | PDB Collector | 5 | 2-3 дня |
| 4 | phnt parser | 5 | 1-2 дня |
| 5 | Merge & CLI | 6 | 1-2 дня |
| 6 | Wire runtime to canonical | 7,8 | 1 день |
| 7 | C bindings | 8 | 1 день |
| 8 | Testing | 9 | 2 дня |
| 9 | Docs & polish | — | 1-2 дня |
| 10 | Maintenance | — | ongoing |

**Итого до v0.1.0**: ~2-3 недели фокусной работы.

## Критический путь

```
Phase 0 ─► Phase 1 ─► Phase 2 ─────┐
               └──► Phase 3 ─► Phase 5 ─► Phase 6 ─► Phase 7 ─► Phase 8 ─► Phase 9
                    Phase 4 ────┘
```

Phase 1 + Phase 3 + Phase 4 — **параллельно-допустимы** (разные крейты, разные зоны ответственности).

## Git-стратегия

- Каждая Phase = feature branch: `phase-N-short-name`
- Merge в `master` (или `main`) после закрытия всех `[ ]` фазы
- Коммиты внутри фазы — атомарные подфазы (1.1, 1.2, ...)
- Conventional commits: `feat(rsc-runtime): add PEB walker`, `fix(rsc-collector): handle CAB unpack error`

## Правила обновления этого файла

1. **Метку `[ ]` → `[x]` ставить только после**:
   - реального выполнения и ручной проверки
   - успешного `cargo build` / `cargo test` (где применимо)
   - записи в dev log
2. **Новые задачи** — добавлять в соответствующую фазу (или в Phase 10 если вне scope текущих)
3. **Отмена задачи** — `[-]` + краткое объяснение рядом
4. **В работе** — `[~]` (временная метка, не отражает готовность)
