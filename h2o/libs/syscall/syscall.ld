ENTRY(sv_task_exit)

SECTIONS
{
    . = SIZEOF_HEADERS;
    .note.gnu.build-id  : { *(.note.gnu.build-id) } :note

    .text                   : { *(.text*) }     :load
    .rodata ALIGN (0x10)    : { *(.rodata*) }   :load
    .dynamic                : { *(.dynamic*) }  :load :dynamic

    /DISCARD/ : { *(.comment*) }
}

PHDRS
{
    phdr PT_PHDR PHDRS FLAGS(4);
    load PT_LOAD FILEHDR FLAGS(5);
    dynamic PT_DYNAMIC FLAGS(4);
    note PT_NOTE FLAGS(4);
}