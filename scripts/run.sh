#!/bin/bash

if [ $1 = "qemu" ]; then
      qemu-system-x86_64 -L /usr/share/ovmf -bios OVMF.fd \
            -m 4096 -cpu max -smp $2 -serial file:debug/qemu.log \
            -drive format=raw,file=target/img/efi.img -boot c \
            -monitor stdio -M q35 $3 $4 $5 $6 $7 $8 $9
elif [ $1 = "qdbg" ]; then
      qemu-system-x86_64 -L /usr/share/ovmf -bios OVMF.fd \
            -m 4096 -cpu max -smp $2 -serial file:debug/qemu.log \
            -drive format=raw,file=target/img/efi.img -boot c \
            -monitor stdio -M q35 -s -S $3 $4 $5 $6 $7 $8 $9
elif [ $1 = "vbox" ]; then
    /usr/lib/virtualbox/VirtualBoxVM --startvm "OV3" --dbg $2 $3 $4 $5 $6 $7 $8 $9
elif [ $1 = "vmware" ]; then
    rm -f debug/vmware.log
    vmplayer ~/vmware/OV3/OV3.vmx $2 $3 $4 $5 $6 $7 $8 $9
elif [ $1 = "vmwdbg" ]; then
    rm -f debug/vmware.log
    vmplayer ~/vmware/OV3/OV3Debug.vmx $2 $3 $4 $5 $6 $7 $8 $9
fi