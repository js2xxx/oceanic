file(GLOB_RECURSE ASM_SRC entry/x86_64/*.asm)

set(CMAKE_ASM_NASM_FLAGS "-f elf64")

add_executable(TINIT ${ASM_SRC})

add_dependencies(GEN_IMG TINIT)

add_custom_command(
    TARGET TINIT POST_BUILD
    COMMAND cp TINIT ${CMAKE_SOURCE_DIR}/target
)