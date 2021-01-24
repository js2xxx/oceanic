#!/bin/bash

if [ $1 = "qemu" ]; then
      qemu-system-x86_64 -L /usr/share/ovmf -bios OVMF.fd \
            -m 4096 -cpu max -smp $2 -serial file:qemu.log \
            -monitor stdio -drive format=raw,file=target/img/efi.img -boot c 
fi