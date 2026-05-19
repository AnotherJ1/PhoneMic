//! HTTPS 自签名证书生成与持久化（任务 5.10）。
//!
//! 设计来源：design.md §3.3 / §4.2。
//!
//! 首次启用 HTTPS 时使用 [`rcgen`] 生成自签名证书，将 PEM 编码后的
//! 证书与私钥写入用户配置目录；二次启动时直接读取。
//!
//! 注意：本模块只负责证书的生成 / 加载；axum 与 rustls 的拼装放在
//! [`crate::web::server`] 中。

use std::fs;
use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, KeyPair};

/// 生成或加载证书的结果：PEM 字符串。
#[derive(Debug, Clone)]
pub struct SelfSignedCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

/// 从 `dir` 中读取 `phonemic-cert.pem` / `phonemic-key.pem`；不存在则
/// 生成新证书并写入。`subject_alt_names` 由调用方提供，通常为
/// `["localhost", <LAN IPs>]`。
///
/// # Errors
///
/// 任何 I/O 或 rcgen 失败都会作为 `std::io::Error` 返回。
pub fn ensure_cert(
    dir: impl AsRef<Path>,
    subject_alt_names: Vec<String>,
) -> std::io::Result<SelfSignedCert> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    let cert_path = dir.join("phonemic-cert.pem");
    let key_path = dir.join("phonemic-key.pem");

    if cert_path.exists() && key_path.exists() {
        let cert_pem = fs::read_to_string(&cert_path)?;
        let key_pem = fs::read_to_string(&key_path)?;
        return Ok(SelfSignedCert {
            cert_pem,
            key_pem,
            cert_path,
            key_path,
        });
    }

    // 生成新证书。
    let params = CertificateParams::new(subject_alt_names)
        .map_err(|e| std::io::Error::other(format!("rcgen params: {e}")))?;
    let key_pair = KeyPair::generate()
        .map_err(|e| std::io::Error::other(format!("rcgen keypair: {e}")))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| std::io::Error::other(format!("rcgen self_signed: {e}")))?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    fs::write(&cert_path, &cert_pem)?;
    fs::write(&key_path, &key_pem)?;

    Ok(SelfSignedCert {
        cert_pem,
        key_pem,
        cert_path,
        key_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("phonemic-tls-{label}-{pid}-{nanos}-{seq}"))
    }

    #[test]
    fn ensure_cert_generates_and_round_trips() {
        let dir = unique_dir("gen");
        let cert = ensure_cert(&dir, vec!["localhost".to_owned()]).expect("generate");
        assert!(cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert.key_pem.contains("PRIVATE KEY"));

        let cert2 = ensure_cert(&dir, vec!["localhost".to_owned()]).expect("reload");
        assert_eq!(cert.cert_pem, cert2.cert_pem);
        assert_eq!(cert.key_pem, cert2.key_pem);

        // 清理测试产物。
        let _ = std::fs::remove_dir_all(&dir);
    }
}
