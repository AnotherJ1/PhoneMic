// 任务 14.2：把 Cargo 包版本注入为编译期环境变量 `PHONEMIC_PROTOCOL_VERSION`，
// 供 `lib.rs::VERSION` 通过 `env!` 取用。CI 在 release 流水线中校验 git tag
// 与该常量一致，避免 release 出 mismatch 的二进制。
fn main() {
    let v = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    println!("cargo:rustc-env=PHONEMIC_PROTOCOL_VERSION={v}");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
