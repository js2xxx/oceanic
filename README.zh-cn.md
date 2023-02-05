# 海洋OS：一个个人电脑操作系统

[![996.icu](https://img.shields.io/badge/link-996.icu-red.svg)](https://996.icu)
[![CI](https://github.com/js2xxx/oceanic/actions/workflows/ci.yml/badge.svg)](https://github.com/js2xxx/oceanic/actions/workflows/ci.yml)

**警告:** 这个项目还在非常早的开发阶段，操作系统尚未准备好用户程序环境。使用者或开发者应该考虑任何在虚拟机或裸机中运行的风险。

当前这个项目只支持x86_64架构。未来可能会支持aarch64。

# 特色

- **纯·微内核架构**：内核中只保留内存管理、任务调度、进程间通信等必要功能，其他的都在用户空间实现。
- **全异步内核对象**：所有系统调用均在异步派发器内核对象的支持下实现proactor的异步API，理论上比Linux的`io-uring`更加方便，占用更少资源。
- **全异步的驱动程序**：驱动程序以动态库为单位，在驱动宿主上运行，由设备管理进程控制。DDK使驱动程序能以异步任务的方式运行，也支持多个驱动程序在同一个宿主上运行。
- **以类型系统为基础的任务调度**：任务的状态用Rust的类型系统表示，而不是一个状态变量，并且不需要依赖引用计数指针。正在运行的任务由调度器拥有，而正在等待（堵塞）的任务则由futex引用或者挂起令牌拥有。这样可以减小代码复杂度。
- **非全局的隔离虚拟文件系统**：（借鉴自Fuchsia）每一个进程有自己的VFS，也可以自由选择是否将自己的VFS共享给自己的子进程。

# 路线图

## 正在实现的

- [ ] 完成DDK。
- [ ] 实现基础的驱动（PCI，ACPI等）。

## 之后要做的（可能会改）

- 实现存储设备的驱动。
- 实现一些文件系统（FAT等）。
- 完成设备管理进程和程序管理进程的实现。
- 进入Rust标准库。

# 代码结构

- `debug` - 存储反汇编文件、调试符号表、二进制文件信息和虚拟机的串口记录文件。
- `h2o` - 存储内核的源代码。
- `scripts` - 存储构建项目需要的脚本。
- `src` - 存储整个项目的库和程序代码。
- `target` - 存储生成的二进制和虚拟硬盘映像文件。
- `xtask` - 存储这个项目的构建程序。

# 从源码构建

## Linux

1. 安装Rust和其他依赖（以Ubuntu为例）：
   ```sh
   # 配置 Rust 时需要选择 nightly 通道
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   sudo apt install build-essential qemu-system-x86 llvm-14 nasm
   export LLVM_PATH="/usr/lib/llvm-14"
   ```

2. 添加下列目标：
   ```sh
   rustup target add x86_64-unknown-linux-gnu
   ```

3. 切换到项目的根目录，运行以下命令：
   ```sh
   cargo xtask dist img
   ```

4. 运行以下命令以在qemu上运行：
   ```sh
   sh scripts/run.sh qemu N # N为CPU核心数
   ```
   查看`debug/qemu.log`文件，就能看到操作系统的输出。

5. 运行以下命令以在qemu上调试:
   ```sh
   sh scripts/run.sh qdbg N # 同上
   ```
   然后打开新终端：
   ```sh
   # 切换到项目的根目录
   gdb debug/FOO.sym
   # FOO是想要调试的二进制文件对应的名称，
   # 可以先浏览debug目录看看。

   # 在gdb里：
   target remote :1234
   ```
   然后就可以设置断点（以KERNEL.sym为例）：
   ```sh
   b kmain
   c
   ```

6. 如果你想要用其他虚拟机运行项目，先查看run.sh，然后手动创建虚拟机的配置文件。不要忘了添加生成的虚拟硬盘和串口文件，否则会看不到输出！

# 贡献

如果想要贡献源码或其他，请先联系我。
* 电子邮件: [akucxy@163.com](mailto:akucxy@163.com)
* QQ: 2534027071
