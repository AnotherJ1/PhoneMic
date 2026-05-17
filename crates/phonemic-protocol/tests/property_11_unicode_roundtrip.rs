// Feature: phone-mic-voice-input, Property 11: 文本协议 Unicode round-trip
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §5.1 协议结构
//   - §7 Property 11
//   - §9.2 属性测试规范（共享生成器 `unicode_text()`）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 2.6
//
// **Validates: Requirements 5.5**
//
// 本测试文件覆盖客户端 / 服务端两侧最重要的「文本承载消息」：
//   - `text.submit`     —— Property 11 主体
//   - `text.preview`    —— interim 中间态文本
//   - `transcript.final`—— Server_ASR 最终识别结果
//   - `error` / `inject.error` —— `code` + `message` 字段（含 Unicode 描述）
//
// 共享生成器 `unicode_text()` 按 §9.2 的要求生成
//   - ASCII 可打印区段
//   - 中文（CJK 统一汉字 U+4E00..=U+9FFF）
//   - emoji 区段（U+1F300..=U+1FAFF）
//   - 全角标点（U+3000..=U+303F、U+FF00..=U+FFEF）
//   - 控制字符（U+0000..=U+001F、U+007F）
//
// 这是任务 2.6 在 §9.2 中显式要求的"shared generator"。

use phonemic_protocol::ws::*;
use phonemic_protocol::PROTOCOL_VERSION;
use proptest::prelude::*;

// ---------- 共享 char 生成器 ----------

/// 单个字符生成器：在多种 Unicode 区段中均匀混合。
///
/// 实现细节：
/// - 使用 `prop::char::range` 直接产出落在合法范围内的 [`char`]，
///   `char` 类型本身就排除了 surrogate（U+D800..=U+DFFF），无需额外过滤。
/// - emoji 区段上界 0x1FAFF 合法（位于补充多文种平面）。
fn any_unicode_char() -> impl Strategy<Value = char> {
    prop_oneof![
        // ASCII 可打印（含空格）
        4 => prop::char::range(' ', '~'),
        // 中文（CJK 统一汉字）
        4 => prop::char::range('\u{4E00}', '\u{9FFF}'),
        // emoji（杂项符号 / 表情 / 交通运输 / 等）
        3 => prop::char::range('\u{1F300}', '\u{1FAFF}'),
        // 全角 / CJK 标点
        2 => prop::char::range('\u{3000}', '\u{303F}'),
        // 半 / 全角形式
        2 => prop::char::range('\u{FF00}', '\u{FFEF}'),
        // 控制字符 0x00..=0x1F
        1 => prop::char::range('\u{0000}', '\u{001F}'),
        // DEL
        1 => Just('\u{007F}'),
    ]
}

/// 字符串生成器：长度 0..=256 的 Unicode 字符串。
///
/// 与设计 §9.2 的 `unicode_text()` 共享生成器对齐：
/// 既覆盖空 / 单字符等边界，也包含足够长（256 字符）的样本。
fn unicode_text() -> impl Strategy<Value = String> {
    prop::collection::vec(any_unicode_char(), 0..256)
        .prop_map(|chars| chars.into_iter().collect::<String>())
}

// ---------- proptest 运行参数 ----------

/// 任务 2.6 要求每个 PBT 至少 256 cases；§9.2 要求 ≥ 100。
///
/// 通过 mutate `ProptestConfig::default()` 而非 struct literal 构造，
/// 避免 proptest 升级引入新字段（[`ProptestConfig`] 标注了 `#[non_exhaustive]`）。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- 属性测试 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 11 主体：`text.submit` 经 JSON round-trip 后所有字段保持不变。
    ///
    /// **Validates: Requirements 5.5**
    #[test]
    fn text_submit_payload_round_trip(
        text in unicode_text(),
        lang in "[a-zA-Z\\-]{2,8}",
        ts in 0i64..1_000_000_000_000,
    ) {
        // text.submit 的 validate() 不允许空 text（Property 11 关注非空文本的
        // 序列化往返，与协议层校验保持一致）。
        prop_assume!(!text.is_empty());

        let original = ClientMessage::text_submit("m-1", text.clone(), lang.clone(), ts);
        // 构造时 validate 必须通过；否则我们生成了非法消息。
        prop_assert!(original.validate().is_ok());

        let serialized = serde_json::to_string(&original)
            .expect("text.submit serialize");
        let parsed: ClientMessage = serde_json::from_str(&serialized)
            .expect("text.submit deserialize");

        prop_assert_eq!(&parsed, &original);

        // 进一步断言关键字段确实落在 payload 内（防止 flatten 错位）。
        if let ClientMessageKind::TextSubmit(p) = &parsed.kind {
            prop_assert_eq!(&p.text, &text);
            prop_assert_eq!(&p.lang, &lang);
            prop_assert_eq!(p.client_ts, ts);
        } else {
            prop_assert!(false, "kind mismatch after round-trip");
        }
    }
}

proptest! {
    #![proptest_config(pbt_config())]

    /// `text.preview`（interim 中间态文本）的 Unicode round-trip。
    ///
    /// **Validates: Requirements 5.5**
    #[test]
    fn text_preview_round_trip(text in unicode_text()) {
        prop_assume!(!text.is_empty());

        let original = ClientMessage::text_preview(text.clone());
        prop_assert!(original.validate().is_ok());

        let serialized = serde_json::to_string(&original)
            .expect("text.preview serialize");
        let parsed: ClientMessage = serde_json::from_str(&serialized)
            .expect("text.preview deserialize");

        prop_assert_eq!(&parsed, &original);

        if let ClientMessageKind::TextPreview(p) = &parsed.kind {
            prop_assert_eq!(&p.text, &text);
            // 协议要求 interim 永远为 true（任务 2.1 中实现）
            prop_assert!(p.interim);
        } else {
            prop_assert!(false, "kind mismatch after round-trip");
        }
    }
}

proptest! {
    #![proptest_config(pbt_config())]

    /// `transcript.final`（Server_ASR 最终结果）的 Unicode round-trip。
    ///
    /// **Validates: Requirements 5.5**
    #[test]
    fn transcript_final_round_trip(
        text in unicode_text(),
        lang in "[a-zA-Z\\-]{2,8}",
    ) {
        prop_assume!(!text.is_empty());

        let original = ServerMessage::transcript_final(text.clone(), lang.clone(), None);
        prop_assert!(original.validate().is_ok());

        let serialized = serde_json::to_string(&original)
            .expect("transcript.final serialize");
        let parsed: ServerMessage = serde_json::from_str(&serialized)
            .expect("transcript.final deserialize");

        prop_assert_eq!(&parsed, &original);

        if let ServerMessageKind::TranscriptFinal(p) = &parsed.kind {
            prop_assert_eq!(&p.text, &text);
            prop_assert_eq!(&p.lang, &lang);
        } else {
            prop_assert!(false, "kind mismatch after round-trip");
        }
    }
}

proptest! {
    #![proptest_config(pbt_config())]

    /// `error` 与 `inject.error` 中的 Unicode `message` 字段 round-trip。
    ///
    /// 与文本消息不同，错误描述允许为空字符串（任务 2.1 中 `ErrorPayload::validate`
    /// 仅要求 `code` 非空）；因此本测试不对 `message` 做 `prop_assume!` 过滤。
    ///
    /// **Validates: Requirements 5.5**
    #[test]
    fn error_messages_round_trip(
        code in "[A-Z_]{3,32}",
        message in unicode_text(),
    ) {
        // 通用 error 消息
        let err = ServerMessage::error(code.clone(), message.clone());
        prop_assert!(err.validate().is_ok());
        let s = serde_json::to_string(&err).expect("error serialize");
        let back: ServerMessage = serde_json::from_str(&s).expect("error deserialize");
        prop_assert_eq!(&back, &err);
        if let ServerMessageKind::Error(p) = &back.kind {
            prop_assert_eq!(&p.code, &code);
            prop_assert_eq!(&p.message, &message);
        } else {
            prop_assert!(false, "error kind mismatch");
        }

        // 带 id 的 inject.error 消息
        let ie = ServerMessage::inject_error("m-1", code.clone(), message.clone());
        prop_assert!(ie.validate().is_ok());
        let s2 = serde_json::to_string(&ie).expect("inject.error serialize");
        let back2: ServerMessage = serde_json::from_str(&s2).expect("inject.error deserialize");
        prop_assert_eq!(&back2, &ie);
        if let ServerMessageKind::InjectError(p) = &back2.kind {
            prop_assert_eq!(&p.id, "m-1");
            prop_assert_eq!(&p.code, &code);
            prop_assert_eq!(&p.message, &message);
        } else {
            prop_assert!(false, "inject.error kind mismatch");
        }
    }
}

// ---------- 兜底：固定字符串的 round-trip 健壮性 sanity 检查 ----------

/// 用一条同时包含中文 / emoji / 全角标点 / 控制字符的固定字符串，
/// 确保即便 proptest 的随机种子全部退化也能命中 §9.2 要求的字符类别。
///
/// 这条测试与 `text_submit_payload_round_trip` 共享 round-trip 逻辑，
/// 但作为普通 `#[test]` 直接执行，便于在没有 proptest 时也能给出快速反馈。
#[test]
fn sanity_known_mixed_string_round_trips() {
    let text = "中文 🌍 ！全角 \u{1}";
    let original = ClientMessage::text_submit("m-known", text, "zh-CN", 1_700_000_000_000);
    assert!(original.validate().is_ok());

    let serialized = serde_json::to_string(&original).expect("serialize");
    let parsed: ClientMessage = serde_json::from_str(&serialized).expect("deserialize");
    assert_eq!(parsed, original);

    if let ClientMessageKind::TextSubmit(p) = &parsed.kind {
        assert_eq!(p.text, text);
        assert_eq!(p.lang, "zh-CN");
        assert_eq!(p.client_ts, 1_700_000_000_000);
    } else {
        panic!("kind mismatch");
    }

    // 协议版本号不被本测试改写，仅在此处做轻量化引用，避免 unused import 警告。
    let _ = PROTOCOL_VERSION;
}
