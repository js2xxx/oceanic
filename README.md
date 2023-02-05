# Oceanic: A PC desktop OS

[![996.icu](https://img.shields.io/badge/link-996.icu-red.svg)](https://996.icu)

[中文说明](README.zh-cn.md)

**WARNING:** The project is still at the very early stage, and user programs are
unavailable in the OS. Any potential risk of running the software either in 
virtual machines or bare metals should be taken into account by the user.

Currently, the project only supports x86_64 architecture, and it will probably
support aarch64 in the future. 

# Features

- **Pure microkernel architecture**: Only necessary functions such as memory management, task scheduling and inter-process communication are owned by the kernel, leaving others to the userspace tasks.
- **Fully asynchronous kernel objects**: Every system call supports **proactor** async API based on async dispatcher objects, which is theoretically more convenient and more resource-friendly than Linux's `io-uring`.
- **Fully asynchronous drivers**: Drivers are represented by dynamic libraries, loaded by driver hosts and controlled by the device manager. The DDK library enables drivers to run as async tasks on driver hosts, and multiple drivers can run on single driver host as they wish.
- **Type-based task management**: Every task's state are represented by Rust's type system instead of a single state field, and its structure doesn't need to rely on reference-counted pointers. The running tasks are owned by the scheduler, while the blocking ones by futex references or suspend tokens, thus decreasing code complexity.
- **Isolated and local VFS**: (Inspired by Fuchsia) Every process has its own VFS, and it's the task's choice whether to share the VFS with its child tasks.

# Road map

## Current working

- [ ] Complete the DDK library.
- [ ] Implement basic drivers (PCI, ACPI, etc).

## Further to-dos (may change)

- Implement storage drivers.
- Implement some storage FSes (FAT, etc).
- Complete the device manager and the program manager implementation.
- Merge into Rust std.

# Source tree

- `debug` - contains the decompiled assembly files, debug symbols, object file informations. and the serial log files of the virtual machines.
- `h2o` - contains the source code for the kernel.
- `scripts` - contains the scripts required for building the project.
- `src` - contains the source code of libraries and executables for the entire project.
- `target` - contains the binaries and virtual disk files.
- `xtask` - contains the builder for the project.

# Build and debug from source

## Linux

1. Download rust and other dependencies (Ubuntu for example):
   ```sh
   # Select the nightly channel for rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   sudo apt install build-essential qemu-system-x86 llvm-14 nasm
   export LLVM_PATH="/usr/lib/llvm-14"
   ```

2. Add the following target:
   ```sh
   rustup target add nightly-x86_64-unknown-linux-gnu
   ```

3. Change to the project's root directory and run the following command:
   ```sh
   cargo xtask dist img
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
