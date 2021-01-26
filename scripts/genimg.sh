#!/bin/bash

cd target/img

dd if=/dev/zero of=efi.img bs=1k count=1440
mformat -i efi.img -f 2880 ::
mmd -i efi.img ::/EFI
mmd -i efi.img ::/EFI/Boot
mmd -i efi.img ::/EFI/Oceanic
mcopy -i efi.img ../BootX64.efi ::/EFI/Boot
# mcopy -i efi.img ../H2O ::/EFI/Oceanic

qemu-img convert efi.img -f raw -O vmdk efi.vmdk