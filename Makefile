all : lib

TARGET_LIB:=librv003usb

# List of source files to compile
# DO NOT link in ch32fun.c, as it contains another handle_reset which is linked
# instead of the qingke-rt one and breaks interrupts
C_SOURCES = ./rv003usb/rv003usb.c
S_SOURCES = ./rv003usb/rv003usb.S

# Corresponding object files
C_OBJECTS = $(C_SOURCES:.c=.o)
S_OBJECTS = $(S_SOURCES:.S=_s.o)
OBJECTS = $(C_OBJECTS) $(S_OBJECTS)

PREFIX?=riscv64-elf
CFLAGS?=-g -Os -ffunction-sections -fdata-sections -fmessage-length=0 -msmall-data-limit=8 \
	-march=rv32ec_zicsr -mabi=ilp32e \
	-static-libgcc \
	-nostdlib \
	-I. -Wall


lib : $(TARGET_LIB).a

$(TARGET_LIB).a : $(OBJECTS)
	$(PREFIX)-ar rcs $@ $^

%.o : %.c
	$(PREFIX)-gcc -c $(CFLAGS) $< -o $@

%_s.o : %.S
	$(PREFIX)-gcc -c $(CFLAGS) $< -o $@

clean:
	rm $(TARGET_LIB).a $(OBJECTS)
