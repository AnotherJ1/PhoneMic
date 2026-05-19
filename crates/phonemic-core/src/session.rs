//! Session 注册表与 Session_Token 生命周期管理。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.15
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.3
//! - 需求来源：`.kiro/specs/phone-mic-voice-input/requirements.md` 7.3, 7.4, 7.6, 7.7
//!
//! 本模块提供：
//!
//! - [`SessionToken`]：256 位（32 字节）随机数 + Base64URL（无填充）编码，
//!   `Debug` 实现脱敏，避免误把 token 写入日志（requirement 9.7）；
//! - [`DeviceFingerprint`]：移动端浏览器 / 指纹字符串的 newtype；
//! - [`Session`]：一台已配对设备的运行时元数据，`device_id` 为
//!   `hex(SHA-256(fingerprint))[..16]`，便于 UI 稳定展示；
//! - [`SessionRegistry`]：以 token 为主键的内存注册表，提供
//!   `issue` / `validate` / `touch` / `revoke` / `revoke_device`
//!   / `list_sessions` 等操作。
//!
//! ## 关于恒定时间比较
//!
//! 注册表使用 [`HashMap`] 做 token 查找。token 拥有 256 位熵，攻击者
//! 无法通过时序差异有意义地枚举命中的 token，因此哈希表查找在威胁
//! 模型下是可接受的。Pairing_Code 的恒定时间比较由
//! [`crate::pairing_code::verify_pairing_code`]（任务 3.11）单独承担。

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::SystemTime;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// 256 位 / 32 字节随机数对应的 Base64URL（无填充）字符串长度。
const SESSION_TOKEN_STR_LEN: usize = 43;
/// `device_id` 取 SHA-256 摘要的前若干字节，再以 hex 编码。
/// 16 字节 → 32 hex 字符，足以避免 UI 列表碰撞同时保持紧凑。
const DEVICE_ID_DIGEST_BYTES: usize = 16;

/// 移动端设备指纹（design §4.3 / requirement 7.3）。
///
/// 采用 newtype 包装而非 `String` 别名，避免与"普通字符串"在 API
/// 边界处混用。`From<&str>` / `From<String>` 已实现以保留人体工学。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceFingerprint(pub String);

impl From<&str> for DeviceFingerprint {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for DeviceFingerprint {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Session_Token —— 256 位随机数的 Base64URL（无填充）编码。
///
/// 出于安全考虑，[`fmt::Debug`] 输出经过脱敏，仅显示
/// `SessionToken(<redacted>)`；如确需把字符串下发到客户端或写入响应体，
/// 请显式调用 [`SessionToken::as_str`]。这一模式与
/// [`crate::pairing_code::PairingCode`]（任务 3.9）保持一致，
/// 满足 requirement 9.7 关于"日志中不打印 Session_Token"的约束。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken(String);

impl SessionToken {
    /// 借用底层字符串切片。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 由模块内"已校验"路径直接构造 Session_Token。
    ///
    /// 仅暴露给同 crate 内的工厂函数 [`generate_token`]；保持私有
    /// 可见性可避免外部代码绕过 256 位熵 + Base64URL 的不变量。
    pub(crate) fn from_validated(s: String) -> Self {
        debug_assert_eq!(
            s.len(),
            SESSION_TOKEN_STR_LEN,
            "SessionToken must be 43 base64url chars (32 bytes)",
        );
        Self(s)
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 永不在 Debug 输出中暴露真实 token。
        f.write_str("SessionToken(<redacted>)")
    }
}

/// 一个已配对设备的运行时会话元数据（design §4.3 `Session`）。
///
/// 字段语义：
/// - [`Session::token`]：发放给该设备的 Session_Token；
/// - [`Session::device_id`]：`hex(SHA-256(fingerprint))[..16]` 的稳定字符串
///   摘要，长度 32 hex 字符。该字段仅用于 UI 展示与日志关联；鉴权仍以
///   [`Session::token`] 为唯一凭据；
/// - [`Session::device_label`]：用户可读的设备名（如 "iPhone 15 / Safari"）；
/// - [`Session::paired_at`]：配对成功时间戳（墙钟）；
/// - [`Session::last_seen`]：最近一次活动时间戳，由 [`SessionRegistry::touch`] 更新。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token: SessionToken,
    pub device_id: String,
    pub device_label: String,
    pub paired_at: SystemTime,
    pub last_seen: SystemTime,
}

/// 会话鉴权失败原因。
///
/// `NotFound` 与 `Revoked` 通过外部"已知历史 token"集合区分：
/// - 之前从未颁发过的 token → [`AuthError::NotFound`]；
/// - 曾经颁发但被显式 revoke 的 token → [`AuthError::Revoked`]。
///
/// 上层（任务 5.4 Auth 中间件）可据此返回更细化的错误码 / 文案，但
/// 客户端可见 HTTP 错误统一收敛为 `AUTH_REQUIRED`，避免给攻击者多余的
/// 探测面（design §3.6）。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// token 不在注册表中且从未被颁发过。
    #[error("session token not found")]
    NotFound,
    /// token 曾被颁发，但已通过 [`SessionRegistry::revoke`]
    /// 或 [`SessionRegistry::revoke_device`] 撤销。
    #[error("session token has been revoked")]
    Revoked,
}

/// Session_Token 注册表（design §4.3 `Pairing_Service::sessions` 字段的
/// 具体落地）。
///
/// ## 一致性保证
///
/// - `issue` 允许同一 [`DeviceFingerprint`] 拥有多个并存 token（例如手机
///   清掉浏览器存储后重新配对，或多个浏览器各自配对同一台设备）。这些
///   token 彼此独立，[`revoke`] 只撤销其中之一；[`revoke_device`] 则
///   一次性撤销该指纹下的所有 token。
/// - 撤销过的 token 字符串会保留在 `revoked` 集合中，从而让
///   [`validate`] 能区分 [`AuthError::NotFound`] / [`AuthError::Revoked`]。
/// - 注册表不涉及落盘 / 加密；持久化由更上层的 `Pairing_Service`
///   （任务 3.18）负责。
///
/// [`issue`]: SessionRegistry::issue
/// [`validate`]: SessionRegistry::validate
/// [`revoke`]: SessionRegistry::revoke
/// [`revoke_device`]: SessionRegistry::revoke_device
#[derive(Debug, Default)]
pub struct SessionRegistry {
    /// 当前有效 session：以 token 原始字符串为键。
    by_token: HashMap<String, Session>,
    /// fingerprint → 该指纹下所有有效 token 字符串的集合。
    by_fp: HashMap<DeviceFingerprint, HashSet<String>>,
    /// 已被显式吊销的 token 字符串集合，用于 [`validate`] 区分
    /// `NotFound` 与 `Revoked`。
    revoked: HashSet<String>,
}

impl SessionRegistry {
    /// 创建一个空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 为指定设备颁发一个新的 [`SessionToken`]。
    ///
    /// 内部使用 [`OsRng`] 作为加密随机源；如需在测试中注入确定性 RNG，
    /// 请改用 [`SessionRegistry::issue_with_rng`]。
    ///
    /// 同一 [`DeviceFingerprint`] 多次调用 `issue` 不会自动覆盖之前的
    /// token —— 由 [`revoke_device`] 集中撤销。
    ///
    /// [`revoke_device`]: SessionRegistry::revoke_device
    pub fn issue(
        &mut self,
        fp: DeviceFingerprint,
        label: String,
        now: SystemTime,
    ) -> SessionToken {
        let mut rng = OsRng;
        self.issue_with_rng(fp, label, now, &mut rng)
    }

    /// 注入 RNG 的 [`issue`] 版本，便于属性测试。
    ///
    /// 为生产路径强烈建议使用 [`OsRng`]；若调用方传入低熵 RNG 触发
    /// 同 token 碰撞，本方法会以"覆盖旧 session"的方式继续（因为
    /// [`HashMap`] 主键就是 token 字符串）。
    ///
    /// [`issue`]: SessionRegistry::issue
    pub fn issue_with_rng(
        &mut self,
        fp: DeviceFingerprint,
        label: String,
        now: SystemTime,
        rng: &mut impl RngCore,
    ) -> SessionToken {
        let token = generate_token(rng);
        let device_id = derive_device_id(&fp);
        let session = Session {
            token: token.clone(),
            device_id,
            device_label: label,
            paired_at: now,
            last_seen: now,
        };
        self.by_token.insert(token.0.clone(), session);
        self.by_fp.entry(fp).or_default().insert(token.0.clone());
        // 颁发的 token 一定不再算作"被撤销"——理论上 OsRng 不会与历史
        // 撤销过的 token 字符串碰撞，但仍做防御性清理。
        self.revoked.remove(&token.0);
        token
    }

    /// 校验 token 并返回对应 session 的克隆（requirement 7.4）。
    ///
    /// - 命中有效 session → `Ok(session)`；
    /// - token 曾被撤销 → `Err(AuthError::Revoked)`；
    /// - token 从未颁发过 → `Err(AuthError::NotFound)`。
    pub fn validate(&self, token: &SessionToken) -> Result<Session, AuthError> {
        if let Some(session) = self.by_token.get(token.as_str()) {
            return Ok(session.clone());
        }
        if self.revoked.contains(token.as_str()) {
            Err(AuthError::Revoked)
        } else {
            Err(AuthError::NotFound)
        }
    }

    /// 更新 session 的 [`Session::last_seen`] 时间戳；token 不存在时为 no-op。
    ///
    /// 该方法供 Web Server 在收到合法 WS 帧 / HTTP 请求后调用，从而让
    /// "已配对设备列表"中的活动时间保持新鲜（design §4.1）。
    pub fn touch(&mut self, token: &SessionToken, now: SystemTime) {
        if let Some(session) = self.by_token.get_mut(token.as_str()) {
            session.last_seen = now;
        }
    }

    /// 显式吊销一个 token（requirement 7.7）。
    ///
    /// 吊销后再次 [`validate`] 必定返回 [`AuthError::Revoked`]。
    /// 对未颁发或已吊销的 token 调用本方法是幂等 no-op。
    ///
    /// 注意：`revoke` 只移除该 token；同一 [`DeviceFingerprint`] 下的
    /// 其他 token 不受影响。需要一次性撤销整台设备的全部 token，请使用
    /// [`SessionRegistry::revoke_device`]。
    ///
    /// [`validate`]: SessionRegistry::validate
    pub fn revoke(&mut self, token: &SessionToken) {
        if self.by_token.remove(token.as_str()).is_some() {
            // 同步从 by_fp 索引中移除该 token 的引用。
            // device_id 是摘要，无法反推回 fingerprint，因此遍历
            // by_fp 找到包含该 token 的 fingerprint 集合并清理；
            // by_fp 长度等于"已配对设备数"，规模远小于已颁发 token 数，
            // 这次线性扫描在用户场景下仍然是 O(few)。
            let fp = self
                .by_fp
                .iter()
                .find(|(_, set)| set.contains(token.as_str()))
                .map(|(k, _)| k.clone());
            if let Some(fp) = fp {
                if let Some(set) = self.by_fp.get_mut(&fp) {
                    set.remove(token.as_str());
                    if set.is_empty() {
                        self.by_fp.remove(&fp);
                    }
                }
            }
        }
        self.revoked.insert(token.as_str().to_owned());
    }

    /// 吊销某设备指纹下的所有 session（requirement 7.6 / 7.7）。
    ///
    /// 用于"撤销授权"按钮：同一台手机可能因为重连或重置而拥有多个
    /// token，本方法保证它们一并失效，且后续对这些 token 的 [`validate`]
    /// 都会返回 [`AuthError::Revoked`]。其它 fingerprint 的 session
    /// 不受影响。
    ///
    /// [`validate`]: SessionRegistry::validate
    pub fn revoke_device(&mut self, fp: &DeviceFingerprint) {
        if let Some(tokens) = self.by_fp.remove(fp) {
            for token in tokens {
                self.by_token.remove(&token);
                self.revoked.insert(token);
            }
        }
    }

    /// 列出当前所有有效 session（requirement 7.6）。
    ///
    /// 返回值按 [`Session::paired_at`] 升序、`device_id` 字典序作为
    /// 次级键排序，保证 UI 渲染与测试断言的稳定性。
    #[must_use]
    pub fn list_sessions(&self) -> Vec<Session> {
        let mut out: Vec<Session> = self.by_token.values().cloned().collect();
        out.sort_by(|a, b| {
            a.paired_at
                .cmp(&b.paired_at)
                .then_with(|| a.device_id.cmp(&b.device_id))
        });
        out
    }

    /// 当前有效 session 数量，主要供测试与诊断使用。
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_token.len()
    }

    /// 注册表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_token.is_empty()
    }
}

/// 生成一个 256 位随机 + Base64URL（无填充）编码的 [`SessionToken`]。
fn generate_token(rng: &mut impl RngCore) -> SessionToken {
    let mut buf = [0u8; 32];
    rng.fill_bytes(&mut buf);
    SessionToken::from_validated(URL_SAFE_NO_PAD.encode(buf))
}

/// `device_id` 推导：取 SHA-256 摘要前 [`DEVICE_ID_DIGEST_BYTES`] 字节，
/// 再 hex 编码。摘要不可逆，故无法从 `device_id` 反推 fingerprint。
fn derive_device_id(fp: &DeviceFingerprint) -> String {
    let digest = Sha256::digest(fp.0.as_bytes());
    hex::encode(&digest[..DEVICE_ID_DIGEST_BYTES])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ts(secs_after_epoch: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs_after_epoch)
    }

    /// requirement 7.3 / design §4.3：颁发 + 校验回路必须打通；
    /// `Session` 字段与传入参数一致，时间戳被忠实保留。
    #[test]
    fn issue_then_validate_roundtrip() {
        let mut reg = SessionRegistry::new();
        let fp = DeviceFingerprint::from("device-fp-001");
        let label = "iPhone 15".to_owned();
        let now = ts(1_000);

        let token = reg.issue(fp.clone(), label.clone(), now);
        let session = reg.validate(&token).expect("token 应当合法");

        assert_eq!(session.token.as_str(), token.as_str());
        assert_eq!(session.device_label, label);
        assert_eq!(session.paired_at, now);
        assert_eq!(session.last_seen, now);
        // device_id 是 SHA-256(fingerprint) 的前 16 字节 hex —— 32 字符。
        assert_eq!(session.device_id.len(), DEVICE_ID_DIGEST_BYTES * 2);
        assert_eq!(session.device_id, derive_device_id(&fp));
        assert!(
            !session.device_id.contains(&fp.0),
            "device_id 不应包含原始 fingerprint",
        );
    }

    /// design §4.3：token 必须为 256 位随机 + Base64URL（无填充）。
    /// 32 字节经 Base64URL 编码后长度为 43；字符集仅含 `[A-Z a-z 0-9 _ -]`。
    #[test]
    fn token_is_256bit_base64url() {
        let mut reg = SessionRegistry::new();
        let token = reg.issue("fp".into(), "label".into(), ts(0));
        let s = token.as_str();

        assert_eq!(
            s.len(),
            SESSION_TOKEN_STR_LEN,
            "32 字节 base64url 无填充应为 43 字符",
        );
        assert!(
            s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "token 必须是 base64url 字符集：{s}",
        );
    }

    /// requirement 7.3：连续颁发的 token 互不相同（OsRng 概率上保证）。
    #[test]
    fn issued_tokens_are_unique() {
        let mut reg = SessionRegistry::new();
        let t1 = reg.issue("fp1".into(), "A".into(), ts(0));
        let t2 = reg.issue("fp2".into(), "B".into(), ts(0));
        assert_ne!(t1.as_str(), t2.as_str());
        assert_eq!(reg.len(), 2);
    }

    /// requirement 7.7：revoke 后 validate 必须返回 `AuthError::Revoked`，
    /// 且 token 已不再出现在有效集合中。
    #[test]
    fn revoke_makes_validate_return_revoked() {
        let mut reg = SessionRegistry::new();
        let token = reg.issue("fp".into(), "label".into(), ts(0));

        assert!(reg.validate(&token).is_ok());
        reg.revoke(&token);
        assert_eq!(reg.validate(&token), Err(AuthError::Revoked));
        assert_eq!(reg.len(), 0);
    }

    /// 从未颁发过的 token 校验时返回 `NotFound`，与 `Revoked` 区分。
    #[test]
    fn unknown_token_yields_not_found() {
        let reg = SessionRegistry::new();
        // 构造一个长度 / 字符集合法但从未颁发的 token。
        let fake = SessionToken::from_validated("A".repeat(SESSION_TOKEN_STR_LEN));
        assert_eq!(reg.validate(&fake), Err(AuthError::NotFound));
    }

    /// revoke 必须是幂等的：对未颁发的 token 调用不会 panic，且
    /// 之后对同一 token 调用 validate 仍返回 `Revoked`（已记入 `revoked`）。
    #[test]
    fn revoke_unknown_token_is_noop_and_marks_revoked() {
        let mut reg = SessionRegistry::new();
        let fake = SessionToken::from_validated("B".repeat(SESSION_TOKEN_STR_LEN));
        reg.revoke(&fake);
        assert!(reg.is_empty());
        assert_eq!(reg.validate(&fake), Err(AuthError::Revoked));
    }

    /// requirement 7.6 / 7.7：撤销某设备授权时，该设备的所有 session
    /// 全部失效，其它设备的 session 保持有效。
    #[test]
    fn revoke_device_drops_only_matching_sessions() {
        let mut reg = SessionRegistry::new();
        let fp_a = DeviceFingerprint::from("fp-A");
        let fp_b = DeviceFingerprint::from("fp-B");

        let token_a1 = reg.issue(fp_a.clone(), "A1".into(), ts(0));
        let token_a2 = reg.issue(fp_a.clone(), "A2".into(), ts(1));
        let token_b = reg.issue(fp_b.clone(), "B".into(), ts(2));

        reg.revoke_device(&fp_a);

        assert_eq!(reg.validate(&token_a1), Err(AuthError::Revoked));
        assert_eq!(reg.validate(&token_a2), Err(AuthError::Revoked));
        assert!(reg.validate(&token_b).is_ok());
        assert_eq!(reg.len(), 1);

        // 再次 revoke_device 同一 fingerprint 必须为幂等 no-op。
        reg.revoke_device(&fp_a);
        assert_eq!(reg.len(), 1);
    }

    /// 同一设备指纹的多个 token 在没有触发 `revoke_device` 前彼此独立：
    /// `revoke(t1)` 不应影响 `t2`。
    #[test]
    fn revoke_single_token_does_not_affect_siblings() {
        let mut reg = SessionRegistry::new();
        let fp = DeviceFingerprint::from("fp-shared");

        let t1 = reg.issue(fp.clone(), "browser-A".into(), ts(0));
        let t2 = reg.issue(fp.clone(), "browser-B".into(), ts(1));

        reg.revoke(&t1);

        assert_eq!(reg.validate(&t1), Err(AuthError::Revoked));
        assert!(reg.validate(&t2).is_ok());

        // revoke_device 之后剩余的 t2 也应被撤销。
        reg.revoke_device(&fp);
        assert_eq!(reg.validate(&t2), Err(AuthError::Revoked));
        assert!(reg.is_empty());
    }

    /// `touch` 必须能更新 `last_seen` 至传入时间，且对未知 token 为 no-op。
    #[test]
    fn touch_updates_last_seen_to_provided_time() {
        let mut reg = SessionRegistry::new();
        let token = reg.issue("fp".into(), "label".into(), ts(100));

        let before = reg.validate(&token).unwrap().last_seen;
        assert_eq!(before, ts(100));

        let later = ts(200);
        reg.touch(&token, later);
        let after = reg.validate(&token).unwrap().last_seen;
        assert_eq!(after, later);
        assert!(after > before);

        // 未知 token 不应 panic 也不应改变注册表大小。
        let len_before = reg.len();
        let unknown = SessionToken::from_validated("C".repeat(SESSION_TOKEN_STR_LEN));
        reg.touch(&unknown, ts(300));
        assert_eq!(reg.len(), len_before);

        // 已撤销 token 的 touch 同样为 no-op。
        reg.revoke(&token);
        reg.touch(&token, ts(400));
        assert_eq!(reg.validate(&token), Err(AuthError::Revoked));
    }

    /// requirement 7.6：`list_sessions` 返回所有有效 session，
    /// 且按 `paired_at` 升序排序，便于 UI 稳定渲染。
    #[test]
    fn list_sessions_returns_paired_at_sorted() {
        let mut reg = SessionRegistry::new();
        let t1 = reg.issue("fp-A".into(), "A".into(), ts(10));
        let t2 = reg.issue("fp-B".into(), "B".into(), ts(20));
        let t3 = reg.issue("fp-C".into(), "C".into(), ts(30));

        let sessions = reg.list_sessions();
        assert_eq!(sessions.len(), 3);

        let tokens: Vec<&str> = sessions.iter().map(|s| s.token.as_str()).collect();
        assert_eq!(tokens, vec![t1.as_str(), t2.as_str(), t3.as_str()]);

        for pair in sessions.windows(2) {
            assert!(pair[0].paired_at <= pair[1].paired_at);
        }

        // 撤销其一后列表中不再包含它。
        reg.revoke(&t2);
        let after = reg.list_sessions();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|s| s.token.as_str() != t2.as_str()));
    }

    /// requirement 9.7 / design §4.3：`SessionToken` 的 Debug 输出必须脱敏。
    #[test]
    fn session_token_debug_is_redacted() {
        let mut reg = SessionRegistry::new();
        let token = reg.issue("fp".into(), "label".into(), ts(0));

        let debug = format!("{token:?}");
        assert_eq!(debug, "SessionToken(<redacted>)");
        assert!(
            !debug.contains(token.as_str()),
            "Debug 输出不能泄漏真实 token",
        );
    }

    /// `Session` 的 Debug 输出会包含 `token: SessionToken(<redacted>)`，
    /// 因此即使整行被记入日志，原始 token 也不会出现在文本中。
    #[test]
    fn session_debug_does_not_leak_token() {
        let mut reg = SessionRegistry::new();
        let token = reg.issue("fp".into(), "label".into(), ts(0));
        let session = reg.validate(&token).unwrap();

        let debug = format!("{session:?}");
        assert!(debug.contains("<redacted>"));
        assert!(
            !debug.contains(token.as_str()),
            "Session Debug 输出不能泄漏真实 token",
        );
    }

    /// `device_id` 推导是确定的纯函数：相同 fingerprint → 相同 device_id；
    /// 不同 fingerprint → 不同 device_id（碰撞概率约 2^-128）。
    #[test]
    fn device_id_is_deterministic_and_distinct() {
        let mut reg = SessionRegistry::new();
        let t_a1 = reg.issue("fp-A".into(), "L1".into(), ts(0));
        let t_a2 = reg.issue("fp-A".into(), "L2".into(), ts(1));
        let t_b = reg.issue("fp-B".into(), "L3".into(), ts(2));

        let s_a1 = reg.validate(&t_a1).unwrap();
        let s_a2 = reg.validate(&t_a2).unwrap();
        let s_b = reg.validate(&t_b).unwrap();

        assert_eq!(
            s_a1.device_id, s_a2.device_id,
            "同一 fingerprint 派生的 device_id 必须一致",
        );
        assert_ne!(
            s_a1.device_id, s_b.device_id,
            "不同 fingerprint 应当映射到不同 device_id",
        );
    }

    /// 注入式 RNG：`issue_with_rng` 接受外部 `RngCore`，便于可重复测试。
    #[test]
    fn issue_with_rng_uses_provided_rng() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut reg = SessionRegistry::new();
        let mut rng_a = StdRng::seed_from_u64(42);
        let mut rng_b = StdRng::seed_from_u64(42);

        let token_a = reg.issue_with_rng("fp-1".into(), "A".into(), ts(0), &mut rng_a);
        // 用同一注册表会保留前一个 token；为避免主键冲突干扰断言，
        // 这里用第二个新注册表确认相同种子产出相同 token。
        let mut reg2 = SessionRegistry::new();
        let token_b = reg2.issue_with_rng("fp-2".into(), "B".into(), ts(0), &mut rng_b);

        assert_eq!(token_a.as_str(), token_b.as_str());
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// 任务 3.16 / 3.17：Session 生命周期 + 已配对设备 CRUD 属性测试。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::{BTreeSet, HashMap};
    use std::time::Duration;

    fn ts(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    /// 高层操作脚本：用于驱动 `SessionRegistry` 的状态机式属性测试。
    #[derive(Debug, Clone)]
    enum Op {
        /// 颁发一个新 token；fingerprint 由 `fp_idx` 选定（小整数池）。
        Issue { fp_idx: u8, label: u8 },
        /// 撤销之前已颁发的第 `pos` 个 token（按 issue 顺序，对总量取模）。
        RevokeIssued { pos: u32 },
        /// 撤销某 fingerprint 下的所有 token。
        RevokeDevice { fp_idx: u8 },
        /// 校验当前所有曾颁发过的 token，断言与历史模型保持一致。
        ValidateAll,
    }

    fn op_seq() -> impl Strategy<Value = Vec<Op>> {
        prop::collection::vec(
            prop_oneof![
                (0u8..4, 0u8..16).prop_map(|(fp_idx, label)| Op::Issue { fp_idx, label }),
                (0u32..32).prop_map(|pos| Op::RevokeIssued { pos }),
                (0u8..4).prop_map(|fp_idx| Op::RevokeDevice { fp_idx }),
                Just(Op::ValidateAll),
            ],
            0..40,
        )
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 18: Session_Token 生命周期
        // Feature: phone-mic-voice-input, Property 20: 已配对设备 CRUD
        //
        // 在任意 issue / revoke / revoke_device 操作序列下断言：
        //   - validate(token) 在每一步与"历史模型"一致：
        //       * 已 issue 但从未被 revoke* → Ok(session)
        //       * issue 后被任一种 revoke 触及 → Err(Revoked)
        //       * 从未 issue 过 → Err(NotFound)
        //   - list_sessions() 等于"按时间应用后"未被吊销的 token 集合（按 token 字符串）。
        #[test]
        fn property_18_and_20_session_lifecycle_and_crud(ops in op_seq()) {
            let mut reg = SessionRegistry::new();

            // 按 issue 顺序记录所有曾颁发过的 token；元素一旦写入不再删除。
            let mut history: Vec<(SessionToken, DeviceFingerprint)> = Vec::new();
            // token 字符串 → 是否被吊销（ground-truth 模型）。
            let mut revoked_model: HashMap<String, bool> = HashMap::new();

            let mut now_secs: u64 = 1_000;

            for op in ops {
                match op {
                    Op::Issue { fp_idx, label } => {
                        let fp = DeviceFingerprint::from(format!("fp-{fp_idx}"));
                        let token = reg.issue(fp.clone(), format!("L-{label}"), ts(now_secs));
                        revoked_model.insert(token.as_str().to_owned(), false);
                        history.push((token, fp));
                        now_secs += 1;
                    }
                    Op::RevokeIssued { pos } => {
                        if history.is_empty() { continue; }
                        let idx = (pos as usize) % history.len();
                        let token = history[idx].0.clone();
                        reg.revoke(&token);
                        revoked_model.insert(token.as_str().to_owned(), true);
                    }
                    Op::RevokeDevice { fp_idx } => {
                        let fp = DeviceFingerprint::from(format!("fp-{fp_idx}"));
                        reg.revoke_device(&fp);
                        for (tok, this_fp) in &history {
                            if this_fp == &fp {
                                revoked_model.insert(tok.as_str().to_owned(), true);
                            }
                        }
                    }
                    Op::ValidateAll => {
                        // 已颁发 token 的 validate 必须与模型一致。
                        for (tok, _fp) in &history {
                            let revoked = *revoked_model.get(tok.as_str()).unwrap_or(&false);
                            match reg.validate(tok) {
                                Ok(s) => {
                                    prop_assert!(!revoked, "未被吊销但模型说应有效");
                                    prop_assert_eq!(s.token.as_str(), tok.as_str());
                                }
                                Err(AuthError::Revoked) => {
                                    prop_assert!(revoked, "模型未标记吊销但 registry 返回 Revoked");
                                }
                                Err(AuthError::NotFound) => {
                                    prop_assert!(false, "已 issue 的 token 不应返回 NotFound");
                                }
                            }
                        }
                        // 从未颁发过的 token 必为 NotFound。
                        let unknown = SessionToken::from_validated("Z".repeat(43));
                        if !revoked_model.contains_key(unknown.as_str()) {
                            prop_assert_eq!(reg.validate(&unknown), Err(AuthError::NotFound));
                        }
                    }
                }
            }

            // Property 20：list_sessions 返回的 token 集合 == 模型中未吊销集合。
            let live_model: BTreeSet<String> = revoked_model
                .iter()
                .filter_map(|(k, &rev)| if !rev { Some(k.clone()) } else { None })
                .collect();
            let live_actual: BTreeSet<String> = reg
                .list_sessions()
                .iter()
                .map(|s| s.token.as_str().to_owned())
                .collect();
            prop_assert_eq!(live_actual, live_model);
        }
    }
}
