make

cargo +nightly objcopy  --release -- -O binary demo.bin

sudo ../ch32v003fun/minichlink/minichlink -w demo.bin flash -b
