fn main() {
    println!("cargo:rustc-link-search=./");
    println!("cargo::rustc-link-lib=static:+whole-archive=rv003usb");
    println!("cargo:rustc-link-arg-bins=-Tlink_usb.x");
}
