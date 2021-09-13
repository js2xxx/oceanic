if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build ${CARGO_ARGS} --target x86_64-unknown-linux-gnu)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-linux-gnu/debug)
else ()
    set(CARGO_CMD cargo build ${CARGO_ARGS} --target x86_64-unknown-linux-gnu --release)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-linux-gnu/release)
endif ()

add_custom_target(
    TINIT_RS ALL 
    CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
)

file(GLOB RS_LIB ${GLOB_PATH}/libtinit.a)
file(GLOB_RECURSE ASM_SRC entry/x86_64/*.asm)

set(CMAKE_ASM_NASM_FLAGS "-f elf64")

add_link_options(-T ${CMAKE_CURRENT_SOURCE_DIR}/tinit.ld  -lto-O2)

add_executable(TINIT ${ASM_SRC})
target_link_libraries(TINIT ${RS_LIB})
add_dependencies(TINIT TINIT_RS)

add_dependencies(GEN_IMG TINIT)

add_custom_command(
    TARGET TINIT POST_BUILD
    COMMAND cp TINIT ${CMAKE_SOURCE_DIR}/target
)