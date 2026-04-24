# rsc-codegen

## Назначение

Proc-macro крейт. Экспортирует один макрос `rsc_syscall!`, который разворачивает
Nt-функциональную сигнатуру в naked syscall stub с JUMPER-диспатчем через ntdll.

## Статус

✅ **Phase 2a завершена** (session 004, 2026-04-24). x64 naked stub полностью
рабочий, live alloc/free/close через macro-generated стаб подтверждены.
x86/WoW64 ветка — placeholder (STATUS_NOT_IMPLEMENTED), реальный stub → Phase 2b.

## Структура

```
rsc-codegen/
├── Cargo.toml        [proc-macro = true] + syn/quote/proc-macro2/proc-macro-error2
└── src/
    └── lib.rs        188 строк — parser + expand_stub() + reject_unsupported()
```

## API (входная грамматика макроса)

```rust
rsc_syscall! {
    fn NtAllocateVirtualMemory(
        process_handle: HANDLE,
        base_address: *mut PVOID,
        zero_bits: ULONG_PTR,
        region_size: *mut SIZE_T,
        allocation_type: ULONG,
        protect: ULONG,
    ) -> NTSTATUS;
}
```

Парсится через `syn::ForeignItemFn` — функциональная сигнатура без тела
(как в extern блоках), закрывается `;`.

## Expansion (x64)

```rust
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[allow(non_snake_case)]
pub unsafe extern "system" fn NtAllocateVirtualMemory(
    _arg1: HANDLE,
    _arg2: *mut PVOID,
    _arg3: ULONG_PTR,
    _arg4: *mut SIZE_T,
    _arg5: ULONG,
    _arg6: ULONG,
) -> NTSTATUS {
    core::arch::naked_asm!(
        "push rcx",
        "push rdx",
        "push r8",
        "push r9",
        "sub rsp, 0x28",
        "mov ecx, {hash}",
        "call {resolve_slide}",
        "mov r11, rax",
        "mov ecx, {hash}",
        "call {resolve_ssn}",
        "add rsp, 0x28",
        "pop r9",
        "pop r8",
        "pop rdx",
        "pop rcx",
        "mov r10, rcx",
        "jmp r11",
        hash = const ::rsc_runtime::rsc_hash(b"NtAllocateVirtualMemory"),
        resolve_slide = sym ::rsc_runtime::__rsc_resolve_slide,
        resolve_ssn = sym ::rsc_runtime::__rsc_resolve_ssn,
    )
}
```

## Expansion (x86 placeholder, Phase 2a)

```rust
#[cfg(target_arch = "x86")]
#[allow(non_snake_case)]
pub unsafe extern "system" fn NtAllocateVirtualMemory(
    _arg1: HANDLE,
    // ...
) -> NTSTATUS {
    0xC000_0002u32 as i32    // STATUS_NOT_IMPLEMENTED
}
```

## Ключевые решения

### Почему split resolvers (не единый 16-byte struct)

Rust's `extern "system" fn() -> ResolveOut` где `ResolveOut` 16 bytes на x64 Windows
использует **hidden pointer ABI**, а не `RAX:RDX`. Это caused STATUS_ACCESS_VIOLATION
в первой реализации — naked asm ожидал `RDX` = slide, но Rust функция трактовала
`RDX` как второй input argument (реально — hash оказался в RDX после first mov rcx).

Split на `__rsc_resolve_ssn(u32) -> u32` + `__rsc_resolve_slide(u32) -> usize` даёт
**предсказуемый integer-returning ABI** (EAX / RAX). Overhead двойного вызова
≈ 10 нс, пренебрежимо на фоне syscall kernel transition ~100 нс.

### Почему два `#[cfg]` ветви (не один naked с runtime WoW64 detect)

- x64 native — `syscall` инструкция работает нативно
- x86 native — использует `sysenter`, иной DISTANCE_TO_SYSCALL
- x86 WoW64 — использует gate `fs:[0xC0]`, требует dummy return address
  на стеке перед аргументами

Три разных ABI — проще иметь раздельные cfg ветви чем один runtime-branching
наked stub (тот будет больше, менее stealth, с большим количеством edge cases).

Phase 2a реализует только x64. Phase 2b добавит naked x86 с runtime WoW64 detect
через `test eax, eax` после `mov eax, fs:[0xC0]`.

### Span-accurate ошибки

`reject_unsupported()` использует `proc_macro_error2::abort!` — error message указывает
на **конкретный узел** Syn AST (e.g. `sig.asyncness`), не просто на call site макроса.
Это даёт максимально полезные сообщения пользователю.

## Зависимости (build-time)

| Crate | Версия | Назначение |
|---|---|---|
| `syn` | 2.0 (full) | Парсинг TokenStream → ForeignItemFn |
| `quote` | 1.0 | Генерация Rust кода |
| `proc-macro2` | 1.0 | TokenStream abstraction (testable вне proc-macro контекста) |
| `proc-macro-error2` | 2.0 | Span-accurate `abort!`/`emit_error!` diagnostics |

## MSRV
Rust 1.88+ — требуется для стабильного `#[unsafe(naked)]` и `core::arch::naked_asm!`.

## Тесты

Пока нет `trybuild` тестов (Phase 8 / follow-up). Косвенно проверяется через
`examples/one_syscall.rs` в rsc-runtime — если макрос сломается, example перестаёт
компилироваться.

Live verification: `cargo run --example one_syscall -p rsc-runtime` должен успешно
выделить + записать + прочитать + освободить страницу памяти.

## Что дальше — Phase 2b (opt)

- [ ] naked x86 stub с runtime WoW64 detect через `fs:[0xC0]`
- [ ] Native x86 path: `sysenter` через slide с DISTANCE_TO_SYSCALL = 0x0F
- [ ] WoW64 path: dummy ret + gate call с inlateout("eax") + params array для >4 args
- [ ] `trybuild` tests для fail cases (3+ ok, 3+ fail)
- [ ] Feature `debug-breakpoints` — `int3` перед `jmp r11`
- [ ] Feature `no-jumper` — прямой `syscall` из стаба (трейдофф: стеалтиer → stealth menos)
