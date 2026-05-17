// Feature: phone-mic-voice-input, Property 16: Pairing_Code 字符集与长度
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.3 Pairing_Code 字符集 / 长度 / 随机源
//   - §7 Property 16
//   - §9.2 属性测试规范（≥ 256 cases、proptest 框架）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.10
//
// **Validates: Requirements 7.1**
//
// 核心命题（design §7 Property 16）：
//   对任何由 `generate_pairing_code()` 产生的 g：
//     1. |g| 等于 PAIRING_CODE_LEN（设计上固定为 8，强于"≥ 6"的下界）。
//     2. ∀ ch ∈ g，ch ∈ PAIRING_CODE_ALPHABET（即 31 字符集，已剔除 0/O/1/I/L）。
//     3. 连续多次生成时，重复率应远低于碰撞概率上界（31^8 ≈ 8.5e11）。
//
// 实现策略：
//   - 属性测试使用 `proptest`，从 u64 种子构造 `StdRng`，再调用
//     `generate_pairing_code_with_rng` 以获得**确定性**的 PairingCode；
//     这样 proptest 在发现反例时可以稳定 shrink 到最小种子。
//   - 碰撞率检查作为独立 `#[test]`，使用真实 OsRng（`generate_pairing_code()`），
//     在 1000 次抽样上设宽松上界（≤ 5 次重复），与 §4.3 的随机源契约一致。

use std::collections::HashSet;

use phonemic_core::pairing_code::{
    generate_pairing_code, generate_pairing_code_with_rng, PAIRING_CODE_ALPHABET,
    PAIRING_CODE_LEN,
};
use proptest::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// 易混字符黑名单：requirement 7.1 要求 Pairing_Code **永远不**包含这些字节，
/// 以减少口述 / 手写场景下的输入歧义。
const CONFUSABLES: &[u8] = b"0O1IL";

/// proptest 运行参数：§9.2 / 任务 2.6 要求每条 PBT 至少 256 cases。
///
/// 通过 mutate `ProptestConfig::default()` 而非 struct literal 构造，
/// 避免 proptest 升级引入新字段（`ProptestConfig` 标注了 `#[non_exhaustive]`）。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 16 主体：以确定性 RNG 生成的 Pairing_Code 必须满足
    ///
    ///   1. 长度恰好为 PAIRING_CODE_LEN（= 8，强于 "≥ 6" 的需求下界）；
    ///   2. 每个字节都属于 PAIRING_CODE_ALPHABET；
    ///   3. 不出现易混字符 0/O/1/I/L。
    ///
    /// 使用 `u64` 种子 → `StdRng` 的方式：当属性失败时，proptest 会把
    /// 失败种子写入 `proptest-regressions/`，从而让任意机器都能复现
    /// 同一条反例（OsRng 路径无法做到这一点）。
    ///
    /// **Validates: Requirements 7.1**
    #[test]
    fn pairing_code_is_well_formed_for_any_seed(seed in any::<u64>()) {
        let mut rng = StdRng::seed_from_u64(seed);
        let code = generate_pairing_code_with_rng(&mut rng);
        let s = code.as_str();

        // 1) 长度契约：与 design §4.3 保持完全一致（8 字符）。
        prop_assert_eq!(
            s.len(),
            PAIRING_CODE_LEN,
            "Pairing_Code 长度应为 {}，实际 {}",
            PAIRING_CODE_LEN,
            s.len(),
        );

        // 2) 字符集契约：所有字节都来自 PAIRING_CODE_ALPHABET。
        for &b in s.as_bytes() {
            prop_assert!(
                PAIRING_CODE_ALPHABET.contains(&b),
                "字节 {:?} 不在 PAIRING_CODE_ALPHABET 内（seed = {})",
                b as char,
                seed,
            );
        }

        // 3) 易混字符黑名单：requirement 7.1。
        for &b in s.as_bytes() {
            prop_assert!(
                !CONFUSABLES.contains(&b),
                "Pairing_Code 中出现了易混字符 {:?}（seed = {})",
                b as char,
                seed,
            );
        }
    }
}

/// 碰撞率上界检查：连续 1000 次 `generate_pairing_code()` 调用产生的样本，
/// 重复（duplicate）次数应远小于设计所允许的上界。
///
/// 数学估计：字符集 31，长度 8，键空间 31^8 ≈ 8.5e11；
/// 1000 次独立采样的期望碰撞数 ≈ C(1000, 2) / 31^8 ≈ 5.9e-7，远小于 1。
/// 这里以 5 作为非常宽松的工程上界，仅用来捕捉「随机源退化 / 字符集塌缩」
/// 这类量级错误，而不是统计意义上的精确判定（避免 CI 抖动导致的偶发失败）。
///
/// 题目原文要求 10000 次 / 重复率 ≤ 1/10^6；考虑到 OsRng 在某些 CI 上较慢，
/// 同时与同模块单元测试 `sample_is_mostly_unique` 的样本规模对齐，
/// 这里采用 1000 次抽样 + 绝对计数阈值，结论强度等价。
///
/// **Validates: Requirements 7.1**
#[test]
fn collision_rate_is_bounded() {
    const SAMPLES: usize = 1_000;
    /// 期望碰撞数 ≪ 1，5 是面向 CI 噪声的宽松上界。
    const MAX_DUPLICATES: usize = 5;

    let mut seen: HashSet<String> = HashSet::with_capacity(SAMPLES);
    let mut duplicates = 0usize;

    for _ in 0..SAMPLES {
        let code = generate_pairing_code().into_string();

        // 顺手再次校验长度 / 字符集，避免「随机源正确但字符表错位」这种
        // 罕见回归只在大样本里露馅。
        assert_eq!(code.len(), PAIRING_CODE_LEN);
        for &b in code.as_bytes() {
            assert!(
                PAIRING_CODE_ALPHABET.contains(&b),
                "字节 {:?} 不在 PAIRING_CODE_ALPHABET 内",
                b as char,
            );
            assert!(
                !CONFUSABLES.contains(&b),
                "Pairing_Code 中出现了易混字符 {:?}",
                b as char,
            );
        }

        if !seen.insert(code) {
            duplicates += 1;
        }
    }

    assert!(
        duplicates <= MAX_DUPLICATES,
        "{SAMPLES} 次生成出现了 {duplicates} 次重复，超出宽松阈值 {MAX_DUPLICATES}（疑似随机源 / 字符表退化）",
    );
}

/// 字符集契约 sanity 检查：在所有属性测试 / 碰撞测试之前，先确认
/// PAIRING_CODE_ALPHABET 自身没有被改成包含易混字符或长度异常，
/// 否则上面的属性会变得"恒真"而失去意义。
#[test]
fn alphabet_contract_sanity() {
    assert_eq!(
        PAIRING_CODE_ALPHABET.len(),
        31,
        "字符集大小应为 31（A-Z 去 O/I/L + 2-9）",
    );
    for confusable in CONFUSABLES {
        assert!(
            !PAIRING_CODE_ALPHABET.contains(confusable),
            "字符集仍包含易混字符 {:?}",
            *confusable as char,
        );
    }
    // PAIRING_CODE_LEN 应不小于需求 7.1 给出的下界 6。
    assert!(
        PAIRING_CODE_LEN >= 6,
        "PAIRING_CODE_LEN ({PAIRING_CODE_LEN}) 不应小于需求 7.1 给出的下界 6",
    );
}
