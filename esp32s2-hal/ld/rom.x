REGION_ALIAS("ROTEXT", irom_seg);
REGION_ALIAS("RWTEXT", iram_seg);
REGION_ALIAS("RODATA", drom_seg);
REGION_ALIAS("RWDATA", dram_seg);


PROVIDE ( uart_tx_one_char = 0x40012b10 );