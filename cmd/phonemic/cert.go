// 自签 TLS 证书：纯 Go 生成，无需 openssl。
//
// 为什么必须 HTTPS：
//
//	Web Speech API（webkitSpeechRecognition）与 getUserMedia 一样，
//	只在"安全上下文"下可用 —— 即 https:// 或 localhost。
//	手机通过 http://<局域网IP> 访问时，Chrome 会直接拒绝麦克风，
//	报 SpeechRecognition error: not-allowed。所以局域网场景必须自签 HTTPS。
//
// 证书设计：
//   - SAN 同时包含检测到的 LAN IP、127.0.0.1、localhost，
//     这样无论手机用 IP 还是本机用 localhost 访问，证书都匹配。
//   - 有效期 10 年，避免频繁过期。
//   - 生成后缓存到用户配置目录；若缓存证书的 SAN 已覆盖当前 LAN IP 且未过期，
//     则复用，避免每次启动都变证书、手机每次都要重新点"继续访问"。
package main

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"fmt"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"time"
)

// certCacheDir 返回证书缓存目录（用户配置目录下的 phonemic 子目录）
func certCacheDir() string {
	dir, err := os.UserConfigDir()
	if err != nil {
		// 退化到临时目录
		dir = os.TempDir()
	}
	return filepath.Join(dir, "phonemic")
}

// loadOrCreateCert 加载缓存证书；若不存在 / 已过期 / 不覆盖当前 LAN IP，则重新生成并缓存。
//
// 参数 lanIP 是当前检测到的局域网 IP，必须在证书 SAN 内，否则 Chrome 仍会拒绝。
func loadOrCreateCert(lanIP string) (tls.Certificate, error) {
	dir := certCacheDir()
	certPath := filepath.Join(dir, "cert.pem")
	keyPath := filepath.Join(dir, "key.pem")

	// 尝试复用缓存证书
	if cert, ok := tryReuseCert(certPath, keyPath, lanIP); ok {
		return cert, nil
	}

	// 重新生成
	certPEM, keyPEM, err := generateSelfSigned(lanIP)
	if err != nil {
		return tls.Certificate{}, err
	}
	// 写缓存（失败不致命，仅影响下次复用）
	if mkErr := os.MkdirAll(dir, 0o700); mkErr == nil {
		_ = os.WriteFile(certPath, certPEM, 0o600)
		_ = os.WriteFile(keyPath, keyPEM, 0o600)
	}
	return tls.X509KeyPair(certPEM, keyPEM)
}

// tryReuseCert 判断缓存证书是否可复用：能解析、未过期、SAN 覆盖当前 lanIP
func tryReuseCert(certPath, keyPath, lanIP string) (tls.Certificate, bool) {
	certPEM, err := os.ReadFile(certPath)
	if err != nil {
		return tls.Certificate{}, false
	}
	keyPEM, err := os.ReadFile(keyPath)
	if err != nil {
		return tls.Certificate{}, false
	}
	cert, err := tls.X509KeyPair(certPEM, keyPEM)
	if err != nil {
		return tls.Certificate{}, false
	}
	leaf, err := x509.ParseCertificate(cert.Certificate[0])
	if err != nil {
		return tls.Certificate{}, false
	}
	// 过期检查：留 24h 余量
	if time.Now().After(leaf.NotAfter.Add(-24 * time.Hour)) {
		return tls.Certificate{}, false
	}
	// SAN 必须覆盖当前 LAN IP
	wantIP := net.ParseIP(lanIP)
	covered := false
	for _, ip := range leaf.IPAddresses {
		if wantIP != nil && ip.Equal(wantIP) {
			covered = true
			break
		}
	}
	if !covered {
		return tls.Certificate{}, false
	}
	cert.Leaf = leaf
	return cert, true
}

// generateSelfSigned 生成自签 ECDSA 证书，SAN 含 lanIP / 127.0.0.1 / localhost
func generateSelfSigned(lanIP string) (certPEM, keyPEM []byte, err error) {
	priv, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return nil, nil, fmt.Errorf("generate key: %w", err)
	}

	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 128))
	if err != nil {
		return nil, nil, fmt.Errorf("serial: %w", err)
	}

	ips := []net.IP{net.IPv4(127, 0, 0, 1), net.IPv6loopback}
	if ip := net.ParseIP(lanIP); ip != nil {
		ips = append(ips, ip)
	}

	tmpl := x509.Certificate{
		SerialNumber: serial,
		Subject:      pkix.Name{CommonName: "PhoneMic Local Cert", Organization: []string{"PhoneMic"}},
		NotBefore:    time.Now().Add(-1 * time.Hour),
		NotAfter:     time.Now().AddDate(10, 0, 0), // 10 年
		KeyUsage:     x509.KeyUsageDigitalSignature | x509.KeyUsageKeyEncipherment,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		IPAddresses:  ips,
		DNSNames:     []string{"localhost"},
	}

	der, err := x509.CreateCertificate(rand.Reader, &tmpl, &tmpl, &priv.PublicKey, priv)
	if err != nil {
		return nil, nil, fmt.Errorf("create cert: %w", err)
	}

	certPEM = pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: der})

	keyDER, err := x509.MarshalECPrivateKey(priv)
	if err != nil {
		return nil, nil, fmt.Errorf("marshal key: %w", err)
	}
	keyPEM = pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: keyDER})
	return certPEM, keyPEM, nil
}
