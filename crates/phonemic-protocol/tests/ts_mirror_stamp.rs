//! 任务 2.5：Rust 端对协议镜像 stamp 的最小校验。
//!
//! 设计来源：design.md §5
//!
//! `scripts/gen-ts-types.mjs` 在每次刷新时把 Rust 与 TS 两侧的 fingerprint
//! 写入 `apps/mobile/src/protocol/.protocol-stamp.json`。本测试做两件事：
//!
//! 1. 找到 stamp 文件（沿 `CARGO_MANIFEST_DIR` 向上查找，对开发者本地与
//!    CI 都鲁棒）；如果文件不存在（极少见的初次脚手架场景），仅打印诊断
//!    信息，避免阻塞编译。
//! 2. 校验 stamp 内的 `protocolVersion` 与 [`phonemic_protocol::PROTOCOL_VERSION`]
//!    完全一致，把 Rust 与 JSON 文件之间的版本漂移在 `cargo test` 里立刻拦截。
//!
//! 该测试只读取本地文件，不依赖网络或 `git`；可在离线 CI 中正常运行。

use std::fs;
use std::path::{Path, PathBuf};

const STAMP_REL_PATH: &str = "apps/mobile/src/protocol/.protocol-stamp.json";

/// 沿 manifest dir 一路向上找仓库根（含 `.protocol-stamp.json` 的祖先）。
fn locate_stamp() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut current: Option<&Path> = Some(manifest_dir);
    while let Some(dir) = current {
        let candidate = dir.join(STAMP_REL_PATH);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// 极简提取：只读取 `"protocolVersion": "<value>"`，避免引入 `serde_json` 的 dev-dep。
fn extract_protocol_version(json: &str) -> Option<String> {
    const KEY: &str = "\"protocolVersion\"";
    let start = json.find(KEY)?;
    let after_key = &json[start + KEY.len()..];
    let colon = after_key.find(':')?;
    let after_colon = &after_key[colon + 1..];
    let first_quote = after_colon.find('"')?;
    let rest = &after_colon[first_quote + 1..];
    let end_quote = rest.find('"')?;
    Some(rest[..end_quote].to_owned())
}

#[test]
fn protocol_stamp_version_matches_rust() {
    let Some(stamp_path) = locate_stamp() else {
        // 初次脚手架场景：stamp 文件可能尚未生成。打印提示而非失败。
        eprintln!(
            "skipping stamp check: {STAMP_REL_PATH} not found above CARGO_MANIFEST_DIR"
        );
        return;
    };

    let json = fs::read_to_string(&stamp_path)
        .unwrap_or_else(|e| panic!("read stamp file {}: {e}", stamp_path.display()));
    let version = extract_protocol_version(&json)
        .unwrap_or_else(|| panic!("missing `protocolVersion` in stamp: {json}"));

    assert_eq!(
        version,
        phonemic_protocol::PROTOCOL_VERSION,
        "stamp file `{}` declares protocolVersion={:?} but Rust constant is {:?}; \
         run `pnpm gen:ts-types` after updating either side",
        stamp_path.display(),
        version,
        phonemic_protocol::PROTOCOL_VERSION,
    );
}

#[test]
fn extract_protocol_version_handles_typical_stamp_shape() {
    let sample = r#"{
  "$note": ["foo"],
  "protocolVersion": "1",
  "fingerprints": { "rust": "abc", "ts": "def" }
}
"#;
    assert_eq!(extract_protocol_version(sample).as_deref(), Some("1"));
}
