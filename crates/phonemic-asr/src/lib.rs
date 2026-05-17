//! `phonemic-asr` —— Server_ASR 引擎适配层（whisper.cpp / 云端兜底）。
//!
//! 任务 1.1 仅产出最小化骨架；`AsrEngine` trait、音频帧类型与
//! whisper.cpp 适配在任务 8.x 中实现。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.8, §4.6

#![forbid(unsafe_code)]
