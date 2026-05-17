// Feature: phone-mic-voice-input, Property 17: Pairing_Code 校验
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.3 Pairing_Service / `verify_pairing_code`
//   - §7 Property 17：accept ⟺ candidate == current
//   - §9.2 属性测试规范（≥ 256 cases）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.12
//
// **Validates: Requirements 7.2**
//
// 本文件覆盖 `phonemic_core::pairing_code::verify_pairing_code` 的等价性
// 与长度边界，并配合 `parse_pairing_code` 验证「合法 8 位 alphabet 输入」
// 的 round-trip 与 verify 协同。
//
// 关于「运行时间不依赖匹配前缀长度」（恒定时间）这一性质：
// - 实现层使用 `subtle::ConstantTimeEq`（设计 §4.3 / requirement 7.2 明确指定），
//   这是抵御时序侧信道的标准库级防御；
// - 在 `proptest` / CI 环境下用 wall-clock 做时序方差断言不可靠（GC、调度、
//   CPU 频率波动等噪声远大于早退分支差），因此本文件**只验证语义等价**，
//   恒定时间属性留给 `subtle` 库的形式化保证 + `pairing_code.rs` 内的
//   `#[ignore]` 时序 smoke test（仅供本地手动核验）。

use phonemic_core::pairing_code::{
    generate_pairing_code_with_rng, parse_pairing_code, verify_pairing_code, PairingCode,
    PAIRING_CODE_ALPHABET, PAIRING_CODE_LEN,
};
use proptest::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

// ---------- proptest 运行参数 ----------

/// design §9.2 / tasks 3.12：每个 PBT 至少 256 cases。
///
/// 通过 mutate `ProptestConfig::default()` 而非 struct literal 构造，
/// 避免 proptest 升级引入新字段（[`ProptestConfig`] 是 `#[non_exhaustive]`）。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- 共享生成器 ----------

/// 由确定性种子驱动 [`StdRng`] 生成一个 [`PairingCode`]。
///
/// 使用 seed→PairingCode 这种「值生成器」而非把 [`PairingCode`] 直接做成
/// proptest 策略，原因有二：
/// 1. [`PairingCode`] 没有 `Arbitrary` 实现，且字段对外不可见；
/// 2. 失败用例只需打印种子即可完整复现，避免在 shrinking 报告里露出
///    真实配对码（[`PairingCode`] 的 `Debug` 已脱敏，但通过 `as_str()`
///    暴露的明文仍可能被框架打印）。
fn pairing_code_from_seed(seed: u64) -> PairingCode {
    let mut rng = StdRng::seed_from_u64(seed);
    generate_pairing_code_with_rng(&mut rng)
}

/// 任意候选字符串：长度 0..=16，仅含 ASCII 字母 / 数字。
///
/// - 0..=16 同时覆盖空串、短于 8、恰为 8、长于 8 多种长度。
/// - 字符集 `[A-Za-z0-9]` 与设计 §4.3 的 `PAIRING_CODE_ALPHABET`（含小写映射的
///   超集）相交又不完全重合：大写字母 / 数字部分有交集（用于触发等价路径），
///   小写字母 / 易混字符 `0/O/1/I/L` 等用于触发拒绝路径。
fn candidate_string() -> impl Strategy<Value = String> {
    // unwrap 仅在静态正则非法时 panic；这里的正则是常量字面量，编译期可信。
    prop::string::string_regex("[A-Za-z0-9]{0,16}").expect("static regex must compile")
}

// ---------- 属性测试 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 17 主体（等价性）：
    ///
    /// 对任意 `current` 与候选 `candidate`，
    /// `verify_pairing_code(current, candidate)` 当且仅当
    /// `candidate.as_bytes() == current.as_str().as_bytes()` 时为 `true`。
    ///
    /// **Validates: Requirements 7.2**
    #[test]
    fn verify_matches_byte_equality(
        seed in any::<u64>(),
        candidate in candidate_string(),
    ) {
        let current = pairing_code_from_seed(seed);
        let expected = candidate.as_bytes() == current.as_str().as_bytes();
        let actual = verify_pairing_code(&current, &candidate);
        prop_assert_eq!(
            actual,
            expected,
            "verify 与字节相等不一致：seed={seed}, candidate_len={}",
            candidate.len(),
        );
    }
}

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 17 自反性：任意生成的 `PairingCode` 必须接受自身。
    ///
    /// **Validates: Requirements 7.2**
    #[test]
    fn verify_accepts_self(seed in any::<u64>()) {
        let current = pairing_code_from_seed(seed);
        prop_assert!(
            verify_pairing_code(&current, current.as_str()),
            "PairingCode 必须接受自身（seed={seed}）",
        );
    }
}

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 17 长度敏感性：候选长度 ≠ [`PAIRING_CODE_LEN`] 时一律拒绝。
    ///
    /// 用 `prop_assume!` 过滤掉恰好长度为 8 的样本，确保本属性只覆盖
    /// 「长度异常」分支；长度恰为 8 的等价语义已由 `verify_matches_byte_equality`
    /// 全面覆盖。
    ///
    /// **Validates: Requirements 7.2**
    #[test]
    fn verify_rejects_wrong_length(
        seed in any::<u64>(),
        candidate in candidate_string(),
    ) {
        prop_assume!(candidate.len() != PAIRING_CODE_LEN);
        let current = pairing_code_from_seed(seed);
        prop_assert!(
            !verify_pairing_code(&current, &candidate),
            "长度为 {} 的候选不应通过校验（seed={seed}）",
            candidate.len(),
        );
    }
}

// ---------- parse + verify 协同 ----------

/// 任意「合法 8 位 alphabet 字符串」：每一位都从 [`PAIRING_CODE_ALPHABET`]
/// 中均匀挑选，整体构成长度为 [`PAIRING_CODE_LEN`] 的 ASCII 字符串。
///
/// 用于驱动 `parse_pairing_code` 的成功路径（design §4.3 / requirement 7.1）。
fn legal_pairing_input() -> impl Strategy<Value = String> {
    let alphabet_len = PAIRING_CODE_ALPHABET.len();
    prop::collection::vec(0usize..alphabet_len, PAIRING_CODE_LEN)
        .prop_map(|indices| {
            let bytes: Vec<u8> = indices
                .into_iter()
                .map(|i| PAIRING_CODE_ALPHABET[i])
                .collect();
            // alphabet 全部是 ASCII，from_utf8 不可能失败。
            String::from_utf8(bytes).expect("alphabet 子集必为合法 UTF-8")
        })
}

proptest! {
    #![proptest_config(pbt_config())]

    /// `parse_pairing_code` 对合法 8 位 alphabet 输入需要 round-trip，
    /// 且解析结果可与「以同一字符串构造的 PairingCode」互相 verify。
    ///
    /// 这是 design §4.3「客户端构造候选 → 服务端 verify」端到端路径的
    /// 简化版属性化覆盖。
    ///
    /// **Validates: Requirements 7.2**
    #[test]
    fn parse_round_trips_and_verifies(input in legal_pairing_input()) {
        let parsed = parse_pairing_code(&input)
            .expect("合法的 8 位 alphabet 字符串必须解析成功");
        prop_assert_eq!(parsed.as_str(), input.as_str());

        // parsed 自身一定可以通过 verify。
        prop_assert!(
            verify_pairing_code(&parsed, parsed.as_str()),
            "解析得到的 PairingCode 必须与自身字符串等价",
        );

        // 用同一明文再次解析，得到的 PairingCode 应与原 parsed 等价。
        let parsed_again = parse_pairing_code(&input)
            .expect("同一合法输入第二次解析也必须成功");
        prop_assert!(
            verify_pairing_code(&parsed, parsed_again.as_str()),
            "对同一合法明文，两次 parse 得到的 PairingCode 必须 verify 通过",
        );
    }
}
