/*****************************************************************************
 *
 * Macros used for ACPICA globals and configuration
 *
 ****************************************************************************/

/*
 * Ensure that global variables are defined and initialized only once.
 *
 * The use of these macros allows for a single list of globals (here)
 * in order to simplify maintenance of the code.
 */
#ifdef DEFINE_ACPI_GLOBALS
#define ACPI_GLOBAL(type,name) \
    extern type name; \
    type name

#define ACPI_INIT_GLOBAL(type,name,value) \
    type name=value

#else

#ifndef CUSTOM_INIT_GLOBALS

#ifndef ACPI_GLOBAL
#define ACPI_GLOBAL(type,name) \
    extern type name
#endif

#ifndef ACPI_INIT_GLOBAL
#define ACPI_INIT_GLOBAL(type,name,value) \
    extern type name
#endif

#else

#ifndef ACPI_GLOBAL
#define ACPI_GLOBAL(type,name) 
#endif

#ifndef ACPI_INIT_GLOBAL
#define ACPI_INIT_GLOBAL(type,name,value) \
    name=value
#endif

#endif

#endif


/*****************************************************************************
 *
 * Public globals and runtime configuration options
 *
 ****************************************************************************/

/*
 * Enable "slack mode" of the AML interpreter?  Default is FALSE, and the
 * interpreter strictly follows the ACPI specification. Setting to TRUE
 * allows the interpreter to ignore certain errors and/or bad AML constructs.
 *
 * Currently, these features are enabled by this flag:
 *
 * 1) Allow "implicit return" of last value in a control method
 * 2) Allow access beyond the end of an operation region
 * 3) Allow access to uninitialized locals/args (auto-init to integer 0)
 * 4) Allow ANY object type to be a source operand for the Store() operator
 * 5) Allow unresolved references (invalid target name) in package objects
 * 6) Enable warning messages for behavior that is not ACPI spec compliant
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_EnableInterpreterSlack, FALSE);

/*
 * Automatically serialize all methods that create named objects? Default
 * is TRUE, meaning that all NonSerialized methods are scanned once at
 * table load time to determine those that create named objects. Methods
 * that create named objects are marked Serialized in order to prevent
 * possible run-time problems if they are entered by more than one thread.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_AutoSerializeMethods, TRUE);

/*
 * Create the predefined _OSI method in the namespace? Default is TRUE
 * because ACPICA is fully compatible with other ACPI implementations.
 * Changing this will revert ACPICA (and machine ASL) to pre-OSI behavior.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_CreateOsiMethod, TRUE);

/*
 * Optionally use default values for the ACPI register widths. Set this to
 * TRUE to use the defaults, if an FADT contains incorrect widths/lengths.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_UseDefaultRegisterWidths, TRUE);

/*
 * Whether or not to validate (map) an entire table to verify
 * checksum/duplication in early stage before install. Set this to TRUE to
 * allow early table validation before install it to the table manager.
 * Note that enabling this option causes errors to happen in some OSPMs
 * during early initialization stages. Default behavior is to allow such
 * validation.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_EnableTableValidation, TRUE);

/*
 * Optionally enable output from the AML Debug Object.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_EnableAmlDebugObject, FALSE);

/*
 * Optionally copy the entire DSDT to local memory (instead of simply
 * mapping it.) There are some BIOSs that corrupt or replace the original
 * DSDT, creating the need for this option. Default is FALSE, do not copy
 * the DSDT.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_CopyDsdtLocally, FALSE);

/*
 * Optionally ignore an XSDT if present and use the RSDT instead.
 * Although the ACPI specification requires that an XSDT be used instead
 * of the RSDT, the XSDT has been found to be corrupt or ill-formed on
 * some machines. Default behavior is to use the XSDT if present.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_DoNotUseXsdt, FALSE);

/*
 * Optionally use 32-bit FADT addresses if and when there is a conflict
 * (address mismatch) between the 32-bit and 64-bit versions of the
 * address. Although ACPICA adheres to the ACPI specification which
 * requires the use of the corresponding 64-bit address if it is non-zero,
 * some machines have been found to have a corrupted non-zero 64-bit
 * address. Default is FALSE, do not favor the 32-bit addresses.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_Use32BitFadtAddresses, FALSE);

/*
 * Optionally use 32-bit FACS table addresses.
 * It is reported that some platforms fail to resume from system suspending
 * if 64-bit FACS table address is selected:
 * https://bugzilla.kernel.org/show_bug.cgi?id=74021
 * Default is TRUE, favor the 32-bit addresses.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_Use32BitFacsAddresses, TRUE);

/*
 * Optionally truncate I/O addresses to 16 bits. Provides compatibility
 * with other ACPI implementations. NOTE: During ACPICA initialization,
 * this value is set to TRUE if any Windows OSI strings have been
 * requested by the BIOS.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_TruncateIoAddresses, FALSE);

/*
 * Disable runtime checking and repair of values returned by control methods.
 * Use only if the repair is causing a problem on a particular machine.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_DisableAutoRepair, FALSE);

/*
 * Optionally do not install any SSDTs from the RSDT/XSDT during initialization.
 * This can be useful for debugging ACPI problems on some machines.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_DisableSsdtTableInstall, FALSE);

/*
 * Optionally enable runtime namespace override.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_RuntimeNamespaceOverride, TRUE);

/*
 * We keep track of the latest version of Windows that has been requested by
 * the BIOS. ACPI 5.0.
 */
ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_OsiData, 0);

/*
 * ACPI 5.0 introduces the concept of a "reduced hardware platform", meaning
 * that the ACPI hardware is no longer required. A flag in the FADT indicates
 * a reduced HW machine, and that flag is duplicated here for convenience.
 */
ACPI_INIT_GLOBAL (BOOLEAN,          AcpiGbl_ReducedHardware, FALSE);

/*
 * Maximum timeout for While() loop iterations before forced method abort.
 * This mechanism is intended to prevent infinite loops during interpreter
 * execution within a host kernel.
 */
ACPI_INIT_GLOBAL (UINT32,           AcpiGbl_MaxLoopIterations, ACPI_MAX_LOOP_TIMEOUT);

/*
 * Optionally ignore AE_NOT_FOUND errors from named reference package elements
 * during DSDT/SSDT table loading. This reduces error "noise" in platforms
 * whose firmware is carrying around a bunch of unused package objects that
 * refer to non-existent named objects. However, If the AML actually tries to
 * use such a package, the unresolved element(s) will be replaced with NULL
 * elements.
 */
ACPI_INIT_GLOBAL (BOOLEAN,          AcpiGbl_IgnorePackageResolutionErrors, FALSE);

/*
 * This mechanism is used to trace a specified AML method. The method is
 * traced each time it is executed.
 */
ACPI_INIT_GLOBAL (UINT32,           AcpiGbl_TraceFlags, 0);
ACPI_INIT_GLOBAL (const char *,     AcpiGbl_TraceMethodName, NULL);
ACPI_INIT_GLOBAL (UINT32,           AcpiGbl_TraceDbgLevel, ACPI_TRACE_LEVEL_DEFAULT);
ACPI_INIT_GLOBAL (UINT32,           AcpiGbl_TraceDbgLayer, ACPI_TRACE_LAYER_DEFAULT);

/*
 * Runtime configuration of debug output control masks. We want the debug
 * switches statically initialized so they are already set when the debugger
 * is entered.
 */
#ifdef ACPI_DEBUG_OUTPUT
ACPI_INIT_GLOBAL (UINT32,           AcpiDbgLevel, ACPI_DEBUG_DEFAULT);
#else
ACPI_INIT_GLOBAL (UINT32,           AcpiDbgLevel, ACPI_NORMAL_DEFAULT);
#endif
ACPI_INIT_GLOBAL (UINT32,           AcpiDbgLayer, ACPI_COMPONENT_DEFAULT);

/* Optionally enable timer output with Debug Object output */

ACPI_INIT_GLOBAL (UINT8,            AcpiGbl_DisplayDebugTimer, FALSE);

/*
 * Debugger command handshake globals. Host OSes need to access these
 * variables to implement their own command handshake mechanism.
 */
#ifdef ACPI_DEBUGGER
ACPI_INIT_GLOBAL (BOOLEAN,          AcpiGbl_MethodExecuting, FALSE);
ACPI_GLOBAL (char,                  AcpiGbl_DbLineBuf[ACPI_DB_LINE_BUFFER_SIZE]);
#endif

/*
 * Other miscellaneous globals
 */
ACPI_GLOBAL (ACPI_TABLE_FADT,       AcpiGbl_FADT);
ACPI_GLOBAL (UINT32,                AcpiCurrentGpeCount);
ACPI_GLOBAL (BOOLEAN,               AcpiGbl_SystemAwakeAndRunning);