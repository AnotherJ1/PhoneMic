//! Pairing_Code 生成器与基础类型定义。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.9
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.3
//! - 需求来源：`.kiro/specs/phone-mic-voice-input/requirements.md` 7.1
//!
//! 本模块当前仅实现 [`generate_pairing_code`] 与 [`PairingCode`] 骨架；
//! 校验逻辑 `verify_pairing_code`（任务 3.11）将在同一模块内追加，
//! 并使用 `subtle::ConstantTimeEq` 以提供恒定时间比较，避免时序侧信道。

use core::fmt;

use rand::Rng;
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;

/// Pairing_Code 长度，固定为 8 位（design §4.3 / requirement 7.1）。
pub const PAIRING_CODE_LEN: usize = 8;

/// Pairing_Code 字符集：A–Z 去除易混 `O`/`I`/`L`，0–9 去除易混 `0`/`1`，
/// 合计 31 个字符（design §4.3）。
///
/// 字节顺序对外稳定，可作为索引表使用。
pub const PAIRING_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// 以固定长度字节数组承载的 Pairing_Code。
///
/// 由构造路径保证内部字节一定取自 [`PAIRING_CODE_ALPHABET`]，
/// 因此可安全地视为 ASCII / UTF-8 字符串。
///
/// 出于安全考虑，[`fmt::Debug`] 输出经过脱敏处理，仅显示
/// `PairingCode(****)`；如需获取原始字符序列，请显式调用
/// [`PairingCode::as_str`] 或 [`PairingCode::into_string`]。
///
/// 当前的 [`PartialEq`] 由 `derive` 提供（按字节比较），
/// 任务 3.11 将引入基于 `subtle::ConstantTimeEq` 的恒定时间比较函数
/// `verify_pairing_code`，用于实际的配对校验路径。
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode([u8; PAIRING_CODE_LEN]);

impl PairingCode {
    /// 借用底层字节序列对应的字符串切片。
    #[must_use]
    pub fn as_str(&self) -> &str {
        // 构造时已校验所有字节属于 PAIRING_CODE_ALPHABET（纯 ASCII），
        // 因此 from_utf8 不可能失败；这里使用安全 API 以避免任何 unsafe。
        core::str::from_utf8(&self.0).expect("PairingCode bytes must be valid ASCII")
    }

    /// 消费自身并产出新的堆分配 [`String`]。
    #[must_use]
    pub fn into_string(self) -> String {
        self.as_str().to_owned()
    }

    /// 从已校验的字节数组直接构造，仅供模块内 / 任务 3.11 校验路径使用。
    #[allow(dead_code)]
    pub(crate) fn from_validated_bytes(bytes: [u8; PAIRING_CODE_LEN]) -> Self {
        debug_assert!(
            bytes.iter().all(|b| PAIRING_CODE_ALPHABET.contains(b)),
            "PairingCode 字节必须全部来自 PAIRING_CODE_ALPHABET",
        );
        Self(bytes)
    }

    /// 借用底层定长字节数组，仅在 crate 内部使用。
    ///
    /// 仅暴露给同 crate 的恒定时间比较路径（任务 3.11 `verify_pairing_code`），
    /// 故采用 `pub(crate)` 可见性，避免外部代码直接获取原始字节
    /// 而绕过脱敏的 [`fmt::Debug`] 输出策略。如需面向外部的字符串表示，
    /// 请继续使用 [`PairingCode::as_str`] / [`PairingCode::into_string`]。
    #[must_use]
    pub(crate) fn as_bytes(&self) -> &[u8; PAIRING_CODE_LEN] {
        &self.0
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 不在 Debug 输出中暴露真实配对码，避免误入日志 / 错误链。
        f.write_str("PairingCode(****)")
    }
}

impl fmt::Display for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 使用操作系统加密随机源 [`OsRng`] 生成一个 8 位 Pairing_Code。
///
/// - 字符集：[`PAIRING_CODE_ALPHABET`]（31 字符，已剔除 `0/O/1/I/L`）。
/// - 随机源：`rand::rngs::OsRng`，由 `getrandom` 提供，满足
///   requirement 7.1 关于"加密强度随机源"的要求。
#[must_use]
pub fn generate_pairing_code() -> PairingCode {
    let mut rng = OsRng;
    generate_pairing_code_with_rng(&mut rng)
}

/// 注入式版本，便于单元 / 属性测试以确定性 RNG 复现失败用例。
///
/// 内部使用 [`Rng::gen_range`]（拒绝采样实现），从而消除
/// 直接 `byte % 31` 会引入的模偏置（256 % 31 != 0）。
pub fn generate_pairing_code_with_rng(rng: &mut impl rand::RngCore) -> PairingCode {
    let mut buf = [0u8; PAIRING_CODE_LEN];
    let alphabet_len = PAIRING_CODE_ALPHABET.len();
    for slot in &mut buf {
        let idx = rng.gen_range(0..alphabet_len);
        *slot = PAIRING_CODE_ALPHABET[idx];
    }
    PairingCode(buf)
}

/// 以恒定时间比较 `candidate`（用户输入的 8 位字符串）与当前期望的
/// `current` Pairing_Code，避免按字节早退式比较泄露匹配前缀长度等信息。
///
/// - 设计来源：`design.md` §4.3
/// - 需求来源：`requirements.md` 7.2
///
/// 实现要点：
/// 1. 始终把 `candidate` 拷贝/填充进固定 [`PAIRING_CODE_LEN`] 字节缓冲区，
///    多余字节用 `0x00` 哨兵填充——该哨兵不属于
///    [`PAIRING_CODE_ALPHABET`]，因此无论候选长度如何，都不会与合法
///    Pairing_Code 字节意外匹配。
/// 2. 使用 `subtle::ConstantTimeEq::ct_eq` 进行字节级比较；该 API
///    内部不会基于差异提前返回，从而消除时序侧信道。
/// 3. 长度判断同样走恒定时间路径（对 `u64` 做 `ct_eq`），并以位与
///    `&` 合并到最终结果，避免 `&&` 的短路求值带来分支信息泄露。
///
/// 仅当 `candidate.len() == PAIRING_CODE_LEN` 且与 `current` 的全部
/// 8 字节按位相等时返回 `true`。
#[must_use]
pub fn verify_pairing_code(current: &PairingCode, candidate: &str) -> bool {
    let candidate_bytes = candidate.as_bytes();
    // 0x00 不属于 PAIRING_CODE_ALPHABET，可作为安全的填充哨兵。
    let mut buf = [0u8; PAIRING_CODE_LEN];
    let copy_len = candidate_bytes.len().min(PAIRING_CODE_LEN);
    buf[..copy_len].copy_from_slice(&candidate_bytes[..copy_len]);

    // 字节级恒定时间比较：即使首字节不同，也会扫完全部 8 字节。
    let bytes_eq: bool = buf.ct_eq(current.as_bytes()).into();
    // 长度比较亦走恒定时间路径，避免 `&&` 短路造成的分支差异。
    let len_eq: bool =
        (candidate_bytes.len() as u64).ct_eq(&(PAIRING_CODE_LEN as u64)).into();

    // 用按位与而非逻辑与，进一步消除短路语义。
    bytes_eq & len_eq
}

/// 将用户侧字符串解析为 [`PairingCode`]，仅在长度与字符集均合法时返回 `Some`。
///
/// - 设计来源：`design.md` §4.3
/// - 需求来源：`requirements.md` 7.1 / 7.2
///
/// **本函数不是恒定时间的**：它面向"客户端构造候选值"路径，被比较的
/// 秘密（即当前活跃 Pairing_Code）出现在 [`verify_pairing_code`] 中，
/// 而不在这里——因此对候选值做长度 / 字符集早退检查不会泄露秘密信息。
///
/// 用于把上层 HTTP / WS 收到的 8 位输入串先转成强类型 [`PairingCode`]，
/// 再交给 [`verify_pairing_code`] 做恒定时间比较；如果输入长度错误或
/// 含有 [`PAIRING_CODE_ALPHABET`] 之外的字符（包括小写 / 易混字符），
/// 则直接返回 `None`，由调用方按"配对码错误"路径计入失败次数。
#[must_use]
pub fn parse_pairing_code(input: &str) -> Option<PairingCode> {
    let bytes = input.as_bytes();
    if bytes.len() != PAIRING_CODE_LEN {
        return None;
    }
    if !bytes.iter().all(|b| PAIRING_CODE_ALPHABET.contains(b)) {
        return None;
    }
    let mut buf = [0u8; PAIRING_CODE_LEN];
    buf.copy_from_slice(bytes);
    Some(PairingCode::from_validated_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// requirement 7.1 / design §4.3：生成的 Pairing_Code 必须恰好 8 位。
    #[test]
    fn generated_code_has_fixed_length() {
        let code = generate_pairing_code();
        assert_eq!(code.as_str().len(), PAIRING_CODE_LEN);
        assert_eq!(code.into_string().len(), PAIRING_CODE_LEN);
    }

    /// 字符集校验：每一位字符都必须落在 [`PAIRING_CODE_ALPHABET`] 内。
    #[test]
    fn every_char_is_in_alphabet() {
        for _ in 0..256 {
            let code = generate_pairing_code();
            for &b in code.as_str().as_bytes() {
                assert!(
                    PAIRING_CODE_ALPHABET.contains(&b),
                    "字符 {:?} 不在配对码字符集内",
                    b as char,
                );
            }
        }
    }

    /// requirement 7.1：易混字符 `0/O/1/I/L` 必须永远不会出现。
    #[test]
    fn never_contains_confusable_chars() {
        const CONFUSABLES: &[u8] = b"0O1IL";
        for _ in 0..256 {
            let code = generate_pairing_code();
            for &b in code.as_str().as_bytes() {
                assert!(
                    !CONFUSABLES.contains(&b),
                    "配对码中出现了易混字符 {:?}",
                    b as char,
                );
            }
        }
    }

    /// 字符集本身的契约：长度必须为 31，且不含易混字符。
    #[test]
    fn alphabet_contract_is_stable() {
        assert_eq!(PAIRING_CODE_ALPHABET.len(), 31);
        for confusable in b"0O1IL" {
            assert!(
                !PAIRING_CODE_ALPHABET.contains(confusable),
                "字符集仍包含易混字符 {:?}",
                *confusable as char,
            );
        }
        // 不允许重复字符，否则会引入分布偏置。
        let mut sorted: Vec<u8> = PAIRING_CODE_ALPHABET.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), PAIRING_CODE_ALPHABET.len());
    }

    /// 轻量唯一性 Sanity：1000 次生成应几乎无碰撞。
    /// 真正的均匀性 / 熵属性测试由任务 3.10 用 proptest 单独覆盖。
    #[test]
    fn sample_is_mostly_unique() {
        use std::collections::HashSet;

        const SAMPLES: usize = 1_000;
        // 31^8 ≈ 8.5e11，1000 样本期望碰撞数远小于 1；保留 5 作为宽松上界。
        const MAX_DUPLICATES: usize = 5;

        let mut seen = HashSet::with_capacity(SAMPLES);
        let mut duplicates = 0usize;
        for _ in 0..SAMPLES {
            let code = generate_pairing_code().into_string();
            if !seen.insert(code) {
                duplicates += 1;
            }
        }
        assert!(
            duplicates <= MAX_DUPLICATES,
            "1000 次生成出现了 {duplicates} 次重复，超出宽松阈值 {MAX_DUPLICATES}",
        );
    }

    /// Debug 输出必须脱敏，避免误入日志 / panic 链。
    #[test]
    fn debug_output_is_redacted() {
        let code = generate_pairing_code();
        let debug = format!("{code:?}");
        assert_eq!(debug, "PairingCode(****)");
        assert!(!debug.contains(code.as_str()));
    }

    /// Display 输出必须等同于 `as_str`，用于面向用户的 UI 渲染。
    #[test]
    fn display_matches_as_str() {
        let code = generate_pairing_code();
        assert_eq!(format!("{code}"), code.as_str());
    }
}

#[cfg(test)]
mod verify_tests {
    //! `verify_pairing_code` 专属测试：
    //!
    //! - requirement 7.2：使用恒定时间比较抵御时序攻击；
    //! - design §4.3：长度 / 字节均不一致时返回 `false`，且不通过
    //!   早退分支泄露匹配进度。

    use super::*;

    /// 取一个固定的 8 位合法 Pairing_Code 用例，避免依赖随机性导致
    /// 偶发不稳定。所有字符均取自 [`PAIRING_CODE_ALPHABET`]。
    fn sample_pairing_code() -> PairingCode {
        PairingCode::from_validated_bytes(*b"ABCDJKMN")
    }

    #[test]
    fn accepts_exact_match() {
        let code = sample_pairing_code();
        assert!(
            verify_pairing_code(&code, code.as_str()),
            "完全相同的候选必须被接受",
        );
    }

    #[test]
    fn rejects_single_byte_diff() {
        let code = sample_pairing_code();
        let original = code.as_str().as_bytes();

        for idx in 0..PAIRING_CODE_LEN {
            // 替换为 alphabet 中的另一个合法字符，保证候选仍是 8 位合法字符串，
            // 但必有 1 字节与原码不同。
            let mut bytes = original.to_vec();
            let replacement = PAIRING_CODE_ALPHABET
                .iter()
                .copied()
                .find(|b| *b != bytes[idx])
                .expect("alphabet 至少有 2 个不同字符");
            bytes[idx] = replacement;
            let candidate =
                core::str::from_utf8(&bytes).expect("alphabet 子集必为合法 UTF-8");
            assert!(
                !verify_pairing_code(&code, candidate),
                "在第 {idx} 位变更后的候选不应通过校验：{candidate}",
            );
        }
    }

    #[test]
    fn rejects_wrong_length_candidates() {
        let code = sample_pairing_code();
        // 覆盖 0 / 短 / 长 / 远长于 8 字节几种情况。
        let cases = ["", "A", "ABCDJKM", "ABCDJKMNX", "ABCDJKMNABCDJKMN"];
        for candidate in cases {
            assert_eq!(
                candidate.len() != PAIRING_CODE_LEN,
                true,
                "测试用例自身必须长度异常"
            );
            assert!(
                !verify_pairing_code(&code, candidate),
                "长度为 {} 的候选 {candidate:?} 不应通过校验",
                candidate.len(),
            );
        }
    }

    #[test]
    fn rejects_non_alphabet_same_length_candidate() {
        let code = sample_pairing_code();
        // 长度恰好 8 但全部由非 alphabet 字符构成（含易混 `0/O/1/I/L`）。
        let candidate = "0OIL01OL";
        assert_eq!(candidate.len(), PAIRING_CODE_LEN);
        assert!(
            !verify_pairing_code(&code, candidate),
            "非字符集候选不应通过校验：{candidate}",
        );
    }

    /// 小写字符不属于 [`PAIRING_CODE_ALPHABET`]，校验路径必须拒绝。
    #[test]
    fn rejects_lowercase_candidate() {
        let code = sample_pairing_code();
        let candidate = code.as_str().to_ascii_lowercase();
        assert_eq!(candidate.len(), PAIRING_CODE_LEN);
        assert!(
            !verify_pairing_code(&code, &candidate),
            "小写形式不应通过校验：{candidate}",
        );
    }

    /// `parse_pairing_code` 对合法输入需要 round-trip 回到原 [`PairingCode`]。
    #[test]
    fn parse_pairing_code_round_trips_valid_input() {
        let code = sample_pairing_code();
        let parsed = parse_pairing_code(code.as_str())
            .expect("合法的 8 位 alphabet 字符串必须解析成功");
        assert_eq!(parsed.as_str(), code.as_str());
        // 同时通过 verify 路径，确认两个 API 协同工作。
        assert!(verify_pairing_code(&code, parsed.as_str()));
    }

    /// 小写输入虽然字符在视觉上"对应"，但不在字符集内，必须返回 `None`。
    #[test]
    fn parse_pairing_code_rejects_lowercase() {
        let code = sample_pairing_code();
        let lower = code.as_str().to_ascii_lowercase();
        assert!(
            parse_pairing_code(&lower).is_none(),
            "小写输入不应被解析为 PairingCode：{lower}",
        );
    }

    /// 长度异常（含空串 / 短 / 长）一律返回 `None`，且不发生 panic。
    #[test]
    fn parse_pairing_code_rejects_wrong_length() {
        for candidate in ["", "ABC", "ABCDJKM", "ABCDJKMNX", "ABCDJKMNABCDJKMN"] {
            assert!(
                parse_pairing_code(candidate).is_none(),
                "长度为 {} 的候选 {candidate:?} 不应解析成功",
                candidate.len(),
            );
        }
    }

    /// 长度合法但含非字符集字节（如 `0/O/1/I/L`）也必须返回 `None`。
    #[test]
    fn parse_pairing_code_rejects_illegal_chars() {
        let candidate = "0OIL01OL";
        assert_eq!(candidate.len(), PAIRING_CODE_LEN);
        assert!(
            parse_pairing_code(candidate).is_none(),
            "含易混 / 非字符集字符的候选不应解析成功：{candidate}",
        );
    }

    /// 粗粒度时序回归（loose smoke test）：完全匹配与零字节填充候选
    /// （首字节即不同）耗时不应出现量级差异。CI 环境抖动较大，故标记
    /// `#[ignore]`，仅供本地手动核验。requirement 7.2 / design §4.3。
    #[test]
    #[ignore = "timing smoke test, run manually with `cargo test -- --ignored`"]
    fn timing_is_independent_of_match_position() {
        use std::hint::black_box;
        use std::time::Instant;

        const ITER: u32 = 10_000;
        let code = sample_pairing_code();
        let exact = code.as_str().to_owned();
        // 长度相同但全部字节不同（0x00 在 alphabet 之外，故首字节即不匹配）。
        let mismatch = String::from_utf8(vec![b'2'; PAIRING_CODE_LEN])
            .expect("ASCII 字节必为合法 UTF-8");

        // 预热，减小首次分支预测 / 缓存填充的偏差。
        for _ in 0..ITER {
            let _ = black_box(verify_pairing_code(&code, &exact));
            let _ = black_box(verify_pairing_code(&code, &mismatch));
        }

        let t0 = Instant::now();
        for _ in 0..ITER {
            let _ = black_box(verify_pairing_code(&code, &exact));
        }
        let exact_dur = t0.elapsed();

        let t1 = Instant::now();
        for _ in 0..ITER {
            let _ = black_box(verify_pairing_code(&code, &mismatch));
        }
        let mismatch_dur = t1.elapsed();

        let (lo, hi) = if exact_dur <= mismatch_dur {
            (exact_dur, mismatch_dur)
        } else {
            (mismatch_dur, exact_dur)
        };
        // 2× 是非常宽松的阈值，仅用于捕捉「明显早退」级别的回归。
        assert!(
            hi.as_nanos() <= lo.as_nanos().saturating_mul(2),
            "时序差异过大：exact={exact_dur:?}, mismatch={mismatch_dur:?}",
        );
    }
}
