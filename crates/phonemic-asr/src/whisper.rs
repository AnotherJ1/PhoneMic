//! whisper.cpp 适配器（任务 8.4，feature = "whisper"）。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.8 / §4.6。
//!
//! ## 当前状态
//!
//! 模块在 `whisper` feature 启用时被编入构建图。本仓库 MVP 阶段只声明
//! 适配器**形状**与"模型路径配置 + 初始化错误"路径，把真正的 whisper-rs
//! FFI 集成留给后续任务（与 design §3.8 一致：whisper.cpp 接入 + 模型
//! 文件下载流程在打包阶段一并完成）。
//!
//! 这种"先固化接口形状、再接入 native 后端"的策略让上层（任务 5.11
//! 消息分发器、任务 8.5 看门狗）可以面向稳定 API 编码，而不必在 CI 上
//! 强制装 C 编译器与下载模型。
//!
//! `whisper` feature 关闭时，整个文件不进入构建图；上层默认依赖
//! [`crate::engine::NoopAsr`]（compile-time 开关，design §4.6）。

#![cfg(feature = "whisper")]

use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::engine::{AsrEngine, AsrError, AudioFrame, TranscriptFinal};

/// `WhisperAsr` 配置：模型文件路径来自 `AppConfig.asr.model_path`。
#[derive(Debug, Clone)]
pub struct WhisperConfig {
    pub model_path: PathBuf,
    pub language: Option<String>,
}

/// whisper.cpp 适配器骨架。
///
/// `feed` 把 PCM 帧 push 到内部缓冲；`end` 触发模型推理。MVP 阶段返回
/// [`AsrError::InitError`]，让调用方明确感知"feature 已编入但模型尚未接入"。
pub struct WhisperAsr {
    cfg: WhisperConfig,
    /// 累积音频缓冲（待真实 whisper-rs FFI 注入时复用）。
    buffer: Mutex<Vec<i16>>,
}

impl std::fmt::Debug for WhisperAsr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不暴露 buffer 内容（可能含敏感语音帧）。
        f.debug_struct("WhisperAsr")
            .field("cfg", &self.cfg)
            .field("buffer_samples", &self.buffer.lock().map(|g| g.len()).unwrap_or(0))
            .finish()
    }
}

impl WhisperAsr {
    /// 构造适配器；仅校验模型路径存在，不进行 FFI 初始化（FFI 接入留作后续任务）。
    pub fn new(cfg: WhisperConfig) -> Result<Self, AsrError> {
        if !cfg.model_path.exists() {
            return Err(AsrError::InitError(format!(
                "model file not found: {}",
                cfg.model_path.display()
            )));
        }
        Ok(Self {
            cfg,
            buffer: Mutex::new(Vec::new()),
        })
    }

    /// 借用配置（诊断用）。
    #[must_use]
    pub fn config(&self) -> &WhisperConfig {
        &self.cfg
    }
}

#[async_trait]
impl AsrEngine for WhisperAsr {
    async fn feed(&self, frame: AudioFrame) -> Result<(), AsrError> {
        // PCM16 little-endian → i16 buffer。
        let mut guard = self
            .buffer
            .lock()
            .map_err(|e| AsrError::Backend(format!("buffer poisoned: {e}")))?;
        for chunk in frame.payload.chunks_exact(2) {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]);
            guard.push(s);
        }
        Ok(())
    }

    async fn end(&self) -> Result<TranscriptFinal, AsrError> {
        // 真实 whisper-rs FFI 接入留作后续任务；此处返回 InitError 以
        // 明确告知调用方"feature 已编译但识别管线未就绪"。这与 design
        // §3.8 "模型文件由打包阶段一并提供"保持一致。
        Err(AsrError::InitError(format!(
            "whisper-rs FFI not yet integrated; feature gate is in place, model_path = {}",
            self.cfg.model_path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_path(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("phonemic-whisper-{label}-{pid}-{nanos}-{seq}.bin"))
    }

    #[test]
    fn new_returns_init_error_when_model_missing() {
        let cfg = WhisperConfig {
            model_path: PathBuf::from(r"D:\definitely\does\not\exist\model.bin"),
            language: None,
        };
        let err = WhisperAsr::new(cfg).unwrap_err();
        assert!(matches!(err, AsrError::InitError(_)));
    }

    #[tokio::test]
    async fn end_returns_init_error_pending_ffi_integration() {
        let path = unique_path("model");
        std::fs::File::create(&path)
            .expect("create stub")
            .write_all(b"stub")
            .expect("write stub");
        let cfg = WhisperConfig {
            model_path: path.clone(),
            language: Some("zh".into()),
        };
        let asr = WhisperAsr::new(cfg).expect("constructor accepts existing path");
        let err = asr.end().await.unwrap_err();
        assert!(matches!(err, AsrError::InitError(msg) if msg.contains("not yet integrated")));
        let _ = std::fs::remove_file(&path);
    }
}
