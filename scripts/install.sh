mkdir -p target/img/mnt
cd target

tar vcf H2O.k KERNEL TINIT

cd img

mkfs.vfat $1
sudo mount $1 mnt
sudo mkdir -p mnt/EFI/BOOT
sudo mkdir -p mnt/EFI/Oceanic
sudo cp ../BootX64.efi mnt/EFI/BOOT
sudo cp ../H2O.k mnt/EFI/Oceanic
sudo umount mnt
