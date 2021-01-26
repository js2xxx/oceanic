OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)

ENTRY(kentry)

SECTIONS
{
      . = 0xffffffff80001000;

      .text :
      {
            *(.text*)
      }

      .rodata ALIGN (4K) :
      {
            *(.rodata*)
      }

      .data ALIGN (4K) :
      {
            *(.data*)
      }

      .tbss ALIGN (4K) :
      {
            *(.tbss*)
      }

      .bss ALIGN (4K) :
      {
            *(.bss*)
            . += 24K;
            . = ALIGN (0x10);
            INIT_STACK = .;
      }

	/DISCARD/ :
	{
		*(.note)
		*(.comment)
		*(.eh_frame)
	}
}