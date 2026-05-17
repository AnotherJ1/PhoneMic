//! `phonemic-injector` —— 跨平台键盘注入抽象。
//!
//! 任务 1.1 仅产出最小化骨架；`InputInjector` trait 与平台后端
//! （Windows `SendInput`、macOS `CGEvent`、Linux `XTest` / `uinput`）
//! 在任务 7.x 中实现。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.5
