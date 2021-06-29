set(CARGO_ARGS -Z build-std=core,compiler_builtins,alloc -Z build-std-features=compiler-builtins-mem)

add_subdirectory(localdep)
add_subdirectory(boot)
add_subdirectory(kernel)
