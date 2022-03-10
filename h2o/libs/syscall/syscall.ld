SECTIONS
{
    .note.gnu.build-id  : { *(.note.gnu.build-id) }

    .hash       : { *(.hash) } :text
    .gnu.hash   : { *(.gnu.hash) }
    .dynsym     : { *(.dynsym) }
    .dynstr     : { *(.dynstr) }
    .text       : { *(.text*) }
    .dynamic    : { *(.dynamic*) }

    /DISCARD/ : { *(.comment*) }
}

PHDRS
{
    text PT_LOAD FLAGS(5);
    dynamic PT_DYNAMIC FLAGS(4);
    note PT_NOTE FLAGS(4);
}