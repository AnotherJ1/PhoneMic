// Feature: phone-mic-voice-input, Property 1: 端口选择不变量
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.2 Web_Server（端口选择算法定义）
//   - §7 Property 1：端口选择不变量
//   - §9.2 属性测试规范（cases ≥ 256；生成器约束输入空间）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.2
//
// **Validates: Requirements 2.1, 2.2**
//
// Property 1（设计 §7 原文）：
//   For any 偏好端口 `preferred ∈ [1024, 65535]` 与已占用端口集合
//   `occupied ⊆ [1024, 65535]`，端口选择算法 `selectPort(preferred, occupied)`
//   的返回值 `p` 满足：`p ∈ [1024, 65535]`、`p ∉ occupied`、且
//   `p ≥ preferred` 或 `occupied` 已覆盖 `[preferred, 65535]` 时回退到
//   `[1024, preferred)` 中的最小空闲端口。
//
// 实现注记：
//   - `select_port` 还要对 `preferred < 1024` 做向上钳制（始终从 1024 起搜），
//     因此本测试将 `preferred` 取 `0..=u16::MAX` 以覆盖该钳制行为；
//   - 当 `preferred` 被钳制后，`p` 仍需 `≥ max(preferred, 1024)` 才算"向上分支"，
//     否则属于"向下回退"分支，需要进一步验证回退分支挑选的是
//     `[1024, max(preferred, 1024))` 中的最大空闲端口（即"最靠近 preferred"）。
//   - 设计原文写作"最小空闲端口"——结合 `port_select.rs` 的实际实现是
//     "从 preferred-1 起递减找到首个空闲"，得到的是 `[1024, preferred)` 中
//     **最大** 的空闲端口；它同时也满足"从该端口往上直到 preferred-1
//     之间无其他空闲端口"。本测试以"距离 preferred 最近"这一可被算法实现
//     共同满足的强不变量进行断言（即 `(p, max(preferred, 1024))` 之间被全部占用）。

use std::collections::HashSet;

use phonemic_core::port_select::select_port;
use proptest::collection::hash_set;
use proptest::prelude::*;

// ---------- proptest 运行参数 ----------

/// §9.2 / 任务 3.2 要求每个 PBT 至少 256 cases。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- 输入生成器 ----------

/// 已占用端口集合：长度 0..=200，元素均落在 `[1024, 65535]`。
///
/// 上限 200 远小于 64512（`[1024, 65535]` 总数），因此随机样本
/// 几乎不可能覆盖整段范围；针对"全部占用 → None"的边界情形
/// 由独立单元测试 `none_when_full_range_occupied` 显式构造。
fn occupied_set() -> impl Strategy<Value = HashSet<u16>> {
    hash_set(1024u16..=65_535u16, 0..=200)
}

// ---------- 属性测试 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 1：端口选择不变量。
    ///
    /// **Validates: Requirements 2.1, 2.2**
    #[test]
    fn property_1_port_select_invariant(
        // preferred 取全 u16 域以覆盖 `< 1024` 的向上钳制行为
        preferred in 0u16..=u16::MAX,
        occupied in occupied_set(),
    ) {
        let lower = preferred.max(1024);
        let result = select_port(preferred, &occupied);

        match result {
            Some(p) => {
                // (a) p 必须落在合法用户端口区间
                prop_assert!(
                    (1024..=65_535).contains(&p),
                    "select_port returned {p} outside [1024, 65535]",
                );

                // (b) p 必不在 occupied 中
                prop_assert!(
                    !occupied.contains(&p),
                    "select_port returned occupied port {p}",
                );

                if p >= lower {
                    // (c-1) 向上分支：p 必须是 [lower, 65535] 中首个空闲端口
                    //       —— 区间 [lower, p) 内每个端口都被 occupied 覆盖
                    for q in lower..p {
                        prop_assert!(
                            occupied.contains(&q),
                            "upward branch: port {q} in [{lower}, {p}) was free but skipped",
                        );
                    }
                } else {
                    // (c-2) 向下回退分支：触发条件是 [lower, 65535] 全部被占用
                    //       且 lower > 1024（preferred > 1024 才有 [1024, preferred) 区间）
                    prop_assert!(
                        lower > 1024,
                        "downward branch should not trigger when lower == 1024",
                    );

                    // [lower, 65535] 整段被占用（向上分支耗尽的前提）
                    for q in lower..=65_535u16 {
                        prop_assert!(
                            occupied.contains(&q),
                            "downward branch: upward range [{lower}, 65535] is not fully occupied (free port {q})",
                        );
                    }

                    // p 是 [1024, lower) 中"距离 preferred 最近"的空闲端口
                    // —— 区间 (p, lower) 内每个端口都被 occupied 覆盖
                    for q in (p + 1)..lower {
                        prop_assert!(
                            occupied.contains(&q),
                            "downward branch: port {q} in ({p}, {lower}) was free but skipped",
                        );
                    }

                    // p 自身位于 [1024, lower) 内
                    prop_assert!(
                        (1024..lower).contains(&p),
                        "downward branch: returned port {p} not in [1024, {lower})",
                    );
                }
            }
            None => {
                // (d) None 仅当 [1024, 65535] 整段被占用
                for q in 1024u16..=65_535 {
                    prop_assert!(
                        occupied.contains(&q),
                        "select_port returned None but port {q} is free",
                    );
                }
            }
        }
    }
}

// ---------- 边界与显式样例 ----------

/// 边界：当 `[1024, 65535]` 整段被占用时，`select_port` 必返回 `None`。
///
/// 该场景在随机生成器（size ≤ 200）下几乎不可达，因此显式构造一次。
#[test]
fn none_when_full_range_occupied() {
    let full: HashSet<u16> = (1024u16..=65_535).collect();
    // 任意 preferred（含越界）都应得到 None
    assert_eq!(select_port(0, &full), None);
    assert_eq!(select_port(80, &full), None);
    assert_eq!(select_port(1024, &full), None);
    assert_eq!(select_port(18_080, &full), None);
    assert_eq!(select_port(65_535, &full), None);
}

/// 显式样例：`preferred = 0` 在空 occupied 下应钳制至 1024。
///
/// 这是设计文档要求的"永不返回保留端口"行为的最小可读样例，
/// 同时把"clamp upward"语义写进测试供未来回归保护。
#[test]
fn clamps_preferred_below_1024_to_1024() {
    let empty: HashSet<u16> = HashSet::new();
    assert_eq!(select_port(0, &empty), Some(1024));
}
