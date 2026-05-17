/**
 * 协议镜像统一出口。
 *
 * 关联 Rust crate：`crates/phonemic-protocol/`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5
 *
 * 通过 `import { ... } from '@/protocol'` 即可拿到所有桌面 ↔ 移动共享
 * 的类型与常量。`PROTOCOL_VERSION` 与 Rust 端 `phonemic_protocol::PROTOCOL_VERSION`
 * 必须保持一致，由 `scripts/gen-ts-types.mjs` 生成的 stamp 文件守护。
 */

export * from './config'
export * from './error'
export * from './error_obj'
export * from './http'
export * from './ws'

/**
 * 协议版本号；写入 `welcome.payload.protocol` 字段。
 *
 * 与 Rust `phonemic_protocol::PROTOCOL_VERSION` 同步演进，stamp 校验脚本
 * 会断言两端字面量一致。
 */
export const PROTOCOL_VERSION = '1'
