cmake_minimum_required(VERSION 3.10)

enable_language(ASM)
enable_language(ASM_NASM)

set(CMAKE_C_CREATE_SHARED_LIBRARY "/usr/bin/ld <CMAKE_C_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")
set(CMAKE_C_LINK_EXECUTABLE "/usr/bin/ld <CMAKE_C_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")
set(CMAKE_ASM_NASM_LINK_EXECUTABLE "/usr/bin/ld <CMAKE_ASM_NASM_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")
set(CMAKE_ASM_NASM_COMPILE_OBJECT "<CMAKE_ASM_NASM_COMPILER> <DEFINES> <INCLUDES> <FLAGS> -o <OBJECT> <SOURCE>")

project(oceanic)

set(CMAKE_BUILD_TYPE Debug)

add_custom_target(
      GEN_IMG ALL
      cd ${CMAKE_SOURCE_DIR} && sh scripts/gendbg.sh && sh scripts/genimg.sh
      COMMENT "Generating the disk image"
)

add_subdirectory(h2o)