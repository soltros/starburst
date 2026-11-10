use crate::config::{self, Config};
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::mpsc, time::Duration};
use usenet_dl::{DownloadId, DownloadOptions, Status, UsenetDownloader};

pub enum Command {
    AddUrl(String, String),
    AddFile(PathBuf),
    Pause(i64),
    Resume(i64),
    Cancel(i64),
    PauseAll,
    ResumeAll,
    Shutdown,
}
pub enum Update {
    Ready,
    Message(String),
    Error(String),
    Queue(Vec<Row>),
}
pub struct Row {
    pub id: i64,
    pub name: String,
    pub status: Status,
    pub progress: f32,
    pub size: u64,
    pub speed: u64,
    pub error: Option<String>,
}
pub struct Worker {
    pub tx: tokio::sync::mpsc::UnboundedSender<Command>,
    pub rx: mpsc::Receiver<Update>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(config: Config, ctx: eframe::egui::Context) -> Self {
        let (tx, mut commands) = tokio::sync::mpsc::unbounded_channel();
        let (updates, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let send = |u| {
                let _ = updates.send(u);
                ctx.request_repaint();
            };
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(r) => r,
                Err(e) => {
                    send(Update::Error(e.to_string()));
                    return;
                }
            };
            runtime.block_on(async {
                let mut cfg=usenet_dl::Config { servers:config.providers.iter().map(|p|usenet_dl::ServerConfig { host:p.host.clone(),port:p.port,tls:p.tls,username:Some(p.username.clone()),password:Some(p.password.clone()),connections:p.connections as usize,priority:p.level as i32,pipeline_depth:10 }).collect(), ..Default::default() };
                cfg.download.download_dir=config.destination.clone();
                cfg.download.temp_dir=config.destination.join(".incomplete");
                cfg.download.default_post_process=if config.unpack {usenet_dl::config::PostProcess::Unpack} else {usenet_dl::config::PostProcess::None};
                cfg.download.delete_samples=false;
                cfg.persistence.database_path=config::root().join("queue.sqlite");
                let downloader=match UsenetDownloader::new(cfg).await {Ok(d)=>d,Err(e)=>{send(Update::Error(format!("Could not initialize downloads: {e}")));return;}};
                if let Err(e)=downloader.restore_queue().await { send(Update::Error(format!("Could not restore queue: {e}"))); }
                let processor=downloader.start_queue_processor();
                send(Update::Ready);
                let mut tick=tokio::time::interval(Duration::from_secs(1));
                loop { tokio::select! {
                    _=tick.tick()=> {
                        match downloader.db.get_all_downloads().await {
                            Ok(rows)=>send(Update::Queue(rows.into_iter().map(|d|Row {id:d.id,name:d.name,status:Status::from_i32(d.status),progress:d.progress,size:d.size_bytes.max(0) as u64,speed:d.speed_bps.max(0) as u64,error:d.error_message}).collect())),
                            Err(e)=>send(Update::Error(format!("Could not read queue: {e}"))),
                        }
                    }
                    command=commands.recv()=> {
                        let Some(command)=command else {break;};
                        if matches!(command,Command::Shutdown) { break; }
                        let result:Result<()>=async { match command {
                            Command::AddUrl(url, name)=> {
                                let d=downloader.clone();let updates=updates.clone();let ctx=ctx.clone();
                                tokio::spawn(async move { let result=fetch_nzb(&url).await; match result {Ok(bytes)=>match d.add_nzb_content(&bytes,&name,DownloadOptions::default()).await {Ok(_)=>{let _=updates.send(Update::Message("Added to your queue".into()));},Err(e)=>{let _=updates.send(Update::Error(format!("Could not add NZB: {e}")));}},Err(e)=>{let _=updates.send(Update::Error(e.to_string()));}} ctx.request_repaint(); });
                            }
                            Command::AddFile(path)=>{downloader.add_nzb(&path,DownloadOptions::default()).await?;send(Update::Message("Added to your queue".into()));}
                            Command::Pause(id)=>downloader.pause(DownloadId(id)).await?,
                            Command::Resume(id)=>downloader.resume(DownloadId(id)).await?,
                            Command::Cancel(id)=>downloader.cancel(DownloadId(id)).await?,
                            Command::PauseAll=>downloader.pause_all().await?,
                            Command::ResumeAll=>downloader.resume_all().await?,
                            Command::Shutdown=>{}
                        } Ok(()) }.await;
                        if let Err(e)=result {send(Update::Error(e.to_string()));}
                    }
                }}
                processor.abort();
                if let Err(e)=downloader.shutdown().await { send(Update::Error(format!("Shutdown: {e}"))); }
            });
        });
        Self {
            tx,
            rx,
            handle: Some(handle),
        }
    }
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
async fn fetch_nzb(url: &str) -> Result<Vec<u8>> {
    let url = reqwest::Url::parse(url).context("Invalid NZB URL")?;
    anyhow::ensure!(
        ["http", "https"].contains(&url.scheme()),
        "NZB URL must use HTTP or HTTPS"
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("NZB download connection failed"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "NZB server returned HTTP {}",
        response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("Could not read NZB"))?
    {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 32 * 1024 * 1024,
            "NZB exceeds 32 MiB limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn embedded_queue_persists_imported_nzb_across_restarts() {
        let root = std::env::temp_dir().join(format!(
            "starburst-queue-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut cfg = usenet_dl::Config::default();
        cfg.download.download_dir = root.join("downloads");
        cfg.download.temp_dir = root.join("incomplete");
        cfg.persistence.database_path = root.join("queue.sqlite");
        cfg.download.default_post_process = usenet_dl::config::PostProcess::None;
        let d = UsenetDownloader::new(cfg.clone()).await.unwrap();
        let nzb=br#"<?xml version="1.0"?><nzb xmlns="http://www.newzbin.com/DTD/2003/nzb"><file poster="fixture" date="1700000000" subject="&quot;hello.txt&quot; yEnc (1/1)"><groups><group>alt.test</group></groups><segments><segment bytes="5" number="1">fixture@example.org</segment></segments></file></nzb>"#;
        let id = d
            .add_nzb_content(nzb, "fixture.nzb", DownloadOptions::default())
            .await
            .unwrap();
        assert_eq!(d.db.get_all_downloads().await.unwrap().len(), 1);
        d.shutdown().await.unwrap();
        drop(d);
        let reopened = UsenetDownloader::new(cfg).await.unwrap();
        let rows = reopened.db.get_all_downloads().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id.0);
        assert_eq!(rows[0].size_bytes, 5);
        reopened.shutdown().await.unwrap();
        drop(reopened);
        let _ = std::fs::remove_dir_all(root);
    }
}
