use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls: bool,
    pub connections: u16,
    pub level: u16,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            name: "My provider".into(),
            host: String::new(),
            port: 563,
            username: String::new(),
            password: String::new(),
            tls: true,
            connections: 8,
            level: 0,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Indexer {
    pub name: String,
    pub url: String,
    pub api_key: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub providers: Vec<Provider>,
    pub indexers: Vec<Indexer>,
    pub destination: PathBuf,
    pub unpack: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            providers: vec![],
            indexers: vec![],
            destination: directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("Starburst"),
            unpack: true,
        }
    }
}
pub fn root() -> PathBuf {
    directories::ProjectDirs::from("", "", "starburst")
        .expect("home directory unavailable")
        .config_dir()
        .to_path_buf()
}
pub fn private_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("No parent directory")?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let temp = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(data)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}
impl Config {
    pub fn load() -> Result<Self> {
        let p = root().join("settings.json");
        if !p.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(p)?)?)
    }
    pub fn validate(&self) -> Result<()> {
        let safe = |s: &str| !s.contains(['\r', '\n', '\0']);
        if !self.destination.is_absolute() || !safe(&self.destination.to_string_lossy()) {
            bail!("Choose an absolute download folder without line breaks");
        }
        for p in &self.providers {
            if p.host.trim().is_empty()
                || p.port == 0
                || p.connections == 0
                || ![&p.name, &p.host, &p.username, &p.password]
                    .iter()
                    .all(|v| safe(v))
            {
                bail!("Each provider needs a hostname, port and connections; line breaks are not allowed");
            }
        }
        for i in &self.indexers {
            let u = reqwest::Url::parse(&i.url).context("Indexer needs a full http(s) URL")?;
            if !["http", "https"].contains(&u.scheme()) || u.host_str().is_none() {
                bail!("Indexer needs an http(s) URL");
            }
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        private_write(
            &root().join("settings.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_config_injection() {
        let mut c = Config::default();
        let p = Provider {
            host: "news.example\nControlIP=0.0.0.0".into(),
            ..Default::default()
        };
        c.providers.push(p);
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_relative_destination() {
        let c = Config {
            destination: "relative".into(),
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }
}
