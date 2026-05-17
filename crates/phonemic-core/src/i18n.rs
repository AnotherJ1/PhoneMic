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
}
