//! 桌面端 i18n 字典与 locale → lang 决策。
//!
//! 任务来源：tasks.md 3.22；设计来源：design.md §4.1。
//! 需求：R8.1（至少支持 zh-CN / en-US 两份桌面端字典）、R8.3（首次启动按
//! OS 区域自动选择默认界面语言）。
//!
//! 本模块对外只暴露：
//! - [`Lang`]：界面语言枚举（仅 `zh-CN`、`en-US` 两种取值）；
//! - [`decide_lang`]：把任意 locale 字符串映射为 [`Lang`]，主语言子标签为
//!   `zh` ⇒ [`Lang::ZhCN`]，否则一律回退 [`Lang::EnUS`]；
//! - [`dict_for`]：拿到目标语言的 key → value 字典（懒加载、进程级缓存）；
//! - [`t`]：按 key 查文案，缺失时返回 `None` 并打印 `tracing::warn!`。
//!
//! 字典本身通过 `include_str!` 内嵌进二进制，避免发布后字典文件丢失。
//! 字典完整性（zh-CN / en-US key 集合相等且无空字符串）由独立任务
//! 3.24 的单元测试负责验证；本文件仅在测试中做基本 sanity check。

use std::collections::HashMap;
use std::sync::OnceLock;

/// 桌面端支持的界面语言。
///
/// 设计明确只支持简体中文与英文（design.md §4.1、requirements.md R8.1），
/// 因此使用闭枚举即可，未来若需扩展再以 `non_exhaustive` 收敛兼容性影响。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// 简体中文（zh-CN）。
    ZhCN,
    /// 英文（en-US），默认回退语言。
    EnUS,
}

impl Lang {
    /// 返回与 BCP-47 标签一致的稳定字符串：`"zh-CN"` 或 `"en-US"`。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::ZhCN => "zh-CN",
            Lang::EnUS => "en-US",
        }
    }

    /// 把 `"zh-CN"` / `"en-US"` 反解析为 [`Lang`]，大小写不敏感。
    ///
    /// 与 [`decide_lang`] 不同：本函数不做"主子标签"近似匹配，仅识别这两个
    /// 完整字符串。用于读取持久化设置（`config.toml` 中的 `language` 字段）
    /// 或前端透传的精确取值。其它输入返回 `None`，由调用方决定是否回退。
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("zh-CN") {
            Some(Lang::ZhCN)
        } else if s.eq_ignore_ascii_case("en-US") {
            Some(Lang::EnUS)
        } else {
            None
        }
    }
}

/// 根据 OS / 浏览器提供的 locale 字符串决策默认 UI 语言。
///
/// 规则（与 design.md §7 Property 24 等价）：
/// 1. 取首个 `-` 或 `_` 之前的子串作为"主语言子标签"；
/// 2. 主子标签忽略大小写后等于 `"zh"` ⇒ [`Lang::ZhCN`]；
/// 3. 其它一切情况（包括空串、纯空白、无法识别） ⇒ [`Lang::EnUS`]。
///
/// 该函数为纯函数：相同输入恒得相同输出，无 IO、无可见副作用。
#[must_use]
pub fn decide_lang(locale: &str) -> Lang {
    let trimmed = locale.trim();
    if trimmed.is_empty() {
        return Lang::EnUS;
    }
    // 主语言子标签 = 第一个 `-` 或 `_` 之前的部分（BCP-47 用 `-`，
    // POSIX locale 如 `zh_CN.UTF-8` 用 `_`，两种都要兼容）。
    let primary = trimmed
        .split(|c: char| c == '-' || c == '_')
        .next()
        .unwrap_or("");
    if primary.eq_ignore_ascii_case("zh") {
        Lang::ZhCN
    } else {
        Lang::EnUS
    }
}

// 内嵌字典的原始 JSON。把 `include_str!` 写在常量上便于在编译期直接报错
// （字典缺失时 `cargo build` 立即失败）。
const ZH_CN_RAW: &str = include_str!("i18n_dict/zh-CN.json");
const EN_US_RAW: &str = include_str!("i18n_dict/en-US.json");

// 进程级缓存：每种语言只解析一次。键 / 值通过 `Box::leak` 提升到 `'static`，
// 这样调用方拿到的 `&'static str` 在整个进程生命周期内有效，且总泄漏量
// 等于字典字节数（O(KB)），可接受。
static ZH_CN_DICT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static EN_US_DICT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn build_dict(raw: &str, lang_tag: &'static str) -> HashMap<&'static str, &'static str> {
    // 字典是内嵌资源，解析失败属于编译期就可发现的开发者错误，
    // 用 `expect` 直接停服更利于在 CI 上暴露问题。
    let parsed: HashMap<String, String> = serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("内嵌 i18n 字典 {lang_tag} 解析失败：{e}"));
    let mut out = HashMap::with_capacity(parsed.len());
    for (k, v) in parsed {
        let k_static: &'static str = Box::leak(k.into_boxed_str());
        let v_static: &'static str = Box::leak(v.into_boxed_str());
        out.insert(k_static, v_static);
    }
    out
}

/// 取得 `lang` 对应的字典引用（首次调用时按需解析并缓存）。
#[must_use]
pub fn dict_for(lang: Lang) -> &'static HashMap<&'static str, &'static str> {
    match lang {
        Lang::ZhCN => ZH_CN_DICT.get_or_init(|| build_dict(ZH_CN_RAW, "zh-CN")),
        Lang::EnUS => EN_US_DICT.get_or_init(|| build_dict(EN_US_RAW, "en-US")),
    }
}

/// 按 key 查文案。缺失时返回 `None` 并打印 `tracing::warn!`，
/// 由调用方决定是否回退到 key 本身或英文字典。
#[must_use]
pub fn t(lang: Lang, key: &str) -> Option<&'static str> {
    let value = dict_for(lang).get(key).copied();
    if value.is_none() {
        tracing::warn!(lang = lang.as_str(), key, "i18n key 缺失");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- decide_lang ----

    #[test]
    fn decide_lang_primary_zh_maps_to_zh_cn() {
        assert_eq!(decide_lang("zh"), Lang::ZhCN);
        assert_eq!(decide_lang("zh-CN"), Lang::ZhCN);
        assert_eq!(decide_lang("zh-TW"), Lang::ZhCN); // 主子标签 zh 即可
        assert_eq!(decide_lang("zh_TW"), Lang::ZhCN); // 兼容 POSIX 下划线
        assert_eq!(decide_lang("ZH-cn"), Lang::ZhCN); // 大小写不敏感
    }

    #[test]
    fn decide_lang_other_primary_falls_back_to_en_us() {
        assert_eq!(decide_lang("en"), Lang::EnUS);
        assert_eq!(decide_lang("en-US"), Lang::EnUS);
        assert_eq!(decide_lang("fr"), Lang::EnUS);
        assert_eq!(decide_lang("ja-JP"), Lang::EnUS);
    }

    #[test]
    fn decide_lang_empty_or_whitespace_falls_back_to_en_us() {
        assert_eq!(decide_lang(""), Lang::EnUS);
        assert_eq!(decide_lang("   "), Lang::EnUS);
        assert_eq!(decide_lang("\t\n"), Lang::EnUS);
    }

    #[test]
    fn decide_lang_is_deterministic() {
        // Property 24 的核心要求：同一输入多次调用结果一致。
        for input in ["zh-CN", "en-US", "fr", "", "zh_TW", "ZH-cn"] {
            assert_eq!(decide_lang(input), decide_lang(input));
        }
    }

    // ---- Lang::from_str / as_str ----

    #[test]
    fn lang_from_str_recognises_exact_tags() {
        assert_eq!(Lang::from_str("zh-CN"), Some(Lang::ZhCN));
        assert_eq!(Lang::from_str("ZH-cn"), Some(Lang::ZhCN));
        assert_eq!(Lang::from_str("en-US"), Some(Lang::EnUS));
        assert_eq!(Lang::from_str("EN-us"), Some(Lang::EnUS));
        assert_eq!(Lang::from_str("zh"), None);
        assert_eq!(Lang::from_str("fr"), None);
        assert_eq!(Lang::from_str(""), None);
    }

    #[test]
    fn lang_as_str_round_trips() {
        for lang in [Lang::ZhCN, Lang::EnUS] {
            assert_eq!(Lang::from_str(lang.as_str()), Some(lang));
        }
    }

    // ---- 字典加载与查询 ----

    #[test]
    fn dictionaries_parse_successfully() {
        // OnceLock 第一次调用即触发解析；如果 JSON 损坏会 panic。
        let zh = dict_for(Lang::ZhCN);
        let en = dict_for(Lang::EnUS);
        assert!(!zh.is_empty(), "zh-CN 字典不应为空");
        assert!(!en.is_empty(), "en-US 字典不应为空");
    }

    #[test]
    fn t_returns_translation_for_known_keys() {
        // app.title 同时存在于两份字典，且翻译彼此不同。
        let zh = t(Lang::ZhCN, "app.title").expect("zh-CN 字典应包含 app.title");
        let en = t(Lang::EnUS, "app.title").expect("en-US 字典应包含 app.title");
        assert!(zh.contains("PhoneMic"));
        assert!(en.contains("PhoneMic"));

        // 错误码文案抽样验证。
        assert!(t(Lang::ZhCN, "error.LAN_LOST").is_some());
        assert!(t(Lang::EnUS, "error.PAIR_INVALID").is_some());
    }

    #[test]
    fn t_returns_none_for_missing_key() {
        assert_eq!(t(Lang::ZhCN, "totally.unknown.key"), None);
        assert_eq!(t(Lang::EnUS, "totally.unknown.key"), None);
    }

    /// Sanity check：两份字典 key 集合相等。
    /// 任务 3.24 会落地完整的字典完整性单元测试（含空字符串检查），此处只做基本核对，
    /// 避免开发期跑测试时遗漏新增 key。
    #[test]
    fn zh_cn_and_en_us_share_same_keys() {
        let zh = dict_for(Lang::ZhCN);
        let en = dict_for(Lang::EnUS);
        let zh_keys: std::collections::BTreeSet<&str> = zh.keys().copied().collect();
        let en_keys: std::collections::BTreeSet<&str> = en.keys().copied().collect();
        assert_eq!(
            zh_keys, en_keys,
            "zh-CN 与 en-US 字典 key 集合应一致；缺失：{:?}；多余：{:?}",
            en_keys.difference(&zh_keys).collect::<Vec<_>>(),
            zh_keys.difference(&en_keys).collect::<Vec<_>>(),
        );
    }

    // ---- 任务 3.24：桌面端 i18n 字典完整性单元测试 ----
    //
    // 任务来源：tasks.md 3.24
    // 关联需求：R8.1（zh-CN / en-US 两份字典必须同时齐全且无空翻译）
    // 设计来源：design.md §9.3
    //
    // 不变量：
    //   1. 两份字典的 key 集合完全一致；
    //   2. 两份字典中不存在空字符串（含仅含空白字符）的 value。
    //
    // 该测试与上方 `zh_cn_and_en_us_share_same_keys` 相比覆盖更严格：
    // 即便未来某个 key 被错误地填成 ""（例如复制粘贴漏译），也会立即失败。

    /// 任务 3.24：zh-CN 与 en-US 字典 key 集合相等且所有 value 非空。
    #[test]
    fn task_3_24_zh_cn_and_en_us_dicts_are_complete() {
        let zh = dict_for(Lang::ZhCN);
        let en = dict_for(Lang::EnUS);

        // 1. key 集合相等。
        let zh_keys: std::collections::BTreeSet<&str> = zh.keys().copied().collect();
        let en_keys: std::collections::BTreeSet<&str> = en.keys().copied().collect();
        let only_in_zh: Vec<&&str> = zh_keys.difference(&en_keys).collect();
        let only_in_en: Vec<&&str> = en_keys.difference(&zh_keys).collect();
        assert!(
            only_in_zh.is_empty() && only_in_en.is_empty(),
            "zh-CN 与 en-US 字典 key 集合不一致；只在 zh-CN 中：{only_in_zh:?}；只在 en-US 中：{only_in_en:?}",
        );

        // 2. 双方都至少有一项（防止空文件被误认为相等）。
        assert!(!zh.is_empty(), "zh-CN 字典不应为空");
        assert!(!en.is_empty(), "en-US 字典不应为空");

        // 3. 任一 value 都不得为空字符串或仅含空白字符。
        let mut empty_zh: Vec<&str> = zh
            .iter()
            .filter(|(_, v)| v.trim().is_empty())
            .map(|(k, _)| *k)
            .collect();
        empty_zh.sort_unstable();
        let mut empty_en: Vec<&str> = en
            .iter()
            .filter(|(_, v)| v.trim().is_empty())
            .map(|(k, _)| *k)
            .collect();
        empty_en.sort_unstable();
        assert!(
            empty_zh.is_empty() && empty_en.is_empty(),
            "i18n 字典存在空翻译；zh-CN 空 key：{empty_zh:?}；en-US 空 key：{empty_en:?}",
        );
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 24: locale → lang 决策
//
// 任务 3.23：对任意 locale 字符串：
//   - 若主子标签（首段，分隔符为 `-` 或 `_`）忽略大小写等于 `"zh"` → ZhCN；
//   - 否则一律 EnUS。
//   - decide_lang 是确定性的（同入参恒同结果）。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// 通用 locale 字符串生成器：随机 ASCII，加各种分隔符与空白。
    fn locale_str() -> impl Strategy<Value = String> {
        prop_oneof![
            // 完全任意 ASCII（含空白与分隔符）。
            prop::string::string_regex("[\\x20-\\x7e]{0,32}").unwrap(),
            // 形如 zh-XX / ZH_xx / fr-FR / en-us 的 BCP-47-like。
            (
                prop::string::string_regex("[A-Za-z]{1,3}").unwrap(),
                prop::string::string_regex("[-_]?[A-Za-z0-9]{0,8}").unwrap()
            )
                .prop_map(|(a, b)| format!("{a}{b}")),
        ]
    }

    /// 参考实现：与 `decide_lang` 等价的纯字符串解析（独立写一份对照）。
    fn reference(locale: &str) -> Lang {
        let trimmed = locale.trim();
        if trimmed.is_empty() { return Lang::EnUS; }
        let primary = trimmed
            .split(|c: char| c == '-' || c == '_')
            .next()
            .unwrap_or("");
        if primary.eq_ignore_ascii_case("zh") { Lang::ZhCN } else { Lang::EnUS }
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 24: locale → lang 决策
        #[test]
        fn property_24_decide_lang_matches_spec(s in locale_str()) {
            prop_assert_eq!(decide_lang(&s), reference(&s));
        }

        // Property 24 推论：决策必须是确定性的。
        #[test]
        fn property_24_decide_lang_is_deterministic(s in locale_str()) {
            let a = decide_lang(&s);
            let b = decide_lang(&s);
            prop_assert_eq!(a, b);
        }

        // Property 24 推论：在合法 BCP-47 格式下，主子标签为 zh 必映射到 ZhCN。
        #[test]
        fn property_24_zh_primary_maps_to_zh(
            sub in prop::string::string_regex("[A-Za-z0-9]{0,8}").unwrap(),
            sep in prop::sample::select(vec!["-", "_"]),
        ) {
            // 必须使用真正的分隔符（`-` 或 `_`）把 zh 与子标签区分开，
            // 否则 "zh" + sub 会被视为单一 primary subtag。
            let s = format!("zh{sep}{sub}");
            prop_assert_eq!(decide_lang(&s), Lang::ZhCN);
            let s2 = format!("ZH{sep}{sub}");
            prop_assert_eq!(decide_lang(&s2), Lang::ZhCN);
            // 主子标签独立 "zh"（无 region）也应被识别。
            prop_assert_eq!(decide_lang("zh"), Lang::ZhCN);
        }

        // 主子标签不为 zh 时（且非空）应回落 EnUS。
        #[test]
        fn property_24_non_zh_primary_falls_back(
            primary in prop::string::string_regex("[A-Za-df-zA-DF-Z]{1,4}").unwrap(),
        ) {
            prop_assume!(!primary.eq_ignore_ascii_case("zh"));
            // 仅当主子标签恰为 "zh" 时才映射；本生成器避开了 z+h 的精确组合即可。
            prop_assert_eq!(decide_lang(&primary), Lang::EnUS);
        }
    }
}
