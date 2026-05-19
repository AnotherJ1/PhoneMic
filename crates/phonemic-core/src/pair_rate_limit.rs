//! 配对失败计数器与限流窗口（按客户端 IP 维护）。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.13
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.3
//!   （`PairingService.failed_attempts: HashMap<IpAddr, FailureWindow>`）
//! - 需求来源：`.kiro/specs/phone-mic-voice-input/requirements.md` 7.5
//!   （连续 5 次失败后对该客户端 IP 进行 5 分钟速率限制；窗口结束自动重置）
//!
//! 本模块只提供按 IP 维护的内存数据结构，不负责实际的 HTTP 响应或
//! 计时驱动 —— 由调用方（任务 5.4 的 `RateLimit` 中间件）传入 `now: Instant`。
//! 这样既便于属性测试（任务 3.14）注入受控时钟，也避免在底层引入
//! `tokio::time` 之类的运行时耦合。
//!
//! ## 关于 `Instant`
//!
//! [`std::time::Instant`] 是单调时钟，不能从任意数值构造，因此测试中
//! 模拟时间推移的方法是：先取 `Instant::now()` 作为基准 `t0`，再用
//! `t0 + Duration::from_secs(...)` 推演未来时刻。
//!
//! ## 状态机概要
//!
//! 对每个 IP 至多维护一个 [`FailureWindow`]：
//!
//! - 首次失败：`{ count: 1, window_start: now }`。
//! - 后续失败若仍在窗口内（`now - window_start < FAILURE_WINDOW`）：
//!   `count` 递增，但在 [`FAILURE_THRESHOLD`] 处饱和（不再增长），
//!   既避免溢出，也避免上层因极端值产生不稳定的指标。
//! - 后续失败若已跨过窗口：丢弃旧窗口，按首次失败处理（`count = 1`，
//!   `window_start = now`）。
//! - [`PairRateLimiter::is_rate_limited`]：仅当存在未过期窗口且
//!   `count >= FAILURE_THRESHOLD` 时为真。窗口过期后即使尚未调用
//!   [`PairRateLimiter::reset_after_window`] 也不视为限流（懒过期）。
//! - [`PairRateLimiter::record_success`]：成功配对时立即清空该 IP 的
//!   计数，避免合法用户的偶发输错后又触发限流。
//! - [`PairRateLimiter::reset_after_window`]：主动清理所有已过期窗口，
//!   便于长时间运行后回收内存。

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// 触发限流的连续失败阈值（requirement 7.5）。
pub const FAILURE_THRESHOLD: u32 = 5;

/// 与 [`FAILURE_THRESHOLD`] 等价的别名，保留以兼容更早期的命名。
pub const MAX_FAILURES_PER_WINDOW: u32 = FAILURE_THRESHOLD;

/// 限流窗口长度，固定为 5 分钟（requirement 7.5）。
pub const FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);

/// 与 [`FAILURE_WINDOW`] 等价的别名，保留以兼容更早期的命名。
pub const WINDOW_DURATION: Duration = FAILURE_WINDOW;

/// 单个 IP 上的"连续失败"窗口。
///
/// 字段为公开是为了便于诊断与属性测试直接读写；外部代码通常无需直接构造，
/// 而是通过 [`PairRateLimiter`] 提供的高层 API 操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureWindow {
    /// 当前窗口内累计失败次数（包括首次失败本身）。
    ///
    /// 在 [`FAILURE_THRESHOLD`] 处饱和：达到上限后再调用
    /// [`PairRateLimiter::record_failure`] 不会再让 `count` 增长，但限流状态
    /// 依旧维持到窗口结束。
    pub count: u32,
    /// 当前窗口起点；用于判断 `now - window_start` 是否仍在
    /// [`FAILURE_WINDOW`] 内。
    pub window_start: Instant,
}

impl FailureWindow {
    /// 构造一次"刚发生首次失败"的窗口。
    #[must_use]
    pub fn started_now(now: Instant) -> Self {
        Self {
            count: 1,
            window_start: now,
        }
    }

    /// 当前窗口是否已过期（`now - window_start >= FAILURE_WINDOW`）。
    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.window_start) >= FAILURE_WINDOW
    }
}

/// 按客户端 IP 维护配对失败计数与限流窗口。
///
/// 内部不持有时钟 —— 所有需要时间的方法都接受 `now: Instant`，既使纯
/// 函数式属性测试（任务 3.14）成为可能，也让多线程调用方自由选择
/// `Instant::now()` 抑或测试桩时钟。
#[derive(Debug, Default)]
pub struct PairRateLimiter {
    failures: HashMap<IpAddr, FailureWindow>,
}

impl PairRateLimiter {
    /// 创建一个空的限流器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次配对失败。
    ///
    /// - 若该 IP 没有窗口或窗口已过期：开启新窗口，`count = 1`，
    ///   `window_start = now`。
    /// - 否则：在当前窗口内 `count` 自增，但在 [`FAILURE_THRESHOLD`] 处饱和。
    pub fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        self.failures
            .entry(ip)
            .and_modify(|w| {
                if w.is_expired(now) {
                    *w = FailureWindow::started_now(now);
                } else if w.count < FAILURE_THRESHOLD {
                    w.count += 1;
                }
            })
            .or_insert_with(|| FailureWindow::started_now(now));
    }

    /// 当前是否对指定 IP 处于限流状态。
    ///
    /// 仅当存在未过期窗口（`now - window_start < FAILURE_WINDOW`）且
    /// `count >= FAILURE_THRESHOLD` 时返回 `true`。
    #[must_use]
    pub fn is_rate_limited(&self, ip: IpAddr, now: Instant) -> bool {
        match self.failures.get(&ip) {
            Some(w) if !w.is_expired(now) => w.count >= FAILURE_THRESHOLD,
            _ => false,
        }
    }

    /// 主动清理所有已过期的窗口（窗口结束自动重置，requirement 7.5）。
    pub fn reset_after_window(&mut self, now: Instant) {
        self.failures.retain(|_, w| !w.is_expired(now));
    }

    /// 显式清除指定 IP 的失败窗口（管理接口 / 测试桩）。
    pub fn clear(&mut self, ip: IpAddr) {
        self.failures.remove(&ip);
    }

    /// 记录一次"成功配对"事件：清空该 IP 的失败计数与窗口。
    ///
    /// 这是 design §4.3 / requirement 7.5 的延伸语义：合法用户在偶发
    /// 几次输错后终于成功配对时，应当立即重置失败计数，避免后续意外
    /// 触发限流。语义上等价于 [`Self::clear`]，但显式命名以便业务路径
    /// （如 [`crate::pairing_service::PairingService::submit_pair`]）的
    /// 调用点更具可读性。
    pub fn record_success(&mut self, ip: IpAddr) {
        self.failures.remove(&ip);
    }

    /// 直接查询指定 IP 当前窗口（若存在），主要供测试与诊断接口使用。
    #[must_use]
    pub fn window_for(&self, ip: IpAddr) -> Option<FailureWindow> {
        self.failures.get(&ip).copied()
    }

    /// 当前持有失败窗口的 IP 数量；便于上层做指标埋点。
    #[must_use]
    pub fn tracked_ip_count(&self) -> usize {
        self.failures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    /// requirement 7.5：阈值之前都不应触发限流，达到阈值后必须限流。
    #[test]
    fn first_four_failures_do_not_rate_limit_fifth_does() {
        let mut limiter = PairRateLimiter::new();
        let client = ip(192, 168, 1, 10);
        let t0 = Instant::now();

        for i in 1..FAILURE_THRESHOLD {
            limiter.record_failure(client, t0);
            assert_eq!(limiter.window_for(client).unwrap().count, i);
            assert!(
                !limiter.is_rate_limited(client, t0),
                "第 {i} 次失败后不应被限流",
            );
        }

        limiter.record_failure(client, t0);
        assert_eq!(
            limiter.window_for(client).unwrap().count,
            FAILURE_THRESHOLD,
        );
        assert!(
            limiter.is_rate_limited(client, t0),
            "达到阈值 {FAILURE_THRESHOLD} 次失败后应进入限流",
        );
    }

    /// requirement 7.5：限流窗口边界 —— 4:59 仍受限，5:00 起解除。
    #[test]
    fn rate_limit_active_until_window_elapses() {
        let mut limiter = PairRateLimiter::new();
        let client = ip(10, 0, 0, 1);
        let t0 = Instant::now();

        for _ in 0..FAILURE_THRESHOLD {
            limiter.record_failure(client, t0);
        }
        assert!(limiter.is_rate_limited(client, t0));

        let almost = t0 + Duration::from_secs(4 * 60 + 59);
        assert!(limiter.is_rate_limited(client, almost));

        let at_boundary = t0 + FAILURE_WINDOW;
        assert!(!limiter.is_rate_limited(client, at_boundary));

        let later = t0 + FAILURE_WINDOW + Duration::from_secs(1);
        assert!(!limiter.is_rate_limited(client, later));
    }

    /// 窗口结束后再失败应当重新开窗，而不是在旧 count 上累加。
    #[test]
    fn record_failure_after_window_expiry_starts_new_window() {
        let mut limiter = PairRateLimiter::new();
        let client = ip(172, 16, 0, 5);
        let t0 = Instant::now();

        for _ in 0..FAILURE_THRESHOLD {
            limiter.record_failure(client, t0);
        }
        assert!(limiter.is_rate_limited(client, t0));

        let after_window = t0 + FAILURE_WINDOW + Duration::from_secs(1);
        limiter.record_failure(client, after_window);
        let w = limiter.window_for(client).expect("窗口应被重建");
        assert_eq!(w.count, 1, "跨过窗口后第一次失败应当重置为 1");
        assert_eq!(w.window_start, after_window);
        assert!(!limiter.is_rate_limited(client, after_window));
    }

    /// `reset_after_window` 应只清理过期条目，未过期的保留。
    #[test]
    fn reset_after_window_drops_only_expired_entries() {
        let mut limiter = PairRateLimiter::new();
        let stale = ip(192, 168, 1, 100);
        let fresh = ip(192, 168, 1, 101);
        let t0 = Instant::now();

        limiter.record_failure(stale, t0);
        let later = t0 + FAILURE_WINDOW + Duration::from_secs(10);
        limiter.record_failure(fresh, later);

        assert_eq!(limiter.tracked_ip_count(), 2);

        limiter.reset_after_window(later);
        assert!(limiter.window_for(stale).is_none(), "过期条目应被清除");
        assert!(limiter.window_for(fresh).is_some(), "未过期条目应被保留");
        assert_eq!(limiter.tracked_ip_count(), 1);
    }

    /// 不同 IP 的失败计数互相独立。
    #[test]
    fn each_ip_is_tracked_independently() {
        let mut limiter = PairRateLimiter::new();
        let alice = ip(192, 168, 1, 2);
        let bob = ip(192, 168, 1, 3);
        let now = Instant::now();

        for _ in 0..FAILURE_THRESHOLD {
            limiter.record_failure(alice, now);
        }

        assert!(limiter.is_rate_limited(alice, now));
        assert!(!limiter.is_rate_limited(bob, now));

        limiter.record_failure(bob, now);
        assert_eq!(limiter.window_for(bob).unwrap().count, 1);
        assert_eq!(
            limiter.window_for(alice).unwrap().count,
            FAILURE_THRESHOLD,
        );
    }

    /// 在限流期间继续失败：count 在阈值处饱和，限流状态保持。
    #[test]
    fn additional_failures_during_block_clamp_at_threshold() {
        let mut limiter = PairRateLimiter::new();
        let client = ip(192, 168, 1, 7);
        let now = Instant::now();

        for _ in 0..(FAILURE_THRESHOLD + 3) {
            limiter.record_failure(client, now);
        }
        assert_eq!(
            limiter.window_for(client).unwrap().count,
            FAILURE_THRESHOLD,
            "count 应在阈值处饱和",
        );
        assert!(limiter.is_rate_limited(client, now));
    }

    /// `clear(ip)` 显式重置某个 IP 的窗口。
    #[test]
    fn clear_removes_only_target_ip() {
        let mut limiter = PairRateLimiter::new();
        let alice = ip(192, 168, 1, 1);
        let bob = ip(192, 168, 1, 2);
        let now = Instant::now();

        for _ in 0..FAILURE_THRESHOLD {
            limiter.record_failure(alice, now);
        }
        limiter.record_failure(bob, now);
        assert_eq!(limiter.tracked_ip_count(), 2);

        limiter.clear(alice);
        assert!(limiter.window_for(alice).is_none());
        assert!(!limiter.is_rate_limited(alice, now));
        assert!(
            limiter.window_for(bob).is_some(),
            "clear 不应影响其他 IP",
        );

        limiter.clear(ip(10, 10, 10, 10));
        assert_eq!(limiter.tracked_ip_count(), 1);
    }

    /// `record_success` 立即清空该 IP 的失败计数。
    #[test]
    fn record_success_clears_failure_window() {
        let mut limiter = PairRateLimiter::new();
        let client = ip(192, 168, 1, 50);
        let now = Instant::now();

        for _ in 0..FAILURE_THRESHOLD {
            limiter.record_failure(client, now);
        }
        assert!(limiter.is_rate_limited(client, now));

        limiter.record_success(client);
        assert!(limiter.window_for(client).is_none());
        assert!(!limiter.is_rate_limited(client, now));
    }

    /// 默认实现等价于 `new`。
    #[test]
    fn default_matches_new() {
        let a = PairRateLimiter::default();
        let b = PairRateLimiter::new();
        assert_eq!(a.tracked_ip_count(), b.tracked_ip_count());
    }

    /// `FailureWindow::is_expired` 的语义校验：`>=` 视为过期。
    #[test]
    fn failure_window_is_expired_uses_inclusive_boundary() {
        let t0 = Instant::now();
        let w = FailureWindow::started_now(t0);

        assert!(!w.is_expired(t0));
        assert!(!w.is_expired(t0 + FAILURE_WINDOW - Duration::from_nanos(1)));
        assert!(w.is_expired(t0 + FAILURE_WINDOW));
        assert!(w.is_expired(t0 + FAILURE_WINDOW + Duration::from_secs(1)));
    }

    /// 别名常量与"权威常量"保持等值，防止任意一处改动忘了同步。
    #[test]
    fn aliases_match_canonical_constants() {
        assert_eq!(MAX_FAILURES_PER_WINDOW, FAILURE_THRESHOLD);
        assert_eq!(WINDOW_DURATION, FAILURE_WINDOW);
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 19: 配对限流（pair rate limit）
//
// 任务 3.14：基于 `pair_event_seq()` 生成器与可控时间，断言：
//   1. 任意失败序列下连续 5 次失败后 5 分钟内拒绝；
//   2. 窗口过期后计数重置；
//   3. 限流期间 `is_rate_limited` 始终为 `true`。
//
// 这里 `pair_event_seq` 用 proptest 的 strategy 直接生成，
// 不引入额外辅助 crate；时间由测试自行推进，故无需真实时钟。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    /// 一次配对事件：要么是失败，要么是把"测试时钟"向前推进若干秒。
    #[derive(Debug, Clone)]
    enum PairEvent {
        Failure,
        Advance(u64), // seconds
    }

    fn pair_event_seq() -> impl Strategy<Value = Vec<PairEvent>> {
        prop::collection::vec(
            prop_oneof![
                Just(PairEvent::Failure),
                (1u64..600).prop_map(PairEvent::Advance),
            ],
            0..40,
        )
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 19: 配对限流
        #[test]
        fn property_19_rate_limit_after_threshold(events in pair_event_seq()) {
            let mut limiter = PairRateLimiter::new();
            let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
            let t0 = Instant::now();
            let mut cur = t0;

            // 重放：任意失败 / 时间推进序列下，
            //   * 若当前窗口未过期且 count >= 阈值 → 限流
            //   * 否则不限流
            for ev in &events {
                match ev {
                    PairEvent::Failure => {
                        limiter.record_failure(ip, cur);
                    }
                    PairEvent::Advance(secs) => {
                        cur += Duration::from_secs(*secs);
                    }
                }

                // 不变量：is_rate_limited 与底层窗口状态一致。
                let expected = match limiter.window_for(ip) {
                    Some(w) if !w.is_expired(cur) => w.count >= FAILURE_THRESHOLD,
                    _ => false,
                };
                prop_assert_eq!(limiter.is_rate_limited(ip, cur), expected);
            }
        }

        // Feature: phone-mic-voice-input, Property 19: 配对限流（边界）
        // 5 次连续失败后 5 分钟内必拒绝；过期后再失败计数重置回 1。
        #[test]
        fn property_19_five_failures_then_block_then_reset(
            advance_secs in 1u64..295,
        ) {
            let mut limiter = PairRateLimiter::new();
            let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));
            let t0 = Instant::now();

            for _ in 0..FAILURE_THRESHOLD {
                limiter.record_failure(ip, t0);
            }
            prop_assert!(limiter.is_rate_limited(ip, t0));

            // 在 [1, 5min) 范围内任意推进，仍应被限流。
            let mid = t0 + Duration::from_secs(advance_secs);
            prop_assert!(limiter.is_rate_limited(ip, mid));

            // 跨过 5 分钟后限流必然解除。
            let after = t0 + FAILURE_WINDOW + Duration::from_secs(1);
            prop_assert!(!limiter.is_rate_limited(ip, after));

            // 过期后再失败：计数应重置为 1，不再处于限流。
            limiter.record_failure(ip, after);
            let w = limiter.window_for(ip).expect("recreated window");
            prop_assert_eq!(w.count, 1);
            prop_assert!(!limiter.is_rate_limited(ip, after));
        }
    }
}
