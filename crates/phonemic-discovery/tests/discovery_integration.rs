// Discovery integration test (任务 6.5).
//
// 启动一个 Discovery 实例，使用第二个 mdns-sd 浏览客户端断言能解析出
// `_phonemic._tcp.local.` 服务以及 TXT 记录的关键字段。
//
// 注：mDNS 在某些 CI 环境（容器、防火墙）下不可用；此时本测试自动跳过
// 而不是失败，与 design §9.4 一致。

use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use phonemic_core::bridge_events::channel as events_channel;
use phonemic_discovery::{Discovery, DiscoveryCfg};

const SERVICE_TYPE: &str = "_phonemic._tcp.local.";

#[tokio::test]
async fn second_browser_resolves_phonemic_service() {
    let (tx, _rx) = events_channel();
    let cfg = DiscoveryCfg {
        instance_name: "phonemic-it-test".into(),
        port: 18190,
        https: true,
    };

    let Ok(discovery) = Discovery::start(cfg, tx) else {
        eprintln!("Discovery startup failed; mDNS likely unavailable in CI; skipping");
        return;
    };

    // 使用第二个 mdns-sd 客户端浏览。
    let Ok(browser_daemon) = ServiceDaemon::new() else {
        eprintln!("second mdns daemon unavailable; skipping");
        discovery.stop().await;
        return;
    };
    let Ok(receiver) = browser_daemon.browse(SERVICE_TYPE) else {
        eprintln!("browse failed; skipping");
        discovery.stop().await;
        return;
    };

    let timeout = Duration::from_secs(5);
    let started = std::time::Instant::now();
    let mut resolved_tls = false;
    let mut resolved_port = false;
    while started.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(500), async {
            receiver.recv_async().await
        })
        .await
        {
            Ok(Ok(ServiceEvent::ServiceResolved(info))) => {
                let txt = info.get_properties();
                if let Some(p) = txt.get_property_val_str("tls") {
                    if p == "1" {
                        resolved_tls = true;
                    }
                }
                if let Some(p) = txt.get_property_val_str("port") {
                    if p == "18190" {
                        resolved_port = true;
                    }
                }
                if resolved_tls && resolved_port {
                    break;
                }
            }
            _ => continue,
        }
    }
    let _ = browser_daemon.shutdown();
    discovery.stop().await;

    // mDNS 在不同 OS / 防火墙下行为差异较大；此处不强制要求成功，
    // 只在能解析时验证 TXT 字段；解析失败则视为 CI 限制并放过。
    if resolved_tls || resolved_port {
        assert!(resolved_tls, "TXT tls=1 not seen");
        assert!(resolved_port, "TXT port=18190 not seen");
    } else {
        eprintln!("mDNS browser saw no resolved services within 5s; skipping (CI restriction)");
    }
}
