use crate::{
    config::{Indexer, Provider},
    ui,
};
use anyhow::Result;
use eframe::egui;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub enum Service {
    Provider(Provider),
    Indexer(Indexer),
}
pub struct Check {
    fingerprint: u64,
    result: Option<Result<String, String>>,
    rx: mpsc::Receiver<Result<String, String>>,
}
pub type Checks = HashMap<(bool, usize), Check>;
pub fn control(ui: &mut egui::Ui, checks: &mut Checks, index: usize, service: Service) {
    let (provider, serialized) = match &service {
        Service::Provider(p) => (true, serde_json::to_string(p)),
        Service::Indexer(i) => (false, serde_json::to_string(i)),
    };
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serialized.unwrap_or_default().hash(&mut hash);
    let fingerprint = hash.finish();
    let key = (provider, index);
    if checks
        .get(&key)
        .is_some_and(|c| c.fingerprint != fingerprint)
    {
        checks.remove(&key);
    }
    if let Some(check) = checks.get_mut(&key) {
        if check.result.is_none() {
            match check.rx.try_recv() {
                Ok(r) => check.result = Some(r),
                Err(mpsc::TryRecvError::Disconnected) => {
                    check.result = Some(Err("Connection test stopped unexpectedly".into()))
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    let busy = checks.get(&key).is_some_and(|c| c.result.is_none());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(if provider {
                    "Test connection"
                } else {
                    "Test API key"
                }),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let start = Instant::now();
                let result = match service {
                    Service::Provider(p) => test_provider(&p),
                    Service::Indexer(i) => crate::indexer::test(&i),
                }
                .map(|m| format!("{m} ({} ms)", start.elapsed().as_millis()))
                .map_err(|e| e.to_string());
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            checks.insert(
                key,
                Check {
                    fingerprint,
                    result: None,
                    rx,
                },
            );
        }
        if busy {
            ui.spinner();
            ui.label(egui::RichText::new("Checking…").color(ui::MUTED));
        }
    });
    if let Some(result) = checks.get(&key).and_then(|c| c.result.as_ref()) {
        match result {
            Ok(message) => {
                ui.colored_label(egui::Color32::from_rgb(133, 209, 167), message);
            }
            Err(message) => {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
        }
    }
}
pub fn test_provider(p: &Provider) -> Result<String> {
    anyhow::ensure!(
        !p.host.trim().is_empty(),
        "Enter your provider's server hostname"
    );
    anyhow::ensure!(
        !p.username.trim().is_empty() && !p.password.is_empty(),
        "Enter the provider username and password"
    );
    anyhow::ensure!(
        ![&p.host, &p.username, &p.password]
            .iter()
            .any(|s| s.contains(['\r', '\n', '\0'])),
        "Connection fields cannot contain line breaks"
    );
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let cfg = nntp_rs::ServerConfig {
            host: p.host.trim().into(),
            port: p.port,
            tls: p.tls,
            allow_insecure_tls: false,
            username: p.username.trim().into(),
            password: p.password.clone(),
        };
        let result = tokio::time::timeout(Duration::from_secs(20), authenticate(cfg)).await;
        match result {
            Ok(Ok(())) => Ok("Provider accepted your login".into()),
            Ok(Err(e)) => Err(anyhow::anyhow!(
                format!("Provider test failed: {e}").replace(&p.password, "[redacted]")
            )),
            Err(_) => Err(anyhow::anyhow!(
                "Connection timed out. Check the hostname, port and TLS setting"
            )),
        }
    })
}

async fn authenticate(cfg: nntp_rs::ServerConfig) -> std::result::Result<(), nntp_rs::NntpError> {
    let mut client = nntp_rs::NntpClient::connect(std::sync::Arc::new(cfg)).await?;
    client.authenticate().await
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::Arc,
    };
    use tokio_rustls::rustls;
    fn probe(accept: bool) -> std::result::Result<(), nntp_rs::NntpError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.der().clone()], key.into())
        .unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream = BufReader::new(rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(tls)).unwrap(),
                stream,
            ));
            stream
                .get_mut()
                .write_all(b"200 Fixture ready\r\n")
                .unwrap();
            stream.get_mut().flush().unwrap();
            let mut line = String::new();
            stream.read_line(&mut line).unwrap();
            assert_eq!(line, "AUTHINFO USER test-user\r\n");
            stream
                .get_mut()
                .write_all(b"381 Password required\r\n")
                .unwrap();
            stream.get_mut().flush().unwrap();
            line.clear();
            stream.read_line(&mut line).unwrap();
            assert_eq!(line, "AUTHINFO PASS test-password\r\n");
            stream
                .get_mut()
                .write_all(if accept {
                    b"281 Authentication accepted\r\n"
                } else {
                    b"481 Authentication rejected\r\n"
                })
                .unwrap();
            stream.get_mut().flush().unwrap();
        });
        let cfg = nntp_rs::ServerConfig {
            host: "127.0.0.1".into(),
            port,
            tls: true,
            allow_insecure_tls: true,
            username: "test-user".into(),
            password: "test-password".into(),
        };
        let r = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(authenticate(cfg));
        server.join().unwrap();
        r
    }
    #[test]
    fn validates_nntp_login() {
        assert!(probe(true).is_ok());
    }
    #[test]
    fn rejects_invalid_nntp_login() {
        assert!(probe(false).is_err());
    }
}
