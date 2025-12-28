cargo +nightly objcopy --release -- -O binary rust_usb.bin && ../ch32v003fun/minichlink/minichlink -w rust_usb.bin flash -b
