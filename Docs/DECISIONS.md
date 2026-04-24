# DECISIONS — SysCalls (RSC)

Архитектурные решения с обоснованием. Каждое решение нумеруется и больше не меняется — если решение пересматривается, создаётся новое с пометкой **супер­сидит D-NN**.

## D-01. Workspace из 6 крейтов

**Решение**: Разбить проект на `rsc-runtime`, `rsc-codegen`, `rsc-collector`, `rsc-types`, `rsc-cli`, `rsc-c`.

**Причина**:
- Runtime (consumer-facing) не должен зависеть от heavyweight деп (DbgHelp, HTTP) — изоляция
- Codegen — proc-macro, должен быть отдельным крейтом по требованию Rust
- Collector / Types — build-time utilities, не нужны в runtime
- CLI — тонкая обёртка над крейтами, не тянет всё в один бинарь
- C-bindings — отдельный крейт-обёртка для `cdylib` + `staticlib`

**Альтернатива**: монокрейт с features — отклонено, features не помогают отделить proc-macro и создают запутанную матрицу.

---

## D-02. Формат БД — TOML (не SQLite, не JSON)

**Решение**: Canonical и слои БД — файлы `.toml`.

**Причина**:
- Human-readable, diffable в git
- Кастомные правки в overrides элементарны
- Нет binary-зависимости (как SQLite)
- Статичные данные — БД обновляется редко (при новой Windows build)
- Rust-экосистема: `toml` + `serde` — стабильны и быстры

**Альтернативы**:
- SQLite — отклонено: не diffable, нужен binding, overkill
- JSON — отклонено: комментариев нет, многословный
- YAML — отклонено: избыточно сложный парсинг
- INI — отклонено: нет типизации (но подход KSC сохраняем для вдохновения)

---

## D-03. Парсер PDB — DbgHelp primary, `pdb` crate fallback

**Решение**: Первичная реализация через DbgHelp FFI (как в KSC). Rust-крейт `pdb` как опциональный backend через feature flag.

**Причина**:
- DbgHelp — authoritative parser от Microsoft, поддерживает все форматы PDB
- `pdb` crate имеет ограничения: неполное покрытие, нет bitfields
- Для ntdll нам НЕ нужны bitfields / sophisticated type queries — только export list + symbol addresses
- Поэтому `pdb` crate реалистично достаточен для **нашего** scope — но DbgHelp даёт страховку

**Решение о приоритете**: начинаем с DbgHelp для consistency с KSC, feature-flag `pdb-rust` добавляется позже если захочется offline-сборка без Windows SDK.

---

## D-04. HTTP client — ureq (синхронный)

**Решение**: Использовать `ureq` v3, как в KSC.

**Причина**:
- Одна загрузка PDB за запуск — async избыточен
- Меньший бинарник, проще код, быстрее компилирует
- `ureq` имеет встроенную поддержку TLS через `rustls` (без OpenSSL)

**Альтернатива**: `reqwest + tokio` — отклонено, добавляет runtime без пользы.

---

## D-05. Cache path — `%APPDATA%\rsc\cache\`

**Решение**: PDB-кэш в `%APPDATA%\Roaming\rsc\cache\{pdb_name}\{GUID}{AGE}\{pdb_name}`.

**Причина**:
- Совместимость со стандартными Windows conventions (не засоряет %LOCALAPPDATA% или Documents)
- Per-user — нет admin-требований
- Иерархия по GUID+AGE — стандарт Symbol Server

**Альтернатива**: рядом с бинарником — отклонено, ломает idempotency, засоряет репо.

---

## D-06. Хеш-алгоритм — ROR8 + seed (наследуем от SW3)

**Решение**: Сохранить ROR8-hash, но сменить seed.

**Причина**:
- ROR8 — проверенный алгоритм, быстрый в asm, уже работает в runtime
- Compile-time вычислимо как `const fn`
- Смена seed → новый проект с уникальными хешами (сигнатуры AV/EDR не совпадут с SW3-based образцами)

**RSC_SEED**: `0x52534300` (ASCII "RSC\0"). Mnemonic + отличный от SW3 `0xB8A54425`.

Утверждается окончательно в Phase 1 (может измениться на этапе проверки коллизий).

---

## D-07. JUMPER mode — всегда включён

**Решение**: Runtime никогда не делает прямой `syscall` из своего кода — всегда `jmp`/`call` на адрес syscall-инструкции внутри ntdll.

**Причина**:
- Stack-trace маскировка — главное security-преимущество SW3
- Нет смысла оставлять "простой" режим — это ухудшение security без прироста производительности

**Feature flag `no-jumper`** — опционально для debug/dev сценариев.

---

## D-08. Thread-safe lazy init через AtomicBool

**Решение**: Заменить "гонка идемпотентна" на корректную lazy init через `AtomicBool` + `Ordering::Acquire/Release`.

**Причина**:
- В `syscalls-rust` (legacy) заявлено "гонка идемпотентна" — фактически race condition, но результат одинаков. На x86/x64 TSO это работает, но не формально корректно.
- С AtomicBool + Acquire/Release pattern получаем формально корректный publish без overhead (одна atomic load на не-первом вызове)
- `static mut SYSCALL_TABLE` заменяется на `UnsafeCell` + atomic flag (либо `OnceLock` если подходит под no_std — проверить)

**Важно**: не добавляем `std::sync::Once` — это `std`. Используем чисто atomic или собственную реализацию.

---

## D-09. WoW64 detection — runtime через `fs:[0xC0]`

**Решение**: В стабе x86 проверяем `fs:[0xC0]` (Wow64SystemServiceCall) != 0 → WoW64 путь, иначе → native sysenter.

**Причина**:
- Один бинарник работает и на native x86 и в WoW64
- Не требует compile-time флагов или конфигурации
- Универсальный approach, работает на всех Windows 7+

**Критично**: dummy return address (push 0) перед аргументами при вызове WoW64 gate — иначе аргументы смещены на 4 байта.

---

## D-10. Кастомный per-build seed — через feature + build.rs

**Решение**: По умолчанию — фиксированный `RSC_SEED`. Через feature `random-seed` + env var `RSC_SEED_OVERRIDE` — билд с уникальным seed.

**Причина**:
- Red team может нуждаться в per-campaign unique hashes
- Default фиксированный — для dev experience и reproducibility

**Реализация**: `build.rs` в `rsc-runtime` читает env var → пишет `pub const RSC_SEED: u32 = ...;` в OUT_DIR.

---

## D-11. Не генерируем lib.rs на уровне текста — используем proc-macro + include!

**Решение**: `canonical.toml` → `build.rs` в `rsc-runtime` → `$OUT_DIR/syscalls_generated.rs` (список `rsc_syscall!` вызовов) → `include!()` в `syscalls.rs`.

**Причина**:
- Никогда не коммитим сгенерированные файлы в исходники (нет конфликтов merge)
- proc-macro делает expansion → type-safe at Rust level
- Изменение TOML → пересборка автоматически (через `cargo:rerun-if-changed`)
- Зер-cost для разработчика: `cargo build` делает всё

**Альтернатива**: offline генератор в `src/syscalls.rs` — отклонено, ломает workflow.

---

## D-12. Префикс `RSC_` — обязателен везде

**Решение**: Все публичные символы, C-типы, env-переменные, имена файлов cache/db используют префикс `RSC` (в соответствующем регистре).

**Конкретно**:
- Rust: `rsc_*` snake_case для fn/const, `Rsc*` PascalCase для types, `RSC_*` для consts
- C: `RSC_*` для всех типов, функций, макросов
- Env vars: `RSC_SEED_OVERRIDE`, `RSC_CACHE_DIR`, `RSC_CANONICAL_PATH`
- Files: `rsc.h`, `rsc.dll`, `rsc.lib`, `rsc_mingw.a`

**Причина**:
- Избежать конфликтов с `windows.h` (SDK) и старым `SW3_*` кодом
- Brand consistency — сразу видно "это из SysCalls"

---

## D-13. Два ntdll (native + WoW64) — собираются параллельно

**Решение**: `rsc-collector` на 64-bit Windows собирает **оба** ntdll: `System32\ntdll.dll` (x64) и `SysWOW64\ntdll.dll` (x86). В `auto/<build>.toml` функции имеют поле `arch`.

**Причина**:
- Один бинарник `rsc-runtime` может работать как native x64, так и x86/WoW64 — нужны номера для обеих
- SSN могут различаться между x64 и x86 (обычно совпадают, но не всегда)
- Один collector run = complete picture

**Альтернатива**: два отдельных запуска collector'а — отклонено, лишний шаг.

---

## D-14. Canonical DB — производный, не коммитится (или коммитится с пометкой)

**Решение**: По умолчанию `db/canonical.toml` — в `.gitignore`. Регенерируется `rsc merge` при билде (опционально).

**Причина**:
- Это derived artifact — не должен быть источником конфликтов в PR
- Коммитится `db/auto/*.toml` (per-build snapshots — они ground truth) и `db/overrides.toml`
- `canonical.toml` регенерируется детерминистически из этих двух + phnt

**Альтернатива**: коммитить canonical — рассматривается для облегчения CI, но приоритет — чистоту derived/source разделения.

**Допустимый компромисс**: коммитить `canonical.toml` для **последней** версии Windows (дефолтный билд), регенерация опциональна.

---

## D-15. Phnt — git submodule на pinned commit

**Решение**: `vendor/phnt` — git submodule, зафиксированный на конкретном коммите. Обновление = осознанный `git submodule update` + commit.

**Причина**:
- Reproducible builds через 5 лет
- Обновление phnt = code review точка (т.к. может сломать типы)
- Не зависим от доступности GitHub / phnt repo в момент билда

**Альтернатива**: fetch phnt при build — отклонено, ломает offline сборку.

---

## D-16. Atomic writes через temp file + rename

**Решение**: Все output файлы (`auto/*.toml`, `canonical.toml`, PDB cache) пишутся через `{path}.tmp` → rename.

**Причина**:
- Crash-safe: частичный файл не overwrite'ит хороший
- Стандартная practice (так делает KSC)

---

## D-17. `#![no_std]` в runtime — без компромиссов

**Решение**: `rsc-runtime` не импортирует `std::*`. Нет `Vec`, нет `String`, нет `HashMap`, нет `std::sync::*`.

**Причина**:
- Offensive инструмент должен быть injectable / standalone
- std тянет CRT, allocator, panic unwinder — размер и детектируемость
- Все нужные типы — fixed-size arrays, primitives

**Единственная зависимость runtime**: proc-macro `rsc-codegen` (build-time, не rt).

---

## D-18. error.rs — облегчённая NtStatus, без heavy Display

**Решение**: `NtStatus` имеет методы `is_success()`, `is_error()`, `code()` — но НЕ имеет громоздкой `Display`-таблицы 260 статусов.

**Причина**:
- Таблица 260 статусов → большой размер бинарника
- Для no_std `Display` требует `core::fmt` — ок, но таблица строк разрастается

**Компромисс**: базовые статусы (SUCCESS, ACCESS_DENIED, INVALID_HANDLE, NOT_IMPLEMENTED, и ~20 критичных) — имеют `name()` через match. Остальные — только hex code.

**Feature `status-names-full`** — включает полную таблицу (наследие из legacy error.rs).

---

## D-19. x64 arity — heuristic + phnt cross-ref

**Решение**: Arity (количество параметров) x64 syscall'ов — пытаемся:
1. Если phnt даёт — используем phnt
2. Иначе — heuristic: чтение первых ~40 байт стаба, подсчёт `mov r9,[rsp+...]` и прочих stack-loads
3. Fallback на max arity — 15 параметров (безопасно для ASM stub)

**Причина**:
- x86 — тривиально (`ret N`)
- x64 — нет прямого индикатора в стабе, Microsoft не обязан делать одинаковые стабы

**Tolerance**: если arity > реального — лишние push/mov безвредны (stub положит мусор в stack slots которые kernel не читает). Главное — не меньше реального.

---

## D-20. Version identifier — `{Major}_{Build}_{UBR}` (как в KSC)

**Решение**: Идентификатор Windows build: `10_19045_6466`, `11_26100_2314`, и т.д.

**Причина**:
- Совместимость с KSC-нотацией (их INI секции)
- Уникально идентифицирует точную версию
- Читаемо, сортируется естественно

**Источник данных**:
- Major/Build — `RtlGetVersion`
- UBR — registry `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\UBR`

---

## D-21. CLI — единый бинарь `rsc` с subcommands

**Решение**: `rsc-cli` эмитит бинарь с именем `rsc` с subcommands: `collect`, `merge`, `verify`, `diff`, `stats`, `phnt-parse`.

**Причина**:
- UX: один бинарь для всех операций
- Пакетируется как standalone tool
- Общие флаги (`--verbose`, `--db-dir`) — один раз

**Альтернатива**: отдельные бинари per-команда — отклонено, UX хуже.

---

## D-22. Наследование low-level паттернов из legacy lib

**Решение**: PEB walking, hash computation, syscall table layout, JUMPER random pick, ASM шаблоны — **портируем концептуально из `syscalls-rust/lib.rs`**, но:
- Префикс SW3 → RSC
- Комментарии translate EN, сохраняем
- Разбиваем на module файлы (не один мегафайл)
- Убираем duplication через макрос

**Причина**:
- Legacy код проверен годами — нет смысла переоткрывать
- Заимствование логики ≠ заимствование структуры

---

## D-23. Первая поддерживаемая Windows baseline

**Решение**: Минимально поддерживаемая Windows версия — **Windows 10 1809 (build 17763)**.

**Причина**:
- Современный baseline для red team target'ов
- WoW64 gate pattern стабилизировался
- Не засоряем код legacy Vista/7/8 quirks

**Реально**: runtime скорее всего будет работать и на Win7+, но официально не тестируем.

---

## D-24. Formatter/linter — rustfmt default + clippy

**Решение**: Код форматируется `rustfmt` с default config. `cargo clippy --workspace -- -D warnings` в CI.

**Причина**: стандартная Rust practice, zero-config.

---

## Принципы для будущих решений

1. **Безопасность > удобство** (это security-инструмент)
2. **Reproducibility > актуальность** (vendored всё что можно)
3. **no_std в runtime — свято** (нарушается только с D-XX override)
4. **Один источник истины** для каждого типа данных
5. **Метки RSC** обязательны для любого нового публичного символа

## Ждут решения (open questions)

- **OQ-1**: Использовать `OnceLock` (ждёт стабилизации в no_std) или свой AtomicBool lazy-init? → решим в Phase 1.
- **OQ-2**: `pdb` crate как primary или fallback? → финализируется после Phase 3 prototype.
- **OQ-3**: Нужен ли compat-layer `sw3_*` → `rsc_*` для migration? → финализируется в Phase 9.
- **OQ-4**: Коммитить canonical.toml последней версии или нет? → финализируется в Phase 5.
- **OQ-5**: Формат committed PDB snapshots — zip / lz4 / git-lfs? → финализируется в Phase 8.
