all : lib

TARGET_LIB:=librv003usb
CH32FUN:=../ch32v003fun/ch32fun
TARGET_MCU:=CH32V003

ADDITIONAL_C_FILES_SRC=../rv003usb/rv003usb.S ../rv003usb/rv003usb.c
EXTRA_CFLAGS:=-I../lib -I../rv003usb -fno-lto

# List of source files to compile
# DO NOT link in ch32fun.c, as it contains another handle_reset which is linked
# instead of the qingke-rt one and breaks interrupts
C_SOURCES = $(filter %.c, $(ADDITIONAL_C_FILES_SRC))
S_SOURCES = $(filter %.S, $(ADDITIONAL_C_FILES_SRC))

# Corresponding object files
C_OBJECTS = $(C_SOURCES:.c=.o)
S_OBJECTS = $(S_SOURCES:.S=_s.o)
OBJECTS = $(C_OBJECTS) $(S_OBJECTS)

include $(CH32FUN)/ch32fun.mk

lib : $(TARGET_LIB).a

$(TARGET_LIB).a : $(OBJECTS)
	$(PREFIX)-ar rcs $@ $^

%.o : %.c
	$(PREFIX)-gcc -c $(CFLAGS) $< -o $@

%_s.o : %.S
	$(PREFIX)-gcc -c $(CFLAGS) $< -o $@

flash : cv_flash
clean : cv_clean
