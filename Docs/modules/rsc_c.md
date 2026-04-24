# rsc-c

> Stub. Наполняется по мере реализации **Phase 7**.

## Назначение

C биндинги для `rsc-runtime`. Собирается как `staticlib` + `cdylib`. Генерирует `include/rsc.h` на этапе build.

## Отвечает за

- Re-export публичных символов `rsc-runtime` с C ABI (`#[no_mangle] extern "C"`)
- Автогенерация `rsc.h` (build.rs)
- Артефакты: `rsc.dll`, `rsc.lib`, `rsc_mingw.a`
- Префикс `RSC_` для всех C-типов и функций

## Не отвечает за

- Runtime логика (это `rsc-runtime`)
- Генерация runtime-функций (это `rsc-codegen` + `build.rs` в runtime)

## Структура

```
rsc-c/
├── Cargo.toml              [crate-type = ["staticlib", "cdylib"]]
├── build.rs                — generate include/rsc.h
└── src/
    └── lib.rs              — pub use rsc-runtime::*; + extern "C" wrappers
```

## Подход к генерации rsc.h

**Источник правды**: `canonical.toml` (а НЕ парсинг Rust исходников). Это надёжнее чем regex на `src/syscalls.rs` (который теперь macro-based).

**Альтернатива**: cbindgen — отклонено (см. D-07 в `DECISIONS.md`).

## Схема rsc.h (layout)

```c
#ifndef RSC_H
#define RSC_H

/* Prelude: include guards, stdint.h, extern "C" */

/* Type definitions (RSC_HANDLE, RSC_NTSTATUS, ...) */

/* Conditionally avoid conflicts with windows.h */
#ifndef _WINDEF_
typedef void* RSC_HANDLE;
/* ... */
#else
#define RSC_HANDLE HANDLE
/* ... */
#endif

/* Constants (STATUS_*, PAGE_*, MEM_*, ...) */

/* Structures (UNICODE_STRING, OBJECT_ATTRIBUTES, ...) */

/* Function declarations */
RSC_NTSTATUS RscNtAllocateVirtualMemory(
    RSC_HANDLE ProcessHandle,
    /* ... */
);
/* ... ~500 functions */

#endif /* RSC_H */
```

## Сборка и использование

### MSVC
```
cl myapp.c /I<path-to-rsc-include> /link rsc.lib ntdll.lib kernel32.lib
```

### MinGW
```
gcc myapp.c -I<rsc-include> -L<rsc-lib> -lrsc_mingw -lntdll -lkernel32
```

### CMake (пример)
```cmake
find_library(RSC_LIB rsc PATHS ${CMAKE_SOURCE_DIR}/vendor/rsc/lib)
include_directories(${CMAKE_SOURCE_DIR}/vendor/rsc/include)
target_link_libraries(myapp PRIVATE ${RSC_LIB})
```

## Совместимость с legacy (syscalls-rust/c-bindings)

**Не бинарно совместимо**, так как префикс изменён (`SW3_` → `RSC_`). Миграция через `sed`:

```bash
sed -i 's/SW3_/RSC_/g' *.c *.h
sed -i 's/SW3Nt/RscNt/g' *.c *.h
```

Это сработает для 99% кейсов простой замены. Глубокие изменения типов (если будут) — документируются в `MIGRATION.md` (Phase 9).
