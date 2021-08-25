if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build ${CARGO_ARGS} --target x86_64-unknown-linux-gnu)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-linux-gnu/debug)
else ()
    set(CARGO_CMD cargo build ${CARGO_ARGS} --target x86_64-unknown-linux-gnu --release)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-linux-gnu/release)
endif ()

add_custom_target(
    H2O ALL 
    CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
)
add_dependencies(H2O acpica)

file(GLOB RS_LIB ${GLOB_PATH}/libh2o.a)
file(GLOB_RECURSE ASM_SRC entry/x86_64/*.asm)

set(CMAKE_ASM_NASM_FLAGS "-f elf64")

add_link_options(-T ${CMAKE_CURRENT_SOURCE_DIR}/h2o.ld)

add_executable(KERNEL ${ASM_SRC})
target_link_libraries(KERNEL ${RS_LIB})
add_dependencies(KERNEL H2O)

add_dependencies(GEN_IMG KERNEL)
add_custom_command(
    TARGET KERNEL POST_BUILD
    COMMAND cp KERNEL ${CMAKE_SOURCE_DIR}/target
)