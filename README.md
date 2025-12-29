# A rust "port" of rv003usb, hacky

Port assembly to be inline assembly with generics, basically using rustc as a replacement for the C preprocessor and port everything apart from the core send and receive functions to rust. Only implements HANDLE_IN_REQUEST and USE_REBOOT_FEATURE_REPORT. 

## How do I use this?

Don't. If you *really* want to use rv003usb with rust, the best method is using `rv003usb` with C, compiling it separately and linking it. You need to take care of calling the *correct* interrupt, ch32-rs has a different handler. 

cargo +nightly objcopy --release -- -O binary rust_usb.bin && ../ch32v003fun/minichlink/minichlink -w rust_usb.bin flash -b
