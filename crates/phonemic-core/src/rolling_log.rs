//! 滚动日志缓冲与机密脱敏工具。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.20
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §8.4
//! - 需求来源：`.kiro/specs/phone-mic-voice-input/requirements.md` 9.7
//!
//! 本模块只提供"内存里的滚动日志数据结构 + 机密脱敏摘要"，
//! 与 `tracing` / `tracing-subscriber` 的 `Layer` 集成在任务 13.2
//! 里完成；上层 Subscriber 把已渲染的字符串调用 [`RollingBuffer::push_line`]
//! 即可。两条不变量（Property 32 / Requirement 9.7）：
//!
//! 1. 单条日志字节长度 ≤ [`MAX_LINE_BYTES`]（4 KB）。超长输入按
//!    UTF-8 字符边界安全截断，并在末尾追加 [`TRUNCATED_SUFFIX`]
//!    （`…<truncated>`）。
//! 2. 全部条目的字节总量（含每行的隐式换行符）≤ [`MAX_TOTAL_BYTES`]
//!    （10 MB）。超出时按"最旧条目优先淘汰"的 FIFO 语义滚动覆盖。
//!
//! 同时本模块提供 [`secret_summary`] / [`redact_secret`]：日志中
//! **绝不**写入 Pairing_Code、Session_Token 与文本明文，而是仅记录
//! "字符长度 + SHA-256 摘要前 8 字节"，以满足 design §8.4 的脱敏要求。

use std::collections::VecDeque;

use sha2::{Digest, Sha256};

/// 单条日志最大字节数：4 KB（design §8.4 / requirement 9.7）。
pub const MAX_LINE_BYTES: usize = 4 * 1024;

/// 全部日志条目允许占用的最大字节总量：10 MB（design §8.4 / requirement 9.7）。
///
/// 计入"每行字节数 + 1（换行符）"，与离线落盘后的体感大小一致。
pub const MAX_TOTAL_BYTES: usize = 10 * 1024 * 1024;

/// 截断标记：当原始一行超过 [`MAX_LINE_BYTES`] 时，按 UTF-8 字符
/// 边界向下回退后追加该后缀，方便离线诊断时识别"这是被截断过的行"。
///
/// 字节长度为 14（`"…"` 在 UTF-8 下占 3 字节 + `"<truncated>"` 占 11 字节）。
pub const TRUNCATED_SUFFIX: &str = "…<truncated>";

/// 把单行日志强制裁剪到 [`MAX_LINE_BYTES`] 以内（UTF-8 安全），
/// 必要时附加 [`TRUNCATED_SUFFIX`] 标记。
///
/// 算法：
/// - 若 `line.as_bytes().len() <= MAX_LINE_BYTES` 直接克隆返回；
/// - 否则取 `MAX_LINE_BYTES - TRUNCATED_SUFFIX.len()` 为目标字节位，
///   向下回退到最近的 UTF-8 字符边界（索引 0 永远是边界，循环必终止），
///   然后追加一次 [`TRUNCATED_SUFFIX`]。
///
/// 输出长度恒 ≤ [`MAX_LINE_BYTES`]，内容仍是合法 UTF-8。
#[must_use]
pub fn truncate_line(line: &str) -> String {
    if line.len() <= MAX_LINE_BYTES {
        return line.to_string();
    }

    // TRUNCATED_SUFFIX 长度（14）远小于 MAX_LINE_BYTES（4 KB），下式不会下溢。
    let target = MAX_LINE_BYTES - TRUNCATED_SUFFIX.len();
    // 向下回退至最近的 UTF-8 字符边界。
    let mut idx = target;
    while !line.is_char_boundary(idx) {
        // idx 一定 ≥ 1，因为索引 0 必为字符边界。
        idx -= 1;
    }

    let mut out = String::with_capacity(idx + TRUNCATED_SUFFIX.len());
    out.push_str(&line[..idx]);
    out.push_str(TRUNCATED_SUFFIX);
    out
}

/// 计算输入字符串的 SHA-256 摘要，并以前 8 字节的十六进制（16 个 hex 字符）
/// 形式返回。用于在日志中以**确定性 + 不可逆**的方式标注一段机密文本。
///
/// 8 字节足以在大多数排查场景中区分不同条目，又远不足以反推原文。
#[must_use]
pub fn secret_summary(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    hex::encode(&digest[..8])
}

/// 把任意机密文本（Pairing_Code、Session_Token、待注入文本等）折叠为
/// 可安全写入日志的脱敏字符串：
///
/// ```text
/// len=<字符数> sha8=<前 8 字节十六进制>
/// ```
///
/// - `len`：原始文本的 **Unicode 字符数**（`chars().count()`），便于
///   离线诊断时与协议层 `text.submit` 的字符长度做交叉对账；
/// - `sha8`：[`secret_summary`] 的输出。
///
/// 调用方不应再把原始机密任意拼接进日志行——这是 design §8.4 的红线。
#[must_use]
pub fn redact_secret(s: &str) -> String {
    format!("len={} sha8={}", s.chars().count(), secret_summary(s))
}

/// 在内存中维护的滚动日志缓冲区。
///
/// - `lines`：按写入时间排序的条目队列，队首最旧、队尾最新；
/// - `total_bytes`：所有条目"行字节 + 1（换行符）"之和的缓存值，
///   与 [`MAX_TOTAL_BYTES`] 的判定一致。
///
/// 该结构本身不涉及任何 I/O；调用方决定是否再叠加文件落盘 / `tracing`
/// `Layer` 等副作用。所有公开方法都保持上述两个不变量。
#[derive(Debug, Default)]
pub struct RollingBuffer {
    lines: VecDeque<String>,
    total_bytes: usize,
}

impl RollingBuffer {
    /// 构造一个空的滚动日志缓冲区。
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            total_bytes: 0,
        }
    }

    /// 写入一条日志行。
    ///
    /// 流程：
    /// 1. 先调用 [`truncate_line`] 确保单条 ≤ [`MAX_LINE_BYTES`]；
    /// 2. 计算 `bytes_to_add = truncated.len() + 1`（隐式换行符）；
    /// 3. 当 `total_bytes + bytes_to_add > MAX_TOTAL_BYTES` 时，
    ///    不断 `pop_front` 最旧条目，直到能容纳新行；
    /// 4. 追加新行并更新 `total_bytes`。
    ///
    /// 由于 `MAX_LINE_BYTES + 1 ≪ MAX_TOTAL_BYTES`，"buf 被掏空之后
    /// 还放不下新行"的退化情形在本配置下不可能出现。
    pub fn push_line(&mut self, line: &str) {
        let truncated = truncate_line(line);
        let bytes_to_add = truncated.len() + 1;

        while self.total_bytes + bytes_to_add > MAX_TOTAL_BYTES && !self.lines.is_empty() {
            // 弹出最旧条目并相应回收 (line.len() + 1) 字节。
            if let Some(front) = self.lines.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(front.len() + 1);
            }
        }

        self.total_bytes += bytes_to_add;
        self.lines.push_back(truncated);
    }

    /// 按写入顺序（最旧 → 最新）借用全部条目。
    pub fn lines(&self) -> impl Iterator<Item = &str> + '_ {
        self.lines.iter().map(String::as_str)
    }

    /// 当前缓冲区累计字节数（含每行的隐式换行符）。恒 ≤ [`MAX_TOTAL_BYTES`]。
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// 返回当前所有条目的克隆快照（最旧 → 最新），便于跨线程导出 / 序列化。
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // truncate_line
    // ---------------------------------------------------------------------

    /// 短行原样保留：长度 ≤ MAX_LINE_BYTES 时不应触发截断或后缀。
    #[test]
    fn truncate_line_leaves_short_lines_unchanged() {
        let s = "info: started server on 127.0.0.1:18080";
        assert_eq!(truncate_line(s), s);

        // 边界值：恰好 4 KB 也应原样保留。
        let exact = "x".repeat(MAX_LINE_BYTES);
        assert_eq!(truncate_line(&exact), exact);
        assert!(!truncate_line(&exact).ends_with(TRUNCATED_SUFFIX));
    }

    /// 超长行：截断后必须以 [`TRUNCATED_SUFFIX`] 恰好结尾一次，
    /// 总长度仍 ≤ [`MAX_LINE_BYTES`]。
    #[test]
    fn truncate_line_truncates_with_suffix_once() {
        let huge = "a".repeat(MAX_LINE_BYTES * 3);
        let out = truncate_line(&huge);

        assert!(
            out.len() <= MAX_LINE_BYTES,
            "截断后长度 {} 超过上限 {}",
            out.len(),
            MAX_LINE_BYTES,
        );
        assert!(out.ends_with(TRUNCATED_SUFFIX));
        // 恰好出现一次后缀：从右往左只匹配到一次。
        let occurrences = out.matches(TRUNCATED_SUFFIX).count();
        assert_eq!(occurrences, 1, "TRUNCATED_SUFFIX 应只追加一次");
    }

    /// UTF-8 字符边界安全：在多字节字符附近截断不会 panic，
    /// 也不会切出半个码点；前缀部分的字符必须仍属于原始字符集。
    #[test]
    fn truncate_line_respects_utf8_boundaries() {
        // "你好世界" 每字 3 字节；构造一段稳稳超过 4 KB 的多字节行。
        let unit = "你好世界";
        let times = (MAX_LINE_BYTES / unit.len()) + 16;
        let huge = unit.repeat(times);
        assert!(huge.len() > MAX_LINE_BYTES);

        let out = truncate_line(&huge);

        // String 自身保证内容是合法 UTF-8；额外断言起止点为字符边界。
        assert!(out.is_char_boundary(0));
        assert!(out.is_char_boundary(out.len()));
        assert!(out.ends_with(TRUNCATED_SUFFIX));
        assert!(out.len() <= MAX_LINE_BYTES);

        let prefix = &out[..out.len() - TRUNCATED_SUFFIX.len()];
        assert!(prefix.chars().all(|c| unit.contains(c)));
    }

    // ---------------------------------------------------------------------
    // secret_summary / redact_secret
    // ---------------------------------------------------------------------

    /// `secret_summary` 必须返回 16 个十六进制字符，且确定性可复算。
    ///
    /// SHA-256("hello") =
    ///   2cf24dba 5fb0a30e ...，前 8 字节 → `2cf24dba5fb0a30e`。
    #[test]
    fn secret_summary_returns_16_hex_chars_deterministically() {
        let s1 = secret_summary("hello");
        assert_eq!(s1.len(), 16);
        assert!(s1.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(s1, "2cf24dba5fb0a30e");

        // 相同输入 → 相同输出。
        assert_eq!(secret_summary("hello"), s1);

        // 与 sha2 直接计算交叉验证，避免硬编码失误。
        let expected = hex::encode(&Sha256::digest(b"hello")[..8]);
        assert_eq!(s1, expected);
    }

    /// 不同输入"几乎肯定"产出不同摘要（取一组互异输入做抽查；
    /// 真正的均匀性由 SHA-256 自身保障）。
    #[test]
    fn secret_summary_distinguishes_distinct_inputs() {
        let a = secret_summary("ABCDEFGH");
        let b = secret_summary("ABCDEFGI");
        let c = secret_summary("");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    /// `redact_secret` 输出形如 `len=<N> sha8=<16hex>`，并且不回显原文。
    #[test]
    fn redact_secret_formats_len_and_sha8_without_plaintext() {
        let secret = "ABCDEFGH";
        let out = redact_secret(secret);

        assert!(out.starts_with("len="), "缺少 len= 前缀: {out}");
        assert!(out.contains(" sha8="), "缺少 sha8= 字段: {out}");
        assert!(!out.contains(secret), "脱敏字符串不得回显明文");

        // 字符数（非字节数）：英文 8 字符。
        assert!(out.starts_with("len=8 "));

        // 多字节：6 字节但只有 2 字符。
        let cn = redact_secret("你好");
        assert!(cn.starts_with("len=2 "), "应使用 chars().count() 而非字节数: {cn}");
    }

    // ---------------------------------------------------------------------
    // RollingBuffer
    // ---------------------------------------------------------------------

    /// 写入顺序保留：最旧在前、最新在后；`snapshot` 与 `lines()` 一致。
    #[test]
    fn rolling_buffer_preserves_insertion_order() {
        let mut buf = RollingBuffer::new();
        buf.push_line("first");
        buf.push_line("second");
        buf.push_line("third");

        let collected: Vec<&str> = buf.lines().collect();
        assert_eq!(collected, vec!["first", "second", "third"]);
        assert_eq!(buf.snapshot(), vec!["first", "second", "third"]);
    }

    /// 持续灌入超过 10 MB 时，`total_bytes()` 始终 ≤ MAX_TOTAL_BYTES，
    /// 且最旧条目先被淘汰、最新条目仍可读。
    #[test]
    fn rolling_buffer_evicts_oldest_and_caps_total_bytes() {
        let mut buf = RollingBuffer::new();

        // 单条 4 KB（含末尾换行 +1 字节，相当于占用 MAX_LINE_BYTES+1）。
        // 灌入 needed 条以触发若干次淘汰。
        let make = |idx: usize| {
            let header = format!("[{idx:08}] ");
            let mut s = header.clone();
            // 留出 header 后填充至 MAX_LINE_BYTES 字节。
            s.push_str(&"x".repeat(MAX_LINE_BYTES - header.len()));
            debug_assert_eq!(s.len(), MAX_LINE_BYTES);
            s
        };

        // 容量 ≈ 10MB / 4KB ≈ 2560 条；多写 32 条以明确触发淘汰路径。
        let needed = MAX_TOTAL_BYTES / MAX_LINE_BYTES + 32;
        for i in 0..needed {
            buf.push_line(&make(i));
        }

        assert!(
            buf.total_bytes() <= MAX_TOTAL_BYTES,
            "total_bytes={} 超过 MAX_TOTAL_BYTES={}",
            buf.total_bytes(),
            MAX_TOTAL_BYTES,
        );
        // 至少发生过一次淘汰。
        let remaining = buf.lines().count();
        assert!(remaining < needed, "未触发淘汰：remaining={remaining} needed={needed}");

        // 最新条目（队尾）必为最后一次写入的内容；
        // 最旧条目（队首）的序号必 > 0，因为前面若干条已被淘汰。
        let snap = buf.snapshot();
        let last = snap.last().expect("buffer 不应为空");
        assert!(last.starts_with(&format!("[{:08}]", needed - 1)));
        let first = snap.first().expect("buffer 不应为空");
        assert!(
            !first.starts_with("[00000000]"),
            "最旧条目未被淘汰：{}",
            &first[..18.min(first.len())],
        );
    }

    /// 单条远超 [`MAX_LINE_BYTES`] 的输入：先被截断，再正常入队，
    /// 占用绝不会超过 [`MAX_LINE_BYTES`] + 1 字节。
    #[test]
    fn rolling_buffer_truncates_oversized_line_and_still_fits() {
        let mut buf = RollingBuffer::new();
        let huge = "a".repeat(MAX_LINE_BYTES * 5);

        buf.push_line(&huge);

        let stored: Vec<&str> = buf.lines().collect();
        assert_eq!(stored.len(), 1);
        let only = stored[0];
        assert!(only.len() <= MAX_LINE_BYTES);
        assert!(only.ends_with(TRUNCATED_SUFFIX));
        assert_eq!(buf.total_bytes(), only.len() + 1);
        assert!(buf.total_bytes() <= MAX_TOTAL_BYTES);
    }
}
