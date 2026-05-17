//! Pairing_Service：Pairing_Code 重启失效语义与配对/会话编排门面。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.18
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.3
//! - 需求来源：`.kiro/specs/phone-mic-voice-input/requirements.md` Requirement 7.9
//!
//! ## 关键不变量（Req 7.9）
//!
//! [`PairingService::on_startup`] **必定**调用
//! [`crate::pairing_code::generate_pairing_code`] 生成全新 code，
//! 不会从磁盘 / 配置目录读取上一次会话遗留的 `current_code`。
//! 这条约束由该方法本身的实现保证：模块内不存在任何反序列化路径或
//! 文件 I/O，即便上层把 `PairingService` 序列化后写盘，下次启动只要
//! 经过 `on_startup` 都会被无条件覆盖。
//!
//! 反过来说，旧 code 在重启之后只可能存在于：
//! 1. 已下发给手机端的 [`SessionToken`]（与 Pairing_Code 解耦，Req 7.3 / 7.4）；
//! 2. 桌面 UI 的瞬时显示（被新 code 直接覆盖）。
//!
//! 因此重启后 `submit_pair(oldCode, …)` 一律返回 [`PairError::Invalid`]
//! ——这是 Property 22（design.md §7）要属性测试覆盖的语义，
//! 由任务 3.19 单独完成。
//!
//! ## 与同 crate 其他模块的协作
//!
//! - `current_code` 来自 [`crate::pairing_code`]（任务 3.9 / 3.11）；
//! - 限流由 [`crate::pair_rate_limit::PairRateLimiter`]（任务 3.13）承担；
//! - 会话登记由 [`crate::session::SessionRegistry`]（任务 3.15）承担。
//!
//! 本模块只负责把三者拼装成一个面向调用方（任务 5.6 `/api/pair`、
//! 任务 10.2 桌面 UI）的稳定门面，不引入新的状态与时钟。

use std::net::IpAddr;
use std::time::{Instant, SystemTime};

use crate::pair_rate_limit::PairRateLimiter;
use crate::pairing_code::{PairingCode, generate_pairing_code, verify_pairing_code};
use crate::session::{DeviceFingerprint, Session, SessionRegistry, SessionToken};

/// 配对失败 / 限流错误。
///
/// 与 design.md §3.6 中定义的 HTTP 错误码 `PAIR_INVALID` / `PAIR_RATELIMIT`
/// 一一对应；HTTP 适配层（任务 5.6）负责把本枚举映射成
/// 401 / 429 + `retryAfter` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    /// 同一来源 IP 在 5 分钟内累计失败次数已达阈值（Req 7.5）。
    /// 对应 HTTP 错误码 `PAIR_RATELIMIT`。
    #[error("rate limited")]
    RateLimited,
    /// 配对码不匹配（包含恒定时间比较失败，Req 7.2）。
    /// 对应 HTTP 错误码 `PAIR_INVALID`。
    #[error("invalid pairing code")]
    Invalid,
}

/// Desktop_App 范围内的配对/会话门面。
///
/// 见模块级文档关于 Req 7.9 的不变量。本结构体通过组合三个底层组件
/// 提供单进程内的可变状态，不持有 `Mutex` —— 由调用方（典型情况下
/// 是 axum 的 `State<Arc<Mutex<PairingService>>>`，任务 5.6）决定如何
/// 在并发请求间同步。
pub struct PairingService {
    /// 当前对外公开的 Pairing_Code，重启即失效（Req 7.9）。
    current_code: PairingCode,
    /// 按客户端 IP 维护的失败计数 / 限流窗口（Req 7.5）。
    rate_limiter: PairRateLimiter,
    /// 已颁发的 Session 注册表（Req 7.3 / 7.4 / 7.7）。
    sessions: SessionRegistry,
}

impl Default for PairingService {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingService {
    /// 构造一个全新的 [`PairingService`]，立即生成一份 Pairing_Code。
    ///
    /// 该方法等价于"启动一次 Desktop_App"——既会被 Tauri 的 `setup`
    /// 回调调用，也可在测试中直接使用。任何后续重启场景都应当通过
    /// [`Self::on_startup`] 显式表达"作废旧 code"语义，参见 Req 7.9。
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_code: generate_pairing_code(),
            rate_limiter: PairRateLimiter::new(),
            sessions: SessionRegistry::new(),
        }
    }

    /// 应用启动钩子：**无条件**重新生成 Pairing_Code，使旧 code 立即失效。
    ///
    /// 这是 Req 7.9 的具体落地：即便 `PairingService` 在某种持久化方案下
    /// 被复活，只要 Desktop_App 在 `setup` 阶段调用本方法，旧 code 就一定
    /// 会被覆盖。配对失败计数和已颁发 Session 是否清空属于不同议题：
    /// - 失败计数与已成功配对的 Session 在重启时一并丢失（结构体本身被
    ///   重新构造），因此实务上 Tauri 的 `setup` 回调倾向于
    ///   `let svc = PairingService::new();` 然后 `svc.on_startup();`
    ///   或直接使用 `PairingService::new()`；
    /// - 当 Desktop_App 选择持久化 Session（任务 7.7 之外的扩展）时，
    ///   `on_startup` 仍只动 `current_code`，避免误伤合法用户的 token。
    pub fn on_startup(&mut self) {
        self.current_code = generate_pairing_code();
    }

    /// 借用当前 Pairing_Code，供桌面 UI 渲染连接面板（任务 10.2）。
    #[must_use]
    pub fn current_pairing_code(&self) -> &PairingCode {
        &self.current_code
    }

    /// 用户在桌面 UI 点击"重新生成"时调用，立即作废旧 code 并替换为新 code。
    ///
    /// 与 [`Self::on_startup`] 共享同一份"生成-替换"路径，但语义上专门用于
    /// 用户主动触发的轮换；不动失败计数与已颁发 Session。
    pub fn rotate_code(&mut self) -> &PairingCode {
        self.current_code = generate_pairing_code();
        &self.current_code
    }

    /// 处理一次配对请求。语义见 design §4.3：
    ///
    /// 1. 若来源 IP 已被限流，直接返回 [`PairError::RateLimited`]，
    ///    **不再**核对 candidate（避免在被限流期内通过时序探测有效 code）。
    /// 2. 若 candidate 与当前 `current_code` 不一致，则记录失败并返回
    ///    [`PairError::Invalid`]。注意：`record_failure` 在窗口已过期时
    ///    会自动开新窗口，因此这里无需特殊处理"刚解除限流"的情况。
    /// 3. 否则颁发新的 [`SessionToken`] 并返回。成功配对会清空该 IP
    ///    的失败计数（[`PairRateLimiter::record_success`]），符合
    ///    Req 7.5 关于"连续失败"的约束。
    ///
    /// 参数 `now` 为成功路径的 `paired_at` / `last_seen` 时间戳；
    /// `mono` 是单调时钟实例，用于失败计数窗口判断。两者解耦使得：
    /// - 单调时钟（`Instant`）能正确反映限流窗口长度，不受系统时间
    ///   被向后调整影响；
    /// - 墙钟（`SystemTime`）适合写入 Session 元数据并在 UI 上展示。
    pub fn submit_pair(
        &mut self,
        candidate: &str,
        fp: DeviceFingerprint,
        label: String,
        peer: IpAddr,
        now: SystemTime,
        mono: Instant,
    ) -> Result<SessionToken, PairError> {
        if self.rate_limiter.is_rate_limited(peer, mono) {
            return Err(PairError::RateLimited);
        }

        if !verify_pairing_code(&self.current_code, candidate) {
            self.rate_limiter.record_failure(peer, mono);
            return Err(PairError::Invalid);
        }

        self.rate_limiter.record_success(peer);
        // `now` 即 Session 的 `paired_at` 时间戳（design §4.3）。
        // 与 `mono`（限流单调时钟）解耦，使属性测试可独立注入两条时间线。
        Ok(self.sessions.issue(fp, label, now))
    }

    /// 列出当前所有有效 Session（Req 7.6）。
    #[must_use]
    pub fn list_sessions(&self) -> Vec<Session> {
        self.sessions.list_sessions()
    }

    /// 显式吊销一个 Session_Token（Req 7.7）。
    pub fn revoke(&mut self, token: &SessionToken) {
        self.sessions.revoke(token);
    }

    /// 吊销某设备指纹下的所有 Session（Req 7.6 / 7.7）。
    pub fn revoke_device(&mut self, fp: &DeviceFingerprint) {
        self.sessions.revoke_device(fp);
    }

    /// 借用底层 Session 注册表，供需要 `validate` / `touch` 等方法的
    /// 上层（任务 5.4 Auth 中间件、任务 6.x WebSocket 路径）使用。
    #[must_use]
    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    /// 可变借用底层 Session 注册表，供调用方在持锁情况下完成
    /// `touch` 等需要 `&mut self` 的操作。
    #[must_use]
    pub fn sessions_mut(&mut self) -> &mut SessionRegistry {
        &mut self.sessions
    }
}

#[cfg(test)]
mod tests {
    //! `PairingService` 的单元测试。
    //!
    //! 这里覆盖 design.md §4.3 / Req 7.5 / 7.7 / 7.9 的核心语义，
    //! 但 Property 22（"重启后旧 code 永远 Invalid"）由任务 3.19 的属性测试
    //! 单独覆盖，避免在此重复样本式断言。

    use super::*;
    use crate::pair_rate_limit::FAILURE_THRESHOLD;
    use crate::pairing_code::{PAIRING_CODE_ALPHABET, PAIRING_CODE_LEN};
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    /// 取一个固定的合法 8 位字符串作为反例，所有字节都来自字母表，
    /// 因此能进入 `verify_pairing_code` 的"长度合法但内容不同"分支。
    const SAMPLE_WRONG_CODE: &str = "ABCDJKMN";

    fn loopback(d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, d))
    }

    /// 找到一个与给定 `current` 不同、且全部字节合法的候选，避免出现
    /// "随机生成的当前 code 恰好等于 SAMPLE_WRONG_CODE"造成的偶发干扰。
    fn pick_wrong_candidate(current: &PairingCode) -> String {
        if current.as_str() != SAMPLE_WRONG_CODE {
            return SAMPLE_WRONG_CODE.to_owned();
        }
        // 把第一个字节换成字母表中下一个字符。
        let bytes = current.as_str().as_bytes();
        let first = bytes[0];
        let next = PAIRING_CODE_ALPHABET
            .iter()
            .copied()
            .find(|b| *b != first)
            .expect("alphabet 至少有 2 个不同字符");
        let mut buf = bytes.to_vec();
        buf[0] = next;
        String::from_utf8(buf).expect("alphabet 子集必为合法 UTF-8")
    }

    /// Req 7.9：`on_startup` 必须把当前 code 替换为新随机值。
    /// 严格意义上的"概率断言"——但 31^8 ≈ 8.5e11，碰撞率可忽略。
    #[test]
    fn on_startup_replaces_current_code() {
        let mut svc = PairingService::new();
        let before = svc.current_pairing_code().as_str().to_owned();
        assert_eq!(before.len(), PAIRING_CODE_LEN);

        svc.on_startup();
        let after = svc.current_pairing_code().as_str().to_owned();

        assert_eq!(after.len(), PAIRING_CODE_LEN);
        assert_ne!(
            before, after,
            "on_startup 后 code 必须改变（碰撞概率约 1 / 31^8）",
        );
    }

    /// Req 7.9：旧 code 在 `on_startup` 之后立即失效。
    #[test]
    fn on_startup_invalidates_previous_code() {
        let mut svc = PairingService::new();
        let old = svc.current_pairing_code().as_str().to_owned();

        svc.on_startup();

        let result = svc.submit_pair(
            &old,
            "fp".into(),
            "label".into(),
            loopback(2),
            SystemTime::now(),
            Instant::now(),
        );
        assert_eq!(result.unwrap_err(), PairError::Invalid);
    }

    /// `rotate_code` 与 `on_startup` 共享生成逻辑，但语义上由用户主动触发。
    #[test]
    fn rotate_code_changes_current_code() {
        let mut svc = PairingService::new();
        let before = svc.current_pairing_code().as_str().to_owned();
        let after = svc.rotate_code().as_str().to_owned();
        assert_ne!(before, after);
    }

    /// 提交合法 code → 颁发 token，且该 token 能通过底层注册表校验。
    #[test]
    fn submit_pair_with_valid_code_issues_token() {
        let mut svc = PairingService::new();
        let candidate = svc.current_pairing_code().as_str().to_owned();

        let token = svc
            .submit_pair(
                &candidate,
                "fp-A".into(),
                "iPhone".into(),
                loopback(3),
                SystemTime::now(),
                Instant::now(),
            )
            .expect("合法 code 应当颁发 token");

        let session = svc
            .sessions()
            .validate(&token)
            .expect("注册表应认得刚颁发的 token");
        // `device_id` 是 fingerprint 的 SHA-256 前 16 字节 hex（design §4.3）；
        // 这里只断言长度与稳定性，避免在测试中硬编码摘要常量。
        assert_eq!(session.device_id.len(), 32);
        assert_eq!(session.device_label, "iPhone");
    }

    /// 提交错误 code → `Invalid`；连续 `FAILURE_THRESHOLD` 次失败之后
    /// 第 `FAILURE_THRESHOLD + 1` 次提交转为 `RateLimited`。
    #[test]
    fn rate_limit_kicks_in_after_threshold_failures() {
        let mut svc = PairingService::new();
        let peer = loopback(4);
        let mono = Instant::now();
        let now = SystemTime::now();
        let wrong = pick_wrong_candidate(svc.current_pairing_code());

        for i in 0..FAILURE_THRESHOLD {
            let err = svc
                .submit_pair(
                    &wrong,
                    "fp".into(),
                    "label".into(),
                    peer,
                    now,
                    mono,
                )
                .unwrap_err();
            assert_eq!(err, PairError::Invalid, "第 {i} 次失败应当返回 Invalid");
        }

        // 第 FAILURE_THRESHOLD + 1 次提交：限流必须先于 verify 命中。
        let err = svc
            .submit_pair(&wrong, "fp".into(), "label".into(), peer, now, mono)
            .unwrap_err();
        assert_eq!(err, PairError::RateLimited);
    }

    /// 限流在窗口期内必须先于 `verify_pairing_code` 命中：
    /// 即便此后提交的是合法 code，也不应当被颁发 token。
    /// 这是 design §4.3 的语义——限流期间不暴露任何 verify 结果差异。
    #[test]
    fn rate_limit_blocks_even_valid_code() {
        let mut svc = PairingService::new();
        let peer = loopback(5);
        let mono = Instant::now();
        let now = SystemTime::now();
        let wrong = pick_wrong_candidate(svc.current_pairing_code());
        let valid = svc.current_pairing_code().as_str().to_owned();

        for _ in 0..FAILURE_THRESHOLD {
            let _ = svc.submit_pair(&wrong, "fp".into(), "label".into(), peer, now, mono);
        }

        let err = svc
            .submit_pair(&valid, "fp".into(), "label".into(), peer, now, mono)
            .unwrap_err();
        assert_eq!(err, PairError::RateLimited);
    }

    /// 不同 IP 的失败窗口互相独立：A 被限流时 B 仍可正常配对。
    #[test]
    fn rate_limit_is_scoped_per_ip() {
        let mut svc = PairingService::new();
        let alice = loopback(10);
        let bob = loopback(11);
        let mono = Instant::now();
        let now = SystemTime::now();
        let wrong = pick_wrong_candidate(svc.current_pairing_code());

        for _ in 0..FAILURE_THRESHOLD {
            let _ = svc.submit_pair(&wrong, "fp-A".into(), "A".into(), alice, now, mono);
        }
        let valid = svc.current_pairing_code().as_str().to_owned();

        // bob 不受 alice 的失败计数影响。
        let token = svc
            .submit_pair(&valid, "fp-B".into(), "B".into(), bob, now, mono)
            .expect("bob 不应被限流");
        assert!(svc.sessions().validate(&token).is_ok());
    }

    /// 成功配对后失败计数应被清空（[`PairRateLimiter::record_success`]），
    /// 即便此前已有数次失败累积，紧接着的 5 次失败仍应触发限流。
    #[test]
    fn successful_pair_resets_failure_count() {
        let mut svc = PairingService::new();
        let peer = loopback(12);
        let mono = Instant::now();
        let now = SystemTime::now();
        let wrong = pick_wrong_candidate(svc.current_pairing_code());

        // 先做几次失败但未到阈值。
        for _ in 0..(FAILURE_THRESHOLD - 1) {
            let _ = svc.submit_pair(&wrong, "fp".into(), "label".into(), peer, now, mono);
        }

        // 一次成功。
        let valid = svc.current_pairing_code().as_str().to_owned();
        let _ = svc
            .submit_pair(&valid, "fp".into(), "label".into(), peer, now, mono)
            .expect("合法 code 应当颁发 token");

        // 之后的失败要重新从 0 计数 —— 第 FAILURE_THRESHOLD 次失败仍然只是 Invalid。
        for i in 0..FAILURE_THRESHOLD {
            let err = svc
                .submit_pair(&wrong, "fp".into(), "label".into(), peer, now, mono)
                .unwrap_err();
            assert_eq!(err, PairError::Invalid, "第 {i} 次失败应仍为 Invalid");
        }

        // 再一次失败才转入限流。
        let err = svc
            .submit_pair(&wrong, "fp".into(), "label".into(), peer, now, mono)
            .unwrap_err();
        assert_eq!(err, PairError::RateLimited);
    }

    /// `revoke` / `revoke_device` 必须穿透到底层 [`SessionRegistry`]。
    #[test]
    fn revoke_propagates_to_registry() {
        let mut svc = PairingService::new();
        let peer = loopback(20);
        let valid = svc.current_pairing_code().as_str().to_owned();

        let token = svc
            .submit_pair(
                &valid,
                "fp-revoke".into(),
                "device".into(),
                peer,
                SystemTime::now(),
                Instant::now(),
            )
            .expect("合法 code 应当颁发 token");

        assert!(svc.sessions().validate(&token).is_ok());
        svc.revoke(&token);
        assert!(svc.sessions().validate(&token).is_err());
    }

    #[test]
    fn revoke_device_drops_all_tokens_for_fp() {
        let mut svc = PairingService::new();
        let mono = Instant::now();
        let now = SystemTime::now();
        let fp_a: DeviceFingerprint = "fp-A".into();
        let fp_b: DeviceFingerprint = "fp-B".into();

        // 注：每次成功配对都会清空失败计数；这里我们用两个不同 IP 即可。
        let valid = svc.current_pairing_code().as_str().to_owned();
        let token_a1 = svc
            .submit_pair(&valid, fp_a.clone(), "A1".into(), loopback(30), now, mono)
            .expect("合法 code 应颁发 token a1");
        let token_a2 = svc
            .submit_pair(&valid, fp_a.clone(), "A2".into(), loopback(31), now, mono)
            .expect("合法 code 应颁发 token a2");
        let token_b = svc
            .submit_pair(&valid, fp_b.clone(), "B".into(), loopback(32), now, mono)
            .expect("合法 code 应颁发 token b");

        svc.revoke_device(&fp_a);

        assert!(svc.sessions().validate(&token_a1).is_err());
        assert!(svc.sessions().validate(&token_a2).is_err());
        assert!(svc.sessions().validate(&token_b).is_ok());
    }

    /// `list_sessions` 透传给底层注册表，行为与 [`SessionRegistry::list_sessions`] 一致。
    #[test]
    fn list_sessions_returns_all_active() {
        let mut svc = PairingService::new();
        assert!(svc.list_sessions().is_empty());

        let valid = svc.current_pairing_code().as_str().to_owned();
        let _ = svc
            .submit_pair(
                &valid,
                "fp".into(),
                "label".into(),
                loopback(40),
                SystemTime::now(),
                Instant::now(),
            )
            .expect("合法 code 应颁发 token");

        assert_eq!(svc.list_sessions().len(), 1);
    }

    /// `Default` 实现必须等价于 `new`：都构造出有效的 8 位 code。
    #[test]
    fn default_matches_new() {
        let a = PairingService::default();
        let b = PairingService::new();
        assert_eq!(a.current_pairing_code().as_str().len(), PAIRING_CODE_LEN);
        assert_eq!(b.current_pairing_code().as_str().len(), PAIRING_CODE_LEN);
    }

    /// 限流应当在窗口过期之后自然解除：使用同一 `submit_pair` 路径进行验证。
    /// 该测试只是 Sanity，限流窗口本身的边界由 [`crate::pair_rate_limit`] 单独覆盖。
    #[test]
    fn rate_limit_lifts_after_window_passes() {
        use crate::pair_rate_limit::FAILURE_WINDOW;

        let mut svc = PairingService::new();
        let peer = loopback(50);
        let now = SystemTime::now();
        let t0 = Instant::now();
        let wrong = pick_wrong_candidate(svc.current_pairing_code());

        for _ in 0..FAILURE_THRESHOLD {
            let _ = svc.submit_pair(&wrong, "fp".into(), "label".into(), peer, now, t0);
        }
        let blocked = svc
            .submit_pair(&wrong, "fp".into(), "label".into(), peer, now, t0)
            .unwrap_err();
        assert_eq!(blocked, PairError::RateLimited);

        // 跨过限流窗口后，限流自动解除——再次提交错误 code 应回落到 Invalid。
        let later = t0 + FAILURE_WINDOW + Duration::from_secs(1);
        let err = svc
            .submit_pair(&wrong, "fp".into(), "label".into(), peer, now, later)
            .unwrap_err();
        assert_eq!(err, PairError::Invalid);
    }
}
