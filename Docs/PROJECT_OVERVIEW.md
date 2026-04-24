# PROJECT_OVERVIEW — SysCalls (RSC)

## Суть

**SysCalls** — переосмысленная реализация библиотеки прямых Windows NT syscall'ов на Rust. Главная идея — **ground truth из `ntdll.pdb` + curated типы из phnt + ручные overrides**, склеенные в единую TOML-базу и развёрнутые в compile-time макро-генерацию.

Отказ от SysWhispers3. Заимствование концепции **автосбора символов из Microsoft Symbol Server** у проекта [KernelSymbolsCollector](../../../KernelSymbolsCollector/), адаптированное под ntdll (user-mode) вместо ntoskrnl (kernel-mode).

## Целевая аудитория

- Red team операторы
- Пентестеры
- Security-исследователи (Windows internals, EDR bypass)
- Разработчики offensive-инструментов

## Отличия от старого `syscalls-rust`

| Аспект | `syscalls-rust` (legacy) | **SysCalls (RSC)** |
|---|---|---|
| Источник syscall-номеров | runtime (PEB → ntdll → sort) | runtime (тот же) + **compile-time list из PDB** |
| Источник имён и сигнатур | SysWhispers3 (Python, manual JSON) | `ntdll.pdb` (exports) + phnt (types) + overrides |
| Размер lib.rs | ~57 000 строк (автоген SW3) | **~2 000 строк** (macro-based) |
| Добавить функцию | Копипаст inline ASM-стаба | Одна строка в `canonical.toml` |
| Мульти-версия Windows | Нет явной поддержки | Да, БД `db/auto/<build>.toml` на каждую ОС |
| Дрейф-чек при новом патче | Ручной ре-ран SW3 | `cargo run -p rsc-cli -- verify` |
| Префикс C API | `SW3_*` | `RSC_*` |
| Seed для хеша | `0xB8A54425` | Новый, см. `DECISIONS.md` |
| Reproducibility билда | Да (JSON закоммичен) | Да (phnt submodule + PDB snapshots) |
| Offline сборка | Да | Да (после первого сбора БД) |
| Зависимость от Python | Да (SW3) | **Нет** |

## Ключевые возможности

1. **Полное покрытие ntdll** — автосбор списка всех Zw*/Nt* функций из PDB, без "забытых" функций
2. **Типизированные сигнатуры** — phnt overlay для ~95% функций, опаковые указатели для остальных
3. **Мульти-версионная БД** — track по Windows build (10 19045, 11 22631, 11 26100, и т.д.)
4. **Macro-based генерация** — `rsc_syscall!(NtAllocateVirtualMemory, 6, ...)` → compile-time стаб
5. **Нулевая зависимость от SysWhispers3** — свой PDB-парсер (через DbgHelp или `pdb` крейт), свой генератор
6. **WoW64 support из коробки** — макрос разворачивает x64/x86/WoW64 ветки
7. **JUMPER mode** — порт из старого проекта, без изменений логики
8. **Кастомный seed per-build** — каждая сборка может иметь уникальные хеши (опционально)
9. **Feature flags по категориям** — компактный билд под узкую задачу
10. **C биндинги** — cdylib + staticlib + `rsc.h`, как в старом проекте, но префикс `RSC_`

## Ограничения осознанные

- **Windows only** — syscall-номера специфичны для Windows NT
- **Только ntdll** — win32k.sys (GUI: NtUser*/NtGdi*) вне scope (может быть добавлен позже как отдельная ветка)
- **Только user-mode** — kernel syscalls не покрываются
- **Требует DbgHelp или эквивалент** для сборки PDB → но это нужно только на этапе `rsc-collector`, runtime не зависит
- **Первый запуск `rsc-collector` требует интернет** (скачать PDB с `msdl.microsoft.com`), далее работает из кэша

## Стек

| Компонент | Технология |
|---|---|
| Язык | Rust (edition 2021, `#![no_std]` в runtime) |
| Архитектура | x86_64, i686 (Windows only) |
| Сборка | Cargo workspace, proc-macro |
| Парсинг PDB | DbgHelp FFI (предпочтительно) или `pdb` crate |
| HTTP | `ureq` (синхронный, как в KSC) |
| Формат БД | TOML (canonical), TOML snapshots per-build |
| C биндинги | staticlib + cdylib, автоген `rsc.h` |
| Лицензия | MIT |

## Фразеологизмы проекта (быстрый словарь)

- **RSC** — RustSysCall, кодовый префикс
- **Ground truth** — данные прямо из `ntdll.pdb` (истина первой инстанции)
- **Canonical DB** — единый `canonical.toml`, источник для генерации
- **Auto layer** — слой 1, автосбор из PDB + binary analysis
- **Phnt layer** — слой 2, типы из phnt headers
- **Overrides layer** — слой 3, ручные правки
- **Build** — версия Windows в формате `{Major}_{Build}_{UBR}` (например `11_26100_2314`)
- **SSN** — System Service Number (номер syscall в таблице ядра)
- **Stub** — байты ntdll-функции (`mov eax, SSN; syscall; ret`), из них декодируется SSN и arity
- **JUMPER** — режим когда вместо прямого `syscall` делается `jmp` на `syscall; ret` в ntdll

## Как читать остальную документацию

1. **`ARCHITECTURE.md`** — глобальный дизайн + ASCII-графы потоков
2. **`ROADMAP.md`** — разбиение на фазы с метками `[ ]` / `[x]`, это главный документ прогресса
3. **`DECISIONS.md`** — все значимые архитектурные решения с обоснованием
4. **`DATABASE.md`** — полная спецификация формата TOML
5. **`API.md`** — публичный Rust/C API (обновляется по мере реализации)
6. **`CURRENT_STATE.md`** — снимок текущего состояния (%)
7. **`modules/`** — per-crate документация (появляется по мере реализации)

## Размеры (ожидаемые, финальные)

| Файл | Оценка строк | Размер |
|---|---|---|
| rsc-runtime/src/lib.rs | ~500 | ~15 KB |
| rsc-runtime/src/peb.rs | ~200 | ~6 KB |
| rsc-runtime/src/hash.rs | ~100 | ~3 KB |
| rsc-runtime/src/syscalls.rs (macro calls) | ~1500 | ~40 KB |
| rsc-codegen/src/lib.rs (proc-macro) | ~600 | ~20 KB |
| rsc-collector (весь крейт) | ~2500 | ~80 KB |
| rsc-types | ~800 | ~25 KB |
| rsc-c | ~400 | ~12 KB |
| **Итого** | **~6 600** | **~200 KB** |

Для сравнения: старый `syscalls-rust` — ~60 000 строк / ~2 MB.

## Метрики успеха

- [ ] 100% покрытие ntdll ZwXxx функций (сверка с PDB export table)
- [ ] Все тесты старого `test_syscalls.rs` проходят на новом runtime
- [ ] БД содержит минимум 3 Windows builds (Win10 22H2, Win11 23H2, Win11 24H2)
- [ ] lib.rs < 2000 строк
- [ ] Offline сборка возможна после первого `rsc-collector run`
- [ ] C биндинги (`rsc.h`) — drop-in замена для простых случаев (с sed `SW3_` → `RSC_`)
- [ ] Добавление нового syscall = 1 строка в overrides.toml
