//! 任务 3.1：端口选择算法 `select_port`。
//!
//! 给定偏好端口与已占用端口集合，返回首个可用端口，永不打开真实 socket，
//! 以便作为纯函数被属性测试（任务 3.2 / Property 1）覆盖。
//!
//! # 行为约定
//!
//! 1. **优先向上搜索**：从 `max(preferred, 1024)` 起递增至 `65_535`，
//!    返回首个不在 `occupied` 中的端口。
//! 2. **回退向下搜索**：若 `[max(preferred, 1024), 65_535]` 全部占用，
//!    则从 `preferred - 1` 起递减至 `1024`，返回首个不在 `occupied`
//!    中的端口（即 `[1024, preferred)` 中最靠近 `preferred` 的空闲端口）。
//! 3. **永不返回保留端口**：返回值始终落在 `[1024, 65_535]`。
//!    若 `preferred < 1024`，向下搜索直接跳过；若 `preferred == 1024`，
//!    向下搜索范围为空。
//! 4. **None 仅当全部占用**：`[1024, 65_535]` 整段被 `occupied` 覆盖时返回 `None`，
//!    由调用方映射为 `PORT_UNAVAILABLE`（设计 §8.1）。
//!
//! # 关联
//!
//! - Validates: Requirements 2.1, 2.2
//! - Design: §4.2 Web_Server / Property 1

use std::collections::HashSet;

/// 在 `[1024, 65_535]` 范围内挑选一个未被占用的端口。
///
/// 详细行为参见模块级文档。`occupied` 中可能含有 `< 1024` 的元素，
/// 它们对结果不会产生影响（搜索范围始终从 1024 起步）。
pub fn select_port(preferred: u16, occupied: &HashSet<u16>) -> Option<u16> {
    // 1) 向上搜索：从 max(preferred, 1024) 到 65_535
    let upward_start = preferred.max(1024);
    for p in upward_start..=u16::MAX {
        if !occupied.contains(&p) {
            return Some(p);
        }
    }

    // 2) 向下回退：仅当 preferred > 1024 时存在 [1024, preferred) 的有效区间
    if preferred > 1024 {
        // preferred - 1 安全：此处 preferred ≥ 1025
        for p in (1024u16..=(preferred - 1)).rev() {
            if !occupied.contains(&p) {
                return Some(p);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occ(items: &[u16]) -> HashSet<u16> {
        items.iter().copied().collect()
    }

    #[test]
    fn returns_preferred_when_free() {
        assert_eq!(select_port(18080, &occ(&[])), Some(18080));
        assert_eq!(select_port(18080, &occ(&[1024, 65535])), Some(18080));
    }

    #[test]
    fn advances_upward_when_preferred_taken() {
        assert_eq!(select_port(18080, &occ(&[18080])), Some(18081));
        assert_eq!(
            select_port(18080, &occ(&[18080, 18081, 18082])),
            Some(18083),
        );
    }

    #[test]
    fn falls_back_downward_when_upper_range_exhausted() {
        // [18080, 65535] 全部占用，应回退到 [1024, 18080) 中最靠近 18080 的空闲端口
        let mut occupied: HashSet<u16> = (18080u16..=65_535).collect();
        assert_eq!(select_port(18080, &occupied), Some(18079));

        // 若 18079 也占用，应继续向下找到 18078
        occupied.insert(18079);
        assert_eq!(select_port(18080, &occupied), Some(18078));
    }

    #[test]
    fn returns_none_when_all_user_ports_occupied() {
        let occupied: HashSet<u16> = (1024u16..=65_535).collect();
        assert_eq!(select_port(18080, &occupied), None);
        assert_eq!(select_port(1024, &occupied), None);
        assert_eq!(select_port(65_535, &occupied), None);
    }

    #[test]
    fn preferred_below_1024_starts_upward_at_1024() {
        // preferred = 0：等价于从 1024 开始向上找；不应返回 < 1024 的端口
        assert_eq!(select_port(0, &occ(&[])), Some(1024));
        assert_eq!(select_port(0, &occ(&[1024])), Some(1025));

        // preferred = 80：同理，且 occupied 中的 < 1024 项被忽略
        assert_eq!(select_port(80, &occ(&[80, 1024])), Some(1025));
    }

    #[test]
    fn preferred_below_1024_never_falls_back_to_reserved_ports() {
        // 即使 [1024, 65535] 全部被占用，也不应回退到 < 1024 的端口
        let occupied: HashSet<u16> = (1024u16..=65_535).collect();
        assert_eq!(select_port(0, &occupied), None);
        assert_eq!(select_port(500, &occupied), None);
    }

    #[test]
    fn preferred_at_max_falls_back_when_taken() {
        // preferred = 65535 命中且空闲
        assert_eq!(select_port(65_535, &occ(&[])), Some(65_535));

        // 65535 被占，应回退到 65534
        assert_eq!(select_port(65_535, &occ(&[65_535])), Some(65_534));
    }

    #[test]
    fn preferred_at_1024_skips_downward_search() {
        // preferred = 1024：[1024, preferred) 为空，向下搜索应被跳过
        let mut occupied: HashSet<u16> = (1024u16..=65_535).collect();
        assert_eq!(select_port(1024, &occupied), None);

        // 仅 1024 占用，应向上找到 1025
        occupied.clear();
        occupied.insert(1024);
        assert_eq!(select_port(1024, &occupied), Some(1025));
    }
}
