//! [`InjectionPlanner`] —— 把待注入文本编译为 [`InjectionEvent`] 序列的纯函数。
//!
//! 任务来源：tasks.md 7.2。
//! 设计来源：design.md §4.5（"键盘注入抽象"）。
//!
//! 不变量（与 design §7 Property 12 / 13 / 14 一一对应）：
//!
//! - **Property 12（码点保留）**：除 `\n` 外的每个 Unicode 标量值都对应
//!   一个 [`EventKind::Char(cp)`]，cp 与 `text.chars().map(|c| c as u32)`
//!   一一对应、顺序一致。
//! - **Property 13（换行映射）**：每个 `\n` 映射为 [`EventKind::Enter`]，
//!   出现位置与 `text` 中 `\n` 出现位置严格一致。
//! - **Property 14（注入延迟）**：相邻事件 `ts` 间隔 ≥ `delay_ms`。
//!
//! 该模块**完全无副作用**：不调用任何 OS API、不读时钟（基准 `Instant`
//! 由调用方传入或使用一个固定的 `Instant::now()` 起点），便于属性测试快速回放。

use std::time::{Duration, Instant};

use crate::{EventKind, InjectionEvent};

/// 注入计划器。
///
/// 通过 [`InjectionPlanner::with_base`] 自定义起点；通常的属性测试只需用
/// [`plan_injection`] 短捷函数即可。
#[derive(Debug, Clone, Copy)]
pub struct InjectionPlanner {
    base: Instant,
    delay_ms: u32,
}

impl InjectionPlanner {
    /// 用当前时刻作为基准、`delay_ms` 作为字符间延迟。
    #[must_use]
    pub fn new(delay_ms: u32) -> Self {
        Self {
            base: Instant::now(),
            delay_ms,
        }
    }

    /// 用调用方提供的基准时刻构造，便于测试中重放确定性序列。
    #[must_use]
    pub fn with_base(base: Instant, delay_ms: u32) -> Self {
        Self { base, delay_ms }
    }

    /// 把文本编译为 [`InjectionEvent`] 序列。
    #[must_use]
    pub fn plan(&self, text: &str) -> Vec<InjectionEvent> {
        let mut out = Vec::with_capacity(text.chars().count());
        let mut idx = 0u64;
        for ch in text.chars() {
            // 每个事件的 ts = base + idx * delay_ms。
            let ts = self.base + Duration::from_millis(idx * u64::from(self.delay_ms));
            let event = if ch == '\n' {
                InjectionEvent {
                    kind: EventKind::Enter,
                    codepoint: None,
                    ts,
                }
            } else {
                InjectionEvent {
                    kind: EventKind::Char(ch as u32),
                    codepoint: Some(ch as u32),
                    ts,
                }
            };
            out.push(event);
            idx += 1;
        }
        out
    }
}

/// 顶层快捷函数：以"现在"为基准、`delay_ms` 作为延迟，规划 `text`。
#[must_use]
pub fn plan_injection(text: &str, delay_ms: u32) -> Vec<InjectionEvent> {
    InjectionPlanner::new(delay_ms).plan(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn collect_codepoints(events: &[InjectionEvent]) -> Vec<u32> {
        events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Char(cp) => Some(cp),
                EventKind::Enter => None,
            })
            .collect()
    }

    #[test]
    fn plan_empty_text_yields_no_events() {
        assert!(plan_injection("", 0).is_empty());
        assert!(plan_injection("", 100).is_empty());
    }

    #[test]
    fn plan_ascii_text_preserves_codepoints() {
        let events = plan_injection("abc", 0);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, EventKind::Char('a' as u32));
        assert_eq!(events[1].kind, EventKind::Char('b' as u32));
        assert_eq!(events[2].kind, EventKind::Char('c' as u32));
    }

    #[test]
    fn plan_handles_bmp_and_supplementary_codepoints() {
        // "你" (U+4F60, BMP) 与 "🦀" (U+1F980, 非 BMP)
        let events = plan_injection("你🦀", 0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::Char(0x4F60));
        assert_eq!(events[1].kind, EventKind::Char(0x1F980));
    }

    #[test]
    fn plan_maps_newline_to_enter() {
        let events = plan_injection("a\nb\n", 0);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, EventKind::Char('a' as u32));
        assert_eq!(events[1].kind, EventKind::Enter);
        assert_eq!(events[2].kind, EventKind::Char('b' as u32));
        assert_eq!(events[3].kind, EventKind::Enter);
    }

    #[test]
    fn plan_respects_inter_event_delay() {
        let base = Instant::now();
        let p = InjectionPlanner::with_base(base, 10);
        let events = p.plan("ab");
        assert_eq!(events[1].ts.saturating_duration_since(events[0].ts), Duration::from_millis(10));
    }

    // ---------- Property tests ----------

    proptest! {
        /// Property 12：码点保留。
        #[test]
        fn property_12_codepoint_preservation(text in "[a-zA-Z0-9\u{4E00}-\u{9FFF}\u{1F300}-\u{1F6FF}]{0,32}") {
            let events = plan_injection(&text, 0);
            // 文本无 \n 时，事件数量 == 字符数；codepoint 序列保持。
            let expected: Vec<u32> = text.chars().map(|c| c as u32).collect();
            prop_assert_eq!(collect_codepoints(&events), expected);
        }

        /// Property 13：换行映射。
        #[test]
        fn property_13_newline_mapping(text in "[a-z\n]{0,32}") {
            let events = plan_injection(&text, 0);
            prop_assert_eq!(events.len(), text.chars().count());
            for (i, ch) in text.chars().enumerate() {
                if ch == '\n' {
                    prop_assert_eq!(events[i].kind, EventKind::Enter);
                } else {
                    prop_assert_eq!(events[i].kind, EventKind::Char(ch as u32));
                }
            }
        }

        /// Property 14：相邻事件 `ts` 间隔 ≥ delay_ms。
        #[test]
        fn property_14_delay_lower_bound(
            text in "[a-z]{0,16}",
            delay in 0u32..500,
        ) {
            let base = Instant::now();
            let events = InjectionPlanner::with_base(base, delay).plan(&text);
            for w in events.windows(2) {
                let gap = w[1].ts.saturating_duration_since(w[0].ts);
                prop_assert!(
                    gap >= Duration::from_millis(u64::from(delay)),
                    "gap={:?} < delay={}ms",
                    gap,
                    delay
                );
            }
        }
    }
}
