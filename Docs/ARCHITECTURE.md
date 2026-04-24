# ARCHITECTURE — SysCalls (RSC)

## 1. Верхнеуровневая структура

```
                    ┌────────────────────────────────────────┐
                    │            SysCalls / RSC              │
                    │        (Cargo Workspace, MIT)          │
                    └────────────┬───────────────────────────┘
                                 │
      ┌──────────────────────────┼──────────────────────────┐
      │                          │                          │
      ▼                          ▼                          ▼
 ┌─────────┐              ┌──────────┐              ┌─────────┐
 │ Runtime │              │  Build   │              │   DB    │
 │ (runs)  │              │  Tools   │              │ (data)  │
 └─────────┘              └──────────┘              └─────────┘
   │                         │                         │
 rsc-runtime             rsc-collector          db/auto/*.toml
 rsc-codegen             rsc-types              db/phnt/phnt.toml
 rsc-c (bindings)        rsc-cli                db/overrides.toml
                                                 db/canonical.toml
                                                (generated)
```

## 2. Разбиение на крейты

### 2.1 Runtime (использует потребитель)

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-runtime (#![no_std])                                     │
│                                                              │
│  src/                                                        │
│    lib.rs          ─ публичный фасад, re-export всех частей  │
│    peb.rs          ─ PEB, ntdll discovery, export walk       │
│    hash.rs         ─ RSC hash (ROR8 + seed), const fn        │
│    table.rs        ─ SW3-style syscall table, lazy init      │
│    jumper.rs       ─ random syscall-addr picker              │
│    asm_x64.rs      ─ naked stub template (x64 syscall)       │
│    asm_x86.rs      ─ naked stub template (x86 + WoW64 gate)  │
│    syscalls.rs     ─ ТОЛЬКО rsc_syscall!(...) вызовы макроса │
│                     (генерируется build.rs из canonical.toml)│
│    error.rs        ─ NtStatus / RscResult                    │
│    types.rs        ─ HANDLE, PVOID, UNICODE_STRING и т.д.    │
│                                                              │
│  build.rs          ─ читает canonical.toml, эмитит syscalls.rs│
└──────────────────────────────────────────────────────────────┘

  Зависимости: rsc-codegen (proc-macro only, build-time)
  Runtime deps: НЕТ (pure no_std, inline asm)
```

### 2.2 Codegen (proc-macro)

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-codegen (proc-macro crate)                               │
│                                                              │
│  src/lib.rs                                                  │
│    ├─ rsc_syscall! macro                                     │
│    │   │                                                     │
│    │   ├─ parse: (name, arity, types)                        │
│    │   ├─ compute hash at compile-time (const fn RSC hash)   │
│    │   └─ emit:                                              │
│    │       #[cfg(x86_64)] fn name(...) { asm_x64_template }  │
│    │       #[cfg(x86)]    fn name(...) { asm_x86_template }  │
│    │                                                         │
│    └─ helpers: const_ror8, seed constant, template expansion │
│                                                              │
│  Runtime deps: syn, quote, proc-macro2                       │
└──────────────────────────────────────────────────────────────┘
```

### 2.3 Collector (PDB fetcher + analyzer)

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-collector (binary)                                       │
│                                                              │
│  src/                                                        │
│    main.rs               ─ CLI: collect for current build    │
│    version.rs            ─ RtlGetVersion + registry UBR      │
│    symsrv.rs             ─ Microsoft Symbol Server client    │
│                          ─ HTTP GET + CAB extract            │
│                          ─ Cache %APPDATA%\rsc\cache\        │
│    pe.rs                 ─ PE header parse, extract GUID/Age │
│    pdb_reader.rs         ─ DbgHelp FFI (preferred) OR        │
│                          ─ pdb crate (fallback)              │
│    stub_disasm.rs        ─ decode ntdll stub bytes:          │
│                             • x86: ret N → arity = N/4       │
│                             • x64: count reg uses + stack    │
│    emit.rs               ─ write db/auto/<build>.toml        │
│                                                              │
│  Runtime deps: windows, ureq, anyhow, winreg,                │
│                toml, serde, pdb (optional)                   │
└──────────────────────────────────────────────────────────────┘
```

### 2.4 Types (phnt parser)

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-types (binary + library)                                 │
│                                                              │
│  src/                                                        │
│    main.rs               ─ parse phnt headers → phnt.toml    │
│    parser.rs             ─ C header tokenizer/regex          │
│                          ─ extract NTSYSCALLAPI NTSTATUS     │
│                            NTAPI NtXxxFunction(...)          │
│    normalizer.rs         ─ phnt types → canonical type names │
│                                                              │
│  Vendored: vendor/phnt/ (git submodule, pinned commit)       │
│                                                              │
│  Runtime deps: regex, toml, serde, anyhow                    │
└──────────────────────────────────────────────────────────────┘
```

### 2.5 CLI (unified tool)

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-cli (binary)                                             │
│                                                              │
│  Subcommands:                                                │
│    rsc collect [--force] [--build <id>]                      │
│       → runs rsc-collector logic                             │
│                                                              │
│    rsc phnt-parse [--submodule-path <path>]                  │
│       → runs rsc-types parser                                │
│                                                              │
│    rsc merge                                                 │
│       → auto + phnt + overrides → canonical.toml             │
│                                                              │
│    rsc verify [--strict]                                     │
│       → drift check: current PDB vs committed auto.toml      │
│       → phnt vs current submodule                            │
│       → overrides referencing missing functions              │
│                                                              │
│    rsc diff <build-a> <build-b>                              │
│       → show added/removed/changed functions                 │
│                                                              │
│    rsc stats                                                 │
│       → count per category, coverage, etc.                   │
└──────────────────────────────────────────────────────────────┘
```

### 2.6 C bindings

```
┌──────────────────────────────────────────────────────────────┐
│ rsc-c (cdylib + staticlib)                                   │
│                                                              │
│  src/lib.rs            ─ re-export rsc-runtime + C wrappers  │
│  build.rs              ─ regex parse + generate rsc.h        │
│                          (заимствовано из старого build.rs,  │
│                           адаптировано: SW3_ → RSC_)         │
│  include/rsc.h         ─ autogenerated C header              │
│                                                              │
│  Output: rsc.dll, rsc.lib, rsc_mingw.a                       │
└──────────────────────────────────────────────────────────────┘
```

## 3. Поток данных

### 3.1 Build-time: сборка БД

```
┌────────────────────┐       ┌────────────────────┐       ┌────────────────────┐
│ Microsoft          │       │ phnt git submodule │       │ overrides.toml     │
│ Symbol Server      │       │ (pinned commit)    │       │ (hand-maintained)  │
│ msdl.microsoft.com │       │ vendor/phnt/*.h    │       │ db/overrides.toml  │
└─────────┬──────────┘       └─────────┬──────────┘       └─────────┬──────────┘
          │                            │                             │
          │ HTTP GET                   │ read                        │
          ▼                            ▼                             │
┌────────────────────┐       ┌────────────────────┐                  │
│ rsc-collector      │       │ rsc-types          │                  │
│  ├ download PDB    │       │  ├ parse headers   │                  │
│  ├ DbgHelp parse   │       │  ├ extract protos  │                  │
│  ├ disasm stubs    │       │  └ emit TOML       │                  │
│  └ emit TOML       │       └─────────┬──────────┘                  │
└─────────┬──────────┘                 │                             │
          │                            │                             │
          ▼                            ▼                             │
┌────────────────────┐       ┌────────────────────┐                  │
│ db/auto/           │       │ db/phnt/phnt.toml  │                  │
│   10_19045_6466.toml│      │                    │                  │
│   11_22631_2514.toml│      │                    │                  │
│   11_26100_2314.toml│      │                    │                  │
└─────────┬──────────┘       └─────────┬──────────┘                  │
          │                            │                             │
          └──────────┬─────────────────┴─────────────────────────────┘
                     │
                     ▼
              ┌──────────────┐
              │  rsc merge   │   (priority: overrides > phnt > auto)
              └──────┬───────┘
                     │
                     ▼
           ┌──────────────────────┐
           │  db/canonical.toml   │ ← single source of truth
           └──────────────────────┘
```

### 3.2 Build-time: генерация кода

```
           ┌──────────────────────┐
           │  db/canonical.toml   │
           └──────────┬───────────┘
                      │
                      │ read at build
                      ▼
           ┌──────────────────────┐
           │ rsc-runtime/build.rs │
           └──────────┬───────────┘
                      │
                      │ emit: rsc_syscall!(NtXxx, arity, types) × N
                      ▼
           ┌──────────────────────┐
           │  $OUT_DIR/syscalls.rs│
           └──────────┬───────────┘
                      │
                      │ include!()
                      ▼
           ┌──────────────────────┐
           │ rsc-runtime/src/     │
           │   syscalls.rs (module│
           │   aggregator)        │
           └──────────┬───────────┘
                      │
                      │ rsc_syscall! macro expansion
                      │ (from rsc-codegen, proc-macro)
                      ▼
          ┌─────────────────────────┐
          │  generated Rust fns:    │
          │  #[naked] fn NtXxx(...) │
          │  { naked_asm!(...) }    │
          └──────────┬──────────────┘
                     │
                     ▼
              rustc compiles → rsc-runtime.rlib/staticlib/cdylib
```

### 3.3 Runtime: вызов syscall

```
Пользовательский код
      │
      ▼
┌─────────────────────────┐
│ rsc::NtAllocateVirtual..│   macro-expanded naked fn
│   (args)                │
└──────────┬──────────────┘
           │
           │ call
           ▼
┌──────────────────────────────────────┐
│ naked_asm stub                       │
│   ├─ mov eax, <hash>                 │
│   ├─ call rsc_resolve (sym fn)       │  ┐
│   └─ jmp/call via resolved syscall   │  │ first call per-fn:
└──────────────────────────────────────┘  │   resolves hash → SSN
           │                              │   picks random syscall addr
           │ first call?                  │
           ▼                              │
┌─────────────────────────┐               │
│ rsc_resolve(hash)       │               │
│  ├─ [init?] rsc_populate│ ◄─────────────┘
│  │    ├ PEB.Ldr         │
│  │    ├ find ntdll      │
│  │    ├ walk Zw* exports│
│  │    ├ sort by addr    │
│  │    ├ hash each name  │
│  │    └ find syscall ins│
│  └─ table[hash] → ssn, addr│
└──────────┬──────────────┘
           │
           ▼
┌──────────────────────────────────────┐
│ Windows kernel                       │
│   syscall/sysenter/WoW64-gate        │
│   → dispatches by SSN                │
│   → returns NTSTATUS in EAX          │
└──────────────────────────────────────┘
```

## 4. Файловая структура

```
SysCalls/
├── Cargo.toml                     # workspace root
├── CLAUDE.md                      # AI-instructions
├── README.md                      # user-facing overview (создаётся в Phase 0)
├── .gitignore
├── .gitmodules                    # vendor/phnt submodule
│
├── Docs/
│   ├── PROJECT_OVERVIEW.md
│   ├── ARCHITECTURE.md            # <этот файл>
│   ├── ROADMAP.md                 # фазы и метки
│   ├── CURRENT_STATE.md
│   ├── DECISIONS.md
│   ├── DATABASE.md
│   ├── API.md
│   └── modules/
│       ├── rsc_runtime.md
│       ├── rsc_codegen.md
│       ├── rsc_collector.md
│       ├── rsc_types.md
│       ├── rsc_cli.md
│       └── rsc_c.md
│
├── crates/
│   ├── rsc-runtime/
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── peb.rs
│   │   │   ├── hash.rs
│   │   │   ├── table.rs
│   │   │   ├── jumper.rs
│   │   │   ├── asm_x64.rs
│   │   │   ├── asm_x86.rs
│   │   │   ├── syscalls.rs      # aggregator, include!(OUT_DIR/...)
│   │   │   ├── error.rs
│   │   │   └── types.rs
│   │   └── examples/
│   │       └── basic.rs
│   │
│   ├── rsc-codegen/              # proc-macro
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   │
│   ├── rsc-collector/            # binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── version.rs
│   │       ├── symsrv.rs
│   │       ├── pe.rs
│   │       ├── pdb_reader.rs
│   │       ├── stub_disasm.rs
│   │       └── emit.rs
│   │
│   ├── rsc-types/                # binary + lib
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── parser.rs
│   │       └── normalizer.rs
│   │
│   ├── rsc-cli/                  # unified CLI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       └── commands/
│   │           ├── collect.rs
│   │           ├── merge.rs
│   │           ├── verify.rs
│   │           ├── diff.rs
│   │           └── stats.rs
│   │
│   └── rsc-c/                    # C bindings
│       ├── Cargo.toml
│       ├── build.rs
│       ├── src/lib.rs
│       └── include/rsc.h         # generated
│
├── db/
│   ├── auto/
│   │   ├── 10_19045_6466.toml   # per Windows build
│   │   ├── 11_22631_2514.toml
│   │   └── 11_26100_2314.toml
│   ├── phnt/
│   │   └── phnt.toml
│   ├── overrides.toml
│   └── canonical.toml            # generated (gitignored or committed)
│
├── vendor/
│   ├── phnt/                     # git submodule
│   └── pdb-snapshots/            # committed (compressed) for CI reproducibility
│       ├── 10_19045_6466/
│       └── 11_26100_2314/
│
├── examples/
│   ├── basic_syscall.rs
│   ├── memory_alloc.rs
│   └── process_enum.rs
│
└── tests/
    ├── integration.rs
    ├── runtime_smoke.rs
    └── drift.rs
```

## 5. Ключевые инварианты

### 5.1 Runtime (rsc-runtime)

- **`#![no_std]`** — никогда не импортирует `std::*`
- **Нет heap-аллокаций** — всё на стеке или в `static mut`
- **Thread-safe первый вызов** — хоть и idempotent, используем atomic flag для публикации (см. `DECISIONS.md`)
- **JUMPER mode всегда активен** — нет опции "прямой syscall из стаба"
- **WoW64 detection runtime** — через `fs:[0xC0]`, ветка выбирается на каждом вызове
- **Хеш compile-time** — в стабе ассемблерный `mov eax, <hash>`, никакого runtime расчёта хеша

### 5.2 Codegen (rsc-codegen)

- **Один макрос `rsc_syscall!`** — разворачивает ВСЁ для одной функции
- **Два набора шаблонов** — `asm_x64` и `asm_x86_wow64` (через `#[cfg(target_arch)]`)
- **Хеш — `const fn`** — вычисляется макросом, не передаётся как литерал

### 5.3 Collector (rsc-collector)

- **Идемпотентен** — запуск на уже собранной ОС-версии = no-op (если нет `--force`)
- **Atomic writes** — temp file → rename, как в KSC
- **Cache key** — `%APPDATA%\rsc\cache\ntdll.pdb\{GUID}{AGE}\ntdll.pdb`
- **Два варианта ntdll** — native (`C:\Windows\System32\ntdll.dll`) и WoW64 (`C:\Windows\SysWOW64\ntdll.dll`) — собираются отдельно, emit в отдельные секции TOML

### 5.4 Database

- **TOML, не JSON/SQLite** — человеко-читаемый, git-diffable
- **Canonical = derived** — никогда не редактируется руками, только через `rsc merge`
- **Overrides — единственный manual-edit файл** для логики syscall'ов
- **Auto snapshots — commited** — reproducibility билда

## 6. Границы ответственности

| Крейт | Отвечает за | НЕ отвечает за |
|---|---|---|
| `rsc-runtime` | Вызов syscall'ов в runtime | Знания о версиях Windows, типах из phnt |
| `rsc-codegen` | Разворачивание макроса в стаб | Сбор данных, парсинг TOML |
| `rsc-collector` | Сбор ground-truth из PDB | Типы, phnt |
| `rsc-types` | Парсинг phnt headers | PDB, runtime |
| `rsc-cli` | Оркестровка (merge, verify, diff) | Сам сбор / парсинг |
| `rsc-c` | FFI surface | Runtime логика |

## 7. ASCII схема полного цикла разработки проекта

```
┌──────────────────────────────────────────────────────────────────┐
│                  DEVELOPMENT LIFECYCLE                           │
└──────────────────────────────────────────────────────────────────┘

   [New Windows patch released]
              │
              ▼
   ┌──────────────────────┐
   │ Dev runs:            │
   │   rsc collect --force│     ─── 30-60s ───►  db/auto/<build>.toml
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │ Dev runs:            │
   │   rsc merge          │     ─── <1s ───►     db/canonical.toml
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │ Dev runs:            │
   │   rsc verify         │     ─── check drift, missing types
   └──────────────────────┘
              │
        drift detected? ──► [update overrides.toml or bump phnt submodule]
              │
              ▼ no drift
   ┌──────────────────────┐
   │ cargo build          │     ─── build.rs reads canonical
   │   -p rsc-runtime     │          → expand macros
   └──────────────────────┘          → emit staticlib/cdylib
              │
              ▼
   ┌──────────────────────┐
   │ cargo test           │     ─── integration + runtime smoke
   │   --workspace        │
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │ git commit           │     ─── auto.toml + canonical.toml
   │   db/                │          + dev log entry
   └──────────────────────┘
```

## 8. Дизайн-альтернативы (отвергнутые)

| Альтернатива | Причина отказа |
|---|---|
| SQLite вместо TOML | Не diffable в git, нужен binding, overkill для статичных данных |
| cbindgen для `rsc.h` | Не справляется со сложным кодом (проверено в старом проекте) |
| `pdb` crate (Rust) как primary | Неполное покрытие PDB форматов; DbgHelp authoritative. Но оставляем как fallback |
| Async HTTP (reqwest + tokio) | Избыточно для одного запроса PDB; ureq делает то же синхронно |
| Код-ген как отдельная утилита (offline) | Usability хуже: каждое добавление функции требует ре-ран утилиты. build.rs + proc-macro — зероcost для дев-flow |
| Генерация в `lib.rs` напрямую | Ломает reproducibility (конфликты merge). OUT_DIR стандартная практика Cargo |
| Runtime resolve типов из phnt | Невозможно — типы нужны compile-time |

## 9. Совместимость с legacy `syscalls-rust`

- **API НЕ совместимо** по умолчанию — функции с префиксом `rsc_*` в Rust, `RSC_*` в C
- **Возможен compat-layer** в будущем через feature `sw3-compat` → re-export с `sw3_` префиксом (не обязательно, зависит от запроса)
- **Legacy test suite** (`test_syscalls.rs` из старого проекта) — будет портирован в `examples/` и `tests/` с новыми именами
