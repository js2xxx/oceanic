# Oceanic: A PC desktop OS

[![996.icu](https://img.shields.io/badge/link-996.icu-red.svg)](https://996.icu)

[中文说明](README.zh-cn.md)

**WARNING:** The project is still at the very early stage, and user programs are
unavailable in the OS. Any potential risk of running the software either in 
virtual machines or bare metals should be taken into account by the user.

Currently, the project only supports x86_64 architecture, and it will probably
support aarch64 in the future. 

# Source tree

- `debug` - contains the decompiled assembly files, debug symbols, object file informations. and the serial log files of the virtual machines.
- `h2o` - contains the source code for the kernel.
- `lib` - contains the source library code for the entire project.
- `scripts` - contains the scripts required for building the project.
- `target` - contains the binaries and virtual disk files.
- `xtask` - contains the builder for the project.

# Build and debug from source

## Linux

1. Download rust and other dependencies (Ubuntu for example):
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   sudo apt install build-essential qemu-system-x86
   ```

2. Add the following toolchain:
   ```sh
   rustup add toolchain nightly-x86_64-unknown-linux-gnu
   ```

3. Change to the project's root directory and run the following command:
   ```sh
   cargo xtask dist iso
   ```

4. To run the OS with qemu, run the following command:
   ```sh
   sh scripts/run.sh qemu N # N for the number of CPUs
   ```
   and check `debug/qemu.log` file, you should see the output of the OS.

5. To debug with qemu, run the following command:
   ```sh
   sh scripts/run.sh qdbg N # Same as above
   ```
   and open a new terminal:
   ```sh
   # cd to the working directory
   gdb debug/FOO.sym
   # FOO for the binary you want to debug;
   # you may check it in the directory first.

   # In the gdb:
   target remote :1234
   ```
   then you can set breakpoints (KERNEL.sym for example):
   ```sh
   b kmain
   c
   ```

6. If you want to run the OS with other VM softwares, check the run.sh first,
   and manually create VM configuration files as you wish. Don't forget to add
   the virtual disk and the serial log or no output will be present!

# Contributions

If you want to make contributions, be sure to contact me first.
* Email: [akucxy@163.com](mailto:akucxy@163.com)
* QQ: 2534027071
