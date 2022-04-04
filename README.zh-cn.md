# 海洋OS：一个个人电脑操作系统

[![996.icu](https://img.shields.io/badge/link-996.icu-red.svg)](https://996.icu)

**警告:** 这个项目还在非常早的开发阶段，操作系统尚未准备好用户程序环境。使用者或开发者应该考虑任何在虚拟机或裸机中运行的风险。

当前这个项目只支持x86_64架构。未来可能会支持aarch64。

# 代码结构

- `debug` - 存储反汇编文件、调试符号表、二进制文件信息和虚拟机的串口记录文件。
- `h2o` - 存储内核的源代码。
- `lib` - 存储整个项目需要的库代码。
- `scripts` - 存储构建项目需要的脚本。
- `target` - 存储生成的二进制和虚拟硬盘映像文件。
- `xtask` - 存储这个项目的构建程序。

# 从源码构建

## Linux

1. 安装Rust和其他依赖（以Ubuntu为例）：
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   bash -c "$(wget -O - https://apt.llvm.org/llvm.sh)"
   sudo apt install build-essential qemu-system-x86
   ```

2. 添加下列工作链：
   ```sh
   rustup add toolchain nightly-x86_64-unknown-linux-gnu
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
