fn main() {
    // 纯 Rust 核心无需额外 framework 链接。
    // 如需在 Swift 侧调用 NSWorkspace，由 Swift 直接 import AppKit 即可。
    println!("cargo:rerun-if-changed=build.rs");
}
