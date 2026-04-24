# CURRENT_STATE — SysCalls (RSC)

> Снимок текущего состояния реализации. Read-me-first для понимания того, где
> находится проект прямо сейчас. Не changelog — историю см. в dev log.

## Последнее обновление: 2026-04-24

## Текущая версия: **v1.0** (release-ready)

## Общий статус

Проект завершил весь критический путь Phase 0 → Phase 9, плюс v1.0 cleanup
refactor и объединение Win10 + Win11 снимков в один canonical.toml. Никаких
блокирующих gap'ов. Maintenance mode (Phase 10).

- **509 NT syscall функций** — union по имени из обоих покрываемых Windows
  сборок (x64 + x86/WoW64)
- **Path A** (canonical committed) — `db/canonical.toml` лежит в репо,
  consumer делает `cargo add rsc-runtime` без сети и без сборки PDB
- **Core / Tools split** — три крейта ядра + три крейта инструментов
- **Zero runtime deps** в `rsc-runtime` — hard invariant соблюдён
- **No feature flags** — v1.0 сознательно без условной компиляции (JUMPER
  random slide, essential NTSTATUS names, stable `RSC_SEED`)

## Покрываемые Windows сборки

| Build ID | Windows | ntdll exports |
|---|---|---|
| `10_19045_6466` | Windows 10 22H2 | 472 x64 + 492 x86/WoW64 |
| `11_26200_8246` | Windows 11 24H2 | 488 x64 + 509 x86/WoW64 |

**Baseline build** (источник SSN / RVA / stub meta): `11_26200_8246`. При
резолве на хосте, где функция отсутствует, `resolve()` возвращает `None`, а
не паникует — compile-time API всегда = супермножество.

**Win11-эксклюзивные функции** (17): `NtCreateIoRing`, `NtSubmitIoRing`,
`NtQueryIoRingCapabilities`, `NtCreatePartition` (новая сигнатура),
`NtCreateCpuPartition`, `NtOpenCpuPartition`, `NtQueryInformationCpuPartition`,
`NtSetInformationCpuPartition`, `NtCreateThreadStateChange`,
`NtChangeThreadState`, `NtCreateProcessStateChange`, `NtChangeProcessState`,
`NtSetEventEx`, `NtReadVirtualMemoryEx`, и др. (см.
`db/auto/11_26200_8246.toml`).

---

## Crate layout

### Core (конечный потребитель)

| Крейт | Назначение | Артефакт |
|---|---|---|
| `rsc-runtime` | `#![no_std]` lib, syscall стабы, PEB walker, hash, table | rlib |
| `rsc-codegen` | proc-macro `rsc_syscall!{}` — naked stub template (x64 + x86/WoW64) | proc-macro dylib |
| `rsc-c`       | C bindings (cdylib + staticlib + rsc.h) | `rsc.dll`, `rsc.lib`, `rsc.h` |

### Tools (maintainer-only)

| Крейт | Назначение | Артефакт (release) |
|---|---|---|
| `rsc-collector` | Symbol Server + DbgHelp + disasm → `db/auto/<build>.toml` | `rsc-collector.exe` |
| `rsc-types`     | phnt headers → `db/phnt/phnt.toml` | `rsc-types.exe` |
| `rsc-cli`       | unified `rsc merge / verify / diff / stats` | `rsc.exe` |

---

## База данных

Формат и правила merge — см. `DATABASE.md`.

| Файл | Статус | Записей | В репо |
|---|---|---|---|
| `db/auto/10_19045_6466.toml` | ✅ snapshot Win10 22H2 | 492 | yes |
| `db/auto/11_26200_8246.toml` | ✅ snapshot Win11 24H2 | 509 | yes |
| `db/phnt/phnt.toml` | ✅ phnt commit `53fbbdc` | 779 unique | yes |
| `db/overrides.toml` | template | 0 active | yes |
| `db/canonical.toml` | ✅ union merge | **509** | **yes (Path A)** |

---

## Реализованные инварианты

- ZERO runtime deps в `rsc-runtime` (только `rsc-codegen` как build-time proc-macro)
- `RSC_SEED = 0x52534300` — pinned, compile-time hash, zero collisions на всех именах из БД
- Lazy init syscall table: tri-state `AtomicU8` (UNINIT / POPULATING / READY) + compare-exchange — race-safe
- Random JUMPER per call: `__rsc_random_slide()` возвращает свежий случайный `syscall; ret` slide при каждом вызове
- Real x86 / WoW64 naked stub: `fs:[0xC0]` gate + `call ecx` + `ret {cleanup}` (cleanup = 4 × arity)
- `build.rs` рантайма reads `db/canonical.toml` → emits 509 `rsc_syscall!{…}` в `$OUT_DIR/syscalls_generated.rs`
- `rsc-c/build.rs` emits 509 `RscNt*` wrappers + `rsc.h` c `RSC_*` типами / константами

## Тесты и верификация

- `cargo test --workspace` — **49/49 passed**
- `cargo test -p rsc-runtime --test integration` — 4/4 (live alloc/protect/query/free)
- `cargo clippy --workspace --all-targets -- -D warnings` — 0 warnings
- `cargo check --workspace --target i686-pc-windows-msvc` — OK
- `cargo build --workspace --release` — ~30 s

---

## Что работает end-to-end

### Consumer flow (zero setup)

```bash
# В Cargo.toml: rsc-runtime = "..."
cargo add rsc-runtime
# Всё. Каноническая БД уже в репо, сеть не нужна.
```

```rust
use rsc_runtime::{nt_close, HANDLE};
let status = unsafe { nt_close(0xDEADBEEF_usize as HANDLE) };
```

### Maintainer flow (обновление БД под новую Windows)

```bash
scripts/refresh.bat     # collect + phnt + merge + verify + stats
# или вручную:
cargo run -p rsc-collector -- --force
cargo run -p rsc-types
cargo run --bin rsc -- merge
cargo run --bin rsc -- verify
```

### Live smoke examples

```bash
cargo run --example resolver_demo -p rsc-runtime   # хэш/SSN/slide 15 имён
cargo run --example memory_alloc  -p rsc-runtime   # 2 MiB alloc + protect + free
scripts/build_c_example.bat                        # собирает examples/c/basic.c
```

---

## Release artifacts (release build)

| Файл | Размер |
|---|---|
| `target/release/rsc-collector.exe` | 2.2 MB |
| `target/release/rsc-types.exe`     | 1.4 MB |
| `target/release/rsc.exe`           | 1.3 MB |
| `target/release/rsc.dll`           | 282 KB |
| `target/release/rsc.lib`           | 12.9 MB |
| `target/release/rsc-consumer-template.exe` | 273 KB |

---

## Что дальше (Phase 10 — maintenance)

- Новый Windows patch → `scripts/refresh.bat` → commit diff в `db/auto/`
- Обновление phnt submodule → `rsc-types` → `rsc merge` → `rsc verify`
- Периодически — проверка coverage новых функций, review overrides
- Возможные расширения: ARM64 support, дополнительные builds в union

---

## История обновлений файла

- **2026-04-24** — файл создан (Phase 0 начальное состояние)
- **2026-04-24** — обновлён после Phase 0–9 + Phase 2b + opaque audit
- **2026-04-24** — переписан под v1.0: 509 union (Win10+Win11), Path A, Core/Tools split, features removed
