if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/debug)
else ()
    set(CARGO_CMD cargo build --release)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/rls)
endif ()

add_custom_target(
    H2O ALL 
    CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
)

file(GLOB RS_LIB "${GLOB_PATH}/libh2o.a")
file(GLOB_RECURSE ASM_SRC "entry/x86_64/*.asm")

set(CMAKE_ASM_NASM_FLAGS "-f elf64")

add_link_options(-T ${CMAKE_CURRENT_SOURCE_DIR}/kernel.ld)

add_executable(KERNEL ${ASM_SRC})
add_dependencies(KERNEL H2O)

target_link_libraries(KERNEL ${RS_LIB})