/*
 * basic.c — minimal C consumer of rsc-c.
 *
 * Allocates a page via RscNtAllocateVirtualMemory, touches it, frees
 * it via RscNtFreeVirtualMemory. Build with `scripts/build_c_example.bat`.
 */
#include <stdio.h>
#include <stdint.h>
#include "rsc.h"

int main(void)
{
    RSC_HANDLE proc = (RSC_HANDLE)(intptr_t)-1; /* pseudo-handle: current process */
    RSC_PVOID  base = NULL;
    RSC_SIZE_T size = 0x1000;

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
    printf("[+] wrote 0x%08X, read back 0x%08X\n",
           0xCAFEBABEu, *(uint32_t *)base);

    RSC_SIZE_T zero = 0;
    st = RscNtFreeVirtualMemory(proc, &base, &zero, RSC_MEM_RELEASE);
    if (st != RSC_STATUS_SUCCESS) {
        fprintf(stderr, "free failed: 0x%08X\n", (unsigned)st);
        return 1;
    }
    printf("[+] freed\n");
    return 0;
}
