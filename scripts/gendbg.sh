#!/bin/bash

objcopy --only-keep-debug target/BootX64.efi debug/BootX64.sym
objcopy --strip-debug target/BootX64.efi
ndisasm target/BootX64.efi -b 64 > debug/BootX64.asm
objdump -x target/BootX64.efi > debug/BootX64.txt

objcopy --only-keep-debug target/H2O.k debug/H2O.sym
objcopy --strip-debug target/H2O.k
ndisasm target/H2O.k -b 64 > debug/H2O.asm
objdump -x target/H2O.k > debug/H2O.txt