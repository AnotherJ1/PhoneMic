//! `config` —— 用户配置文件（`config.toml`）schema 与读写工具。
//!
//! 任务来源：tasks.md 2.4
//! 设计来源：design.md §5.3
//! 关联需求：2.2（首选端口）、2.3（HTTP/HTTPS 切换）、2.4（HTTPS 自签证书持久化）、
//! 6.5（注入延迟 0–500 ms）、6.7（暂停注入开关）、8.1（i18n 语言）。
//!
//! ## 设计要点
//!
//! - 所有结构都派生 `Serialize` / `Deserialize` / `Debug` / `Clone` /
//!   `PartialEq` / `Eq`，便于 toml 序列化与单元测试相等比较。
//! - 字段全部带 `#[serde(default = …)]`，`AppConfig::default()` 与
//!   "完全空 TOML" 的反序列化结果应一致；使旧版本配置文件在升级后
//!   仍可被向前兼容地读取。
//! - 校验逻辑收敛在 `*::validate` 方法里，并由 `load_from_path` 在
//!   解析后自动调用，杜绝"成功加载却携带非法值"的不变量违例。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 默认首选端口（Requirement 2.2 默认值，详见 design.md §5.3）。
const fn default_preferred_port() -> u16 {
    18080
}

/// 默认仅监听 LAN 接口（Requirement 7.8 子网约束的配置侧默认）。
const fn default_bind_lan_only() -> bool {
    true
}

/// `[asr] default_lang` 默认值（design.md §5.3）。
const fn default_asr_lang() -> AsrLang {
    AsrLang::ZhCN
}

/// `[security] auto_revoke_idle_days` 默认值（design.md §5.3）。
const fn default_auto_revoke() -> u32 {
    30
}

/// `[input] inject_delay_ms` 上限（Requirement 6.5：0–500 ms）。
pub const MAX_INJECT_DELAY_MS: u16 = 500;

/// 配置加载 / 保存 / 校验过程中可能出现的错误。
///
/// `Io` / `ParseToml` / `SerializeToml` 自动从底层错误转换；`Validation`
/// 用于承载业务级约束（如 `inject_delay_ms` 越界）的人可读说明。
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    /// 读写配置文件时的 I/O 错误。
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// TOML 反序列化失败（语法或类型不匹配）。
    #[error("config parse error: {0}")]
    ParseToml(#[from] toml::de::Error),
    /// 将 `AppConfig` 序列化为 TOML 失败（理论上仅在内部 bug 时触发）。
    #[error("config serialize error: {0}")]
    SerializeToml(#[from] toml::ser::Error),
    /// 字段值通过类型校验，但违反业务约束（如 `inject_delay_ms` > 500）。
    #[error("config validation failed: {0}")]
    Validation(String),
}

/// `[server]` —— Web_Server 监听相关配置（design.md §5.3）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ServerCfg {
    /// 首选 TCP 端口；占用时由端口选择器在 1024–65535 内回退（Req 2.2）。
    #[serde(default = "default_preferred_port")]
    pub preferred_port: u16,
    /// 是否启用 HTTPS（自签证书）；默认关闭（Req 2.3 / 2.4）。
    #[serde(default)]
    pub enable_https: bool,
    /// 是否仅监听 LAN 接口；默认 `true`，避免误绑定公网地址。
    #[serde(default = "default_bind_lan_only")]
    pub bind_lan_only: bool,
}

impl Default for ServerCfg {
    fn default() -> Self {
        Self {
            preferred_port: default_preferred_port(),
            enable_https: false,
            bind_lan_only: default_bind_lan_only(),
        }
    }
}

/// 桌面端 UI 显示语言（关联 Req 8.1）。
///
/// 序列化字面量与 design.md §5.3 中 `auto | zh-CN | en-US` 完全一致。
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiLanguage {
    /// 跟随系统语言自动选择。
    #[serde(rename = "auto")]
    Auto,
    /// 简体中文。
    #[serde(rename = "zh-CN")]
    ZhCN,
    /// 美式英语。
    #[serde(rename = "en-US")]
    EnUS,
}

impl Default for UiLanguage {
    fn default() -> Self {
        Self::Auto
    }
}

/// `[ui]` —— 桌面端 UI 配置（Req 8.1）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UiCfg {
    /// 桌面端 UI 语言；`auto` 表示跟随系统语言。
    #[serde(default)]
    pub language: UiLanguage,
}

impl Default for UiCfg {
    fn default() -> Self {
        Self {
            language: UiLanguage::default(),
        }
    }
}

/// ASR 默认语种（关联 Req 5.3）。
///
/// 当前仅支持 `zh-CN` / `en-US` 两种，未来如需扩展将由设计文档先行声明。
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsrLang {
    /// 简体中文（默认）。
    #[serde(rename = "zh-CN")]
    ZhCN,
    /// 美式英语。
    #[serde(rename = "en-US")]
    EnUS,
}

impl Default for AsrLang {
    fn default() -> Self {
        default_asr_lang()
    }
}

/// `[asr]` —— ASR 偏好配置（Req 5.3 / 5.4）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AsrCfg {
    /// 默认识别语种。
    #[serde(default = "default_asr_lang")]
    pub default_lang: AsrLang,
    /// 是否优先使用 Server_ASR；默认 `false`（优先 Browser_ASR）。
    #[serde(default)]
    pub prefer_server_asr: bool,
}

impl Default for AsrCfg {
    fn default() -> Self {
        Self {
            default_lang: default_asr_lang(),
            prefer_server_asr: false,
        }
    }
}

/// `[input]` —— 键盘注入相关配置。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InputCfg {
    /// 字符间注入延迟，单位毫秒；范围 0–500（Req 6.5）。
    #[serde(default)]
    pub inject_delay_ms: u16,
    /// 是否暂停注入（Req 6.7）。
    #[serde(default)]
    pub paused: bool,
}

impl Default for InputCfg {
    fn default() -> Self {
        Self {
            inject_delay_ms: 0,
            paused: false,
        }
    }
}

impl InputCfg {
    /// 校验 `inject_delay_ms` 不超过 500 ms（Req 6.5）。
    ///
    /// # Errors
    ///
    /// 当 `inject_delay_ms` 大于 [`MAX_INJECT_DELAY_MS`] 时返回
    /// [`ConfigError::Validation`]。
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.inject_delay_ms > MAX_INJECT_DELAY_MS {
            return Err(ConfigError::Validation(format!(
                "input.inject_delay_ms = {} 超过上限 {} ms",
                self.inject_delay_ms, MAX_INJECT_DELAY_MS,
            )));
        }
        Ok(())
    }
}

/// `[security]` —— 配对设备的安全策略。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SecurityCfg {
    /// 闲置自动撤销天数（design.md §5.3，默认 30）。
    #[serde(default = "default_auto_revoke")]
    pub auto_revoke_idle_days: u32,
}

impl Default for SecurityCfg {
    fn default() -> Self {
        Self {
            auto_revoke_idle_days: default_auto_revoke(),
        }
    }
}

/// 顶层应用配置：聚合 `[server] / [ui] / [asr] / [input] / [security]` 五个段。
///
/// 所有子结构都带 `#[serde(default)]`，因此读取空文件或缺字段的旧版
/// 配置仍可还原为合法默认值，再由 `validate` 把守业务约束。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct AppConfig {
    /// `[server]` 段。
    #[serde(default)]
    pub server: ServerCfg,
    /// `[ui]` 段。
    #[serde(default)]
    pub ui: UiCfg,
    /// `[asr]` 段。
    #[serde(default)]
    pub asr: AsrCfg,
    /// `[input]` 段。
    #[serde(default)]
    pub input: InputCfg,
    /// `[security]` 段。
    #[serde(default)]
    pub security: SecurityCfg,
}

impl AppConfig {
    /// 跨段总校验：当前仅委托给 [`InputCfg::validate`]，未来如新增约束可在此扩展。
    ///
    /// # Errors
    ///
    /// 任一子段校验失败时返回 [`ConfigError::Validation`]。
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.input.validate()?;
        Ok(())
    }

    /// 从指定路径读取 TOML 文件并解析为 `AppConfig`。
    ///
    /// 解析后会强制调用 [`Self::validate`]，确保返回值满足业务不变量。
    ///
    /// # Errors
    ///
    /// - [`ConfigError::Io`]：文件读取失败；
    /// - [`ConfigError::ParseToml`]：TOML 语法或类型错误；
    /// - [`ConfigError::Validation`]：字段值越界（如 `inject_delay_ms > 500`）。
    pub fn load_from_path<P: AsRef<Path>>(p: P) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(p.as_ref())?;
        let cfg: Self = toml::from_str(&text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// 将当前配置以 pretty TOML 形式写入指定路径。
    ///
    /// 写入前不会再次 `validate`，调用方若需保证落盘内容合法应自行先调用。
    ///
    /// # Errors
    ///
    /// - [`ConfigError::SerializeToml`]：序列化失败；
    /// - [`ConfigError::Io`]：文件写入失败。
    pub fn save_to_path<P: AsRef<Path>>(&self, p: P) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        fs::write(p.as_ref(), text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, AsrCfg, AsrLang, ConfigError, InputCfg, SecurityCfg, ServerCfg, UiCfg,
        UiLanguage, MAX_INJECT_DELAY_MS,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 进程内单调递增的测试用例计数器，配合 PID 与纳秒时间戳共同避免并发碰撞。
    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    /// 在系统临时目录中拼一个唯一文件路径。
    ///
    /// 测试结束（无论通过还是 panic）会通过 `TempPath` 的 `Drop` 自动清理。
    fn unique_temp_path(label: &str) -> TempPath {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let pid = std::process::id();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "phonemic-config-{label}-{pid}-{nanos}-{seq}.toml"
        ));
        TempPath { path }
    }

    /// 简单的 RAII 临时文件守卫；离开作用域时尽力删除底层文件。
    struct TempPath {
        path: PathBuf,
    }

    impl TempPath {
        fn as_path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn defaults_match_design_section_5_3() {
        let cfg = AppConfig::default();

        assert_eq!(cfg.server.preferred_port, 18080);
        assert!(!cfg.server.enable_https);
        assert!(cfg.server.bind_lan_only);

        assert_eq!(cfg.ui.language, UiLanguage::Auto);

        assert_eq!(cfg.asr.default_lang, AsrLang::ZhCN);
        assert!(!cfg.asr.prefer_server_asr);

        assert_eq!(cfg.input.inject_delay_ms, 0);
        assert!(!cfg.input.paused);

        assert_eq!(cfg.security.auto_revoke_idle_days, 30);
    }

    #[test]
    fn substruct_defaults_compose_into_app_config_default() {
        // AppConfig::default() 必须等于「逐段调用 *Cfg::default 拼装」的结果，
        // 防止子结构默认值与顶层派生 Default 之间偷偷发生分歧。
        let composed = AppConfig {
            server: ServerCfg::default(),
            ui: UiCfg::default(),
            asr: AsrCfg::default(),
            input: InputCfg::default(),
            security: SecurityCfg::default(),
        };
        assert_eq!(AppConfig::default(), composed);
    }

    #[test]
    fn default_round_trips_through_toml() {
        let cfg = AppConfig::default();
        let text = toml::to_string(&cfg).expect("serialize default config");
        let parsed: AppConfig = toml::from_str(&text).expect("parse default config");
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn empty_toml_yields_default() {
        // 空文件场景：所有 #[serde(default)] 应让结构回退到默认值。
        let parsed: AppConfig = toml::from_str("").expect("parse empty toml");
        assert_eq!(parsed, AppConfig::default());
    }

    #[test]
    fn input_validate_accepts_boundary_values() {
        let mut input = InputCfg::default();
        input.inject_delay_ms = 0;
        input.validate().expect("0 ms should be valid");

        input.inject_delay_ms = MAX_INJECT_DELAY_MS; // 500
        input.validate().expect("500 ms should be valid");
    }

    #[test]
    fn input_validate_rejects_above_500ms() {
        let input = InputCfg {
            inject_delay_ms: MAX_INJECT_DELAY_MS + 1,
            paused: false,
        };
        match input.validate() {
            Err(ConfigError::Validation(msg)) => {
                assert!(
                    msg.contains("inject_delay_ms"),
                    "validation message should mention field name, got: {msg}"
                );
            }
            other => panic!("expected ConfigError::Validation, got {other:?}"),
        }
    }

    #[test]
    fn load_from_path_rejects_inject_delay_above_500ms() {
        let temp = unique_temp_path("invalid-delay");
        std::fs::write(
            temp.as_path(),
            "[input]\ninject_delay_ms = 600\npaused = false\n",
        )
        .expect("write invalid config");

        match AppConfig::load_from_path(temp.as_path()) {
            Err(ConfigError::Validation(_)) => {}
            other => panic!("expected ConfigError::Validation, got {other:?}"),
        }
    }

    #[test]
    fn save_then_load_round_trips_non_default_config() {
        // 选择一组与默认值不同的字段，确保 save/load 不会丢字段或静默改值。
        let cfg = AppConfig {
            server: ServerCfg {
                preferred_port: 19000,
                enable_https: true,
                bind_lan_only: false,
            },
            ui: UiCfg {
                language: UiLanguage::EnUS,
            },
            asr: AsrCfg {
                default_lang: AsrLang::EnUS,
                prefer_server_asr: true,
            },
            input: InputCfg {
                inject_delay_ms: 250,
                paused: true,
            },
            security: SecurityCfg {
                auto_revoke_idle_days: 7,
            },
        };

        let temp = unique_temp_path("roundtrip");
        cfg.save_to_path(temp.as_path()).expect("save config");
        let loaded = AppConfig::load_from_path(temp.as_path()).expect("load config");
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn load_from_path_returns_default_for_empty_file() {
        let temp = unique_temp_path("empty");
        std::fs::write(temp.as_path(), "").expect("write empty file");
        let loaded = AppConfig::load_from_path(temp.as_path()).expect("load empty file");
        assert_eq!(loaded, AppConfig::default());
    }

    #[test]
    fn ui_language_serializes_to_design_literals() {
        // 序列化结果必须与 design.md §5.3 中的字面量完全一致。
        let cases = [
            (UiLanguage::Auto, "\"auto\""),
            (UiLanguage::ZhCN, "\"zh-CN\""),
            (UiLanguage::EnUS, "\"en-US\""),
        ];
        for (variant, expected) in cases {
            let json = serde_json::to_string(&variant).expect("serialize UiLanguage");
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn asr_lang_serializes_to_design_literals() {
        let cases = [(AsrLang::ZhCN, "\"zh-CN\""), (AsrLang::EnUS, "\"en-US\"")];
        for (variant, expected) in cases {
            let json = serde_json::to_string(&variant).expect("serialize AsrLang");
            assert_eq!(json, expected);
        }
    }
}
