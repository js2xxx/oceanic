OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)

SECTIONS
{
      . = 0x401000;
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

      .tdata ALIGN (4K) :
      {
            *(.tdata*)
      }

      .tbss ALIGN (4K) :
      {
            *(.tbss*)
      }

      .got ALIGN (4K) :
      {
            *(.got*)
      }

	/DISCARD/ :
	{
		*(.note)
		*(.comment)
		*(.eh_frame)
	}
}