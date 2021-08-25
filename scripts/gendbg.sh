#!/bin/bash

objcopy --only-keep-debug target/BootX64.efi debug/BootX64.sym
objcopy --strip-debug target/BootX64.efi
ndisasm target/BootX64.efi -b 64 > debug/BootX64.asm
objdump -x target/BootX64.efi > debug/BootX64.txt

objcopy --only-keep-debug target/KERNEL debug/H2O.sym
objcopy --strip-debug target/KERNEL
ndisasm target/KERNEL -b 64 > debug/H2O.asm
objdump -x target/KERNEL > debug/H2O.txt