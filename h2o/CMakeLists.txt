set(CARGO_ARGS -Zbuild-std=core,compiler_builtins,alloc,panic_abort -Zbuild-std-features=compiler-builtins-mem)

add_subdirectory(localdep)
add_subdirectory(boot)
add_subdirectory(kernel)
add_subdirectory(tinit)
