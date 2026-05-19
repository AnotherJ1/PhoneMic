//! 二维码内容编码 / SVG 渲染（任务 6.3）。
//!
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.7 / §4.1
//! - 任务来源：`tasks.md` 6.3 / 6.4
//!
//! 编码格式：`phonemic://pair?u=<base64url(connectUrl)>&c=<pairingCode>`。
//! 错误纠正等级：`M`。
//!
//! 解码桥（仅 `cfg(test)`）`qr_decode_for_test`：使用 `rqrr` 在
//! resvg 渲染出的 PNG 像素图上解码，验证编码 round-trip。

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// 把 `(connect_url, pairing_code)` 编码为 `phonemic://pair?u=…&c=…`。
#[must_use]
pub fn build_pair_url(connect_url: &str, pairing_code: &str) -> String {
    let u = URL_SAFE_NO_PAD.encode(connect_url.as_bytes());
    format!("phonemic://pair?u={u}&c={pairing_code}")
}

/// 反向解析 `phonemic://pair?u=…&c=…`，仅在结构合法且 base64url 解码成功时
/// 返回 `(connect_url, pairing_code)`。
#[must_use]
pub fn parse_pair_url(s: &str) -> Option<(String, String)> {
    let body = s.strip_prefix("phonemic://pair?")?;
    let mut u: Option<&str> = None;
    let mut c: Option<&str> = None;
    for kv in body.split('&') {
        if let Some(rest) = kv.strip_prefix("u=") {
            u = Some(rest);
        } else if let Some(rest) = kv.strip_prefix("c=") {
            c = Some(rest);
        }
    }
    let u_decoded = URL_SAFE_NO_PAD.decode(u?.as_bytes()).ok()?;
    let url = String::from_utf8(u_decoded).ok()?;
    let code = c?.to_owned();
    Some((url, code))
}

/// 渲染二维码为 SVG 字符串（错误纠正等级 M）。
///
/// # Errors
///
/// `qrcode::QrCode::with_error_correction_level` 失败时返回错误。
pub fn qr_encode(connect_url: &str, pairing_code: &str) -> Result<String, QrError> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    let payload = build_pair_url(connect_url, pairing_code);
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M)
        .map_err(|e| QrError::Encode(e.to_string()))?;
    let svg = code
        .render::<svg::Color<'_>>()
        .min_dimensions(256, 256)
        .build();
    Ok(svg)
}

/// QR 渲染 / 解码失败原因。
#[derive(Debug, thiserror::Error)]
pub enum QrError {
    #[error("qr encode failed: {0}")]
    Encode(String),
    #[error("qr decode failed: {0}")]
    Decode(String),
}

#[cfg(test)]
pub mod test_decode {
    //! 测试桥：把 SVG 渲染为 PNG 像素，再用 rqrr 解码。
    //!
    //! 仅在测试与 `test-decode` feature 开启时编译，避免把 resvg / rqrr 等
    //! 重量级依赖泄漏到生产二进制。

    use super::QrError;
    use rqrr::PreparedImage;

    /// 把 SVG 字符串栅格化并解码出第一段 QR 文本。
    pub fn qr_decode_for_test(svg: &str) -> Result<String, QrError> {
        // 1. 解析 SVG。
        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_system_fonts();
        let tree = usvg::Tree::from_str(svg, &opt)
            .map_err(|e| QrError::Decode(format!("usvg parse: {e}")))?;
        let size = tree.size().to_int_size();
        let (w, h) = (size.width().max(64), size.height().max(64));
        let mut pixmap = tiny_skia::Pixmap::new(w, h)
            .ok_or_else(|| QrError::Decode("pixmap alloc".into()))?;
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

        // 2. 转灰度图供 rqrr 使用。
        let mut luma = vec![0u8; (w as usize) * (h as usize)];
        for (i, px) in pixmap.pixels().iter().enumerate() {
            // tiny_skia::PremultipliedColorU8 → 反推 (r,g,b)
            let r = px.red();
            let g = px.green();
            let b = px.blue();
            // 简单 Rec.601 灰度公式。
            let y = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
            luma[i] = y.min(255) as u8;
        }
        let mut img = PreparedImage::prepare_from_greyscale(w as usize, h as usize, |x, y| {
            luma[y * (w as usize) + x]
        });
        let grids = img.detect_grids();
        let grid = grids
            .into_iter()
            .next()
            .ok_or_else(|| QrError::Decode("no QR grid detected".into()))?;
        let (_, content) = grid
            .decode()
            .map_err(|e| QrError::Decode(format!("rqrr decode: {e}")))?;
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_pair_url_round_trip() {
        let url = "https://192.168.1.10:18080";
        let code = "ABCD2345";
        let s = build_pair_url(url, code);
        let (parsed_url, parsed_code) = parse_pair_url(&s).expect("parse");
        assert_eq!(parsed_url, url);
        assert_eq!(parsed_code, code);
    }

    #[test]
    fn parse_rejects_unknown_scheme() {
        assert!(parse_pair_url("https://x?u=AA&c=B").is_none());
    }

    #[test]
    fn qr_encode_returns_svg_string() {
        let svg = qr_encode("http://192.168.1.10:18080", "ABCD2345").expect("encode");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn qr_round_trip_via_decode_bridge() {
        let url = "http://192.168.1.10:18080";
        let code = "ABCD2345";
        let svg = qr_encode(url, code).expect("encode");
        let payload = test_decode::qr_decode_for_test(&svg).expect("decode");
        let expected = build_pair_url(url, code);
        assert_eq!(payload, expected);
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 4: 二维码内容 round-trip
//
// 任务 6.4：任意"合法 URL + Pairing_Code" → encode → decode → 与原值一致。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn url_strategy() -> impl Strategy<Value = String> {
        // 限制为 LAN URL 形态：scheme + ip + 端口（与 design §3.7 一致）。
        (
            prop::sample::select(vec!["http", "https"]),
            (1u8..=192).prop_flat_map(|a| {
                (
                    Just(a),
                    0u8..=255,
                    0u8..=255,
                    1u8..=254,
                    1024u16..=65535,
                )
            }),
        )
            .prop_map(|(scheme, (a, b, c, d, port))| {
                format!("{scheme}://{a}.{b}.{c}.{d}:{port}")
            })
    }

    fn code_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(b"ABCDEFGHJKMNPQRSTUVWXYZ23456789".to_vec()), 8)
            .prop_map(|bytes| String::from_utf8(bytes).unwrap())
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 4: 二维码内容 round-trip
        #[test]
        fn property_4_pair_url_round_trip(url in url_strategy(), code in code_strategy()) {
            let s = build_pair_url(&url, &code);
            let (parsed_url, parsed_code) = parse_pair_url(&s).expect("parse");
            prop_assert_eq!(parsed_url, url);
            prop_assert_eq!(parsed_code, code);
        }

        // 二维码 round-trip：encode → SVG → decode → 同一 payload。
        #[test]
        fn property_4_qr_svg_round_trip(url in url_strategy(), code in code_strategy()) {
            let svg = qr_encode(&url, &code).expect("encode");
            let payload = test_decode::qr_decode_for_test(&svg).expect("decode");
            let expected = build_pair_url(&url, &code);
            prop_assert_eq!(payload, expected);
        }
    }
}
