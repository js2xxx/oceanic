#if !defined(__ACOCEANIC_H__)
#define __ACOCEANIC_H__

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#define ACPI_USE_DO_WHILE_0
#define ACPI_IGNORE_PACKAGE_RESOLUTION_ERRORS

#define ACPI_USE_GPE_POLLING
#define ACPI_USE_LOCAL_CACHE

#define COMPILER_DEPENDENT_INT64 int64_t
#define COMPILER_DEPENDENT_UINT64 uint64_t
#define ACPI_CPU_FLAGS unsigned long

#if !defined(ACPI_INLINE)
#define ACPI_INLINE inline
#endif // ACPI_INLINE

#define ACPI_DIV_64_BY_32(n, n_hi, n_lo, d32, q32, r32) \
	{                                                 \
		q32 = n / d32;                              \
		r32 = n % d32;                              \
	}
#define ACPI_SHIFT_RIGHT_64(n, n_hi, n_lo) \
	{                                    \
		n <<= 1;                       \
	}

#define ACPI_EXPORT_SYMBOL(Symbol)

#if !defined(ACPI_UNUSED_VAR)
#define ACPI_UNUSED_VAR __attribute__((unused))
#endif // ACPI_UNUSED_VAR

#define ACPI_MACHINE_WIDTH 64
#define ACPI_FLUSH_CPU_CACHE() asm("wbinvd")

// TODO: use self's implementation in the future.
// #define ACPI_USE_SYSTEM_CLIBRARY

#define ACPI_OS_NAME "Microsoft Windows NT"

extern void AcpiOsVprintf(const char *fmt, va_list args);
static inline void AcpiOsPrintf(const char *fmt, ...) {
	va_list	args;
	va_start(args, fmt);
	AcpiOsVprintf(fmt, args);
	va_end(args);
}

#endif // __ACOCEANIC_H__
