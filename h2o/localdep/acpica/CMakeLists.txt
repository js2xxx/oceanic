add_compile_options(-Wno-unused-parameter -Wno-address -Wno-sign-compare -Wno-pointer-to-int-cast -Wno-unused-value -Wno-unknown-pragmas -Wno-implicit-function-declaration -fno-exceptions -Wno-write-strings -fno-stack-protector -fno-builtin -std=c11 -g -O1 -mcmodel=large)

include_directories(include)

file(GLOB_RECURSE AC_SRC "*.c")

add_library(acpica STATIC ${AC_SRC})