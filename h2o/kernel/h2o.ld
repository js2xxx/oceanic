OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)

ENTRY(kentry)

SECTIONS
{
      . = 0xffffffff80001000;

      .text :
      {
            *(.text.init*)
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
            TDATA_START = .;
            *(.tdata*)
            TBSS_START = .;
            *(.tbss*)
      }

      .got ALIGN (4K) :
      {
            *(.got*)
      }

      .bss ALIGN (4K) :
      {
            *(.bss*)
            . = ALIGN (1M);
            . += 1M;
            INIT_STACK = .;
      }

      .dynsym ALIGN (4K) : { *(.dynsym) }
      .gnu.hash : { *(.gnu.hash) }
      .hash : { *(.hash) }
      .dynstr : { *(.dynstr) }
      .rela.dyn : { *(.rela.dyn) }

	/DISCARD/ :
	{
		*(.note)
		*(.comment)
		*(.eh_frame)
	}
}