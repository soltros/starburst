use crate::config::Indexer;
use anyhow::{bail, Context, Result};
pub struct Preset {
    pub name: &'static str,
    pub url: &'static str,
    pub site: &'static str,
}
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "NZBGeek",
        url: "https://api.nzbgeek.info/api",
        site: "https://nzbgeek.info",
    },
    Preset {
        name: "NZBPlanet",
        url: "https://api.nzbplanet.net/api",
        site: "https://nzbplanet.net",
    },
    Preset {
        name: "DrunkenSlug",
        url: "https://drunkenslug.com/api",
        site: "https://drunkenslug.com",
    },
    Preset {
        name: "NZBFinder",
        url: "https://nzbfinder.ws/api",
        site: "https://nzbfinder.ws",
    },
    Preset {
        name: "altHUB",
        url: "https://api.althub.co.za/api",
        site: "https://althub.co.za",
    },
];
pub fn preset_indexer(preset: &Preset) -> Indexer {
    Indexer {
        name: preset.name.into(),
        url: preset.url.into(),
        api_key: String::new(),
    }
}
pub fn search_all(indexers: Vec<Indexer>, query: String) -> (Vec<Release>, Vec<String>) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (vec![], vec!["Could not create search client".into()]),
    };
    let mut rows = vec![];
    let mut errors = vec![];
    for indexer in indexers {
        match search(&client, &indexer, &query) {
            Ok(mut r) => rows.append(&mut r),
            Err(e) => errors.push(format!("{}: {e}", indexer.name)),
        }
    }
    (rows, errors)
}
#[derive(Clone, Debug)]
pub struct Release {
    pub title: String,
    pub url: String,
    pub size: u64,
    pub source: String,
}
pub fn search(
    client: &reqwest::blocking::Client,
    indexer: &Indexer,
    query: &str,
) -> Result<Vec<Release>> {
    let mut url = reqwest::Url::parse(&indexer.url)?;
    if !url.path().trim_end_matches('/').ends_with("/api") {
        url.set_path(&format!("{}/api", url.path().trim_end_matches('/')));
    }
    url.query_pairs_mut()
        .clear()
        .append_pair("t", "search")
        .append_pair("q", query)
        .append_pair("apikey", &indexer.api_key)
        .append_pair("limit", "100")
        .append_pair("o", "xml");
    // Deliberately omit request URLs from errors: they contain API keys.
    let response = client
        .get(url)
        .send()
        .map_err(|_| anyhow::anyhow!("Indexer connection failed"))?;
    if !response.status().is_success() {
        bail!("Indexer returned HTTP {}", response.status().as_u16());
    }
    let text = response
        .text()
        .map_err(|_| anyhow::anyhow!("Could not read indexer response"))?;
    parse(&text, &indexer.name)
}
pub fn parse(text: &str, source: &str) -> Result<Vec<Release>> {
    let doc = roxmltree::Document::parse(text).context("Indexer returned invalid XML")?;
    if let Some(e) = doc.descendants().find(|n| n.has_tag_name("error")) {
        bail!(
            "Indexer API error {}",
            e.attribute("code").unwrap_or("unknown")
        );
    }
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter_map(|n| {
            let title = n
                .children()
                .find(|n| n.has_tag_name("title"))?
                .text()?
                .to_owned();
            let enclosure = n.children().find(|n| n.has_tag_name("enclosure"));
            let raw = enclosure.and_then(|e| e.attribute("url")).or_else(|| {
                n.children()
                    .find(|n| n.has_tag_name("link"))
                    .and_then(|n| n.text())
            })?;
            let url = reqwest::Url::parse(raw).ok()?;
            if !["https", "http"].contains(&url.scheme()) {
                return None;
            }
            let size = enclosure
                .and_then(|e| e.attribute("length"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            Some(Release {
                title,
                url: url.to_string(),
                size,
                source: source.into(),
            })
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_entities_and_enclosures() {
        let r=parse(r#"<rss><channel><item><title>A &amp; B</title><enclosure url="https://example.org/get?a=1&amp;b=2" length="123"/></item></channel></rss>"#,"test").unwrap();
        assert_eq!(r[0].title, "A & B");
        assert_eq!(r[0].size, 123);
        assert!(r[0].url.contains("&b=2"));
    }
    #[test]
    fn reports_api_errors() {
        assert!(parse(r#"<error code="100" description="secret"/>"#, "test").is_err());
    }
    #[test]
    fn rejects_non_http_links() {
        assert!(parse(
            "<rss><item><title>x</title><link>file:///etc/passwd</link></item></rss>",
            "test"
        )
        .unwrap()
        .is_empty());
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn sends_encoded_newznab_query_to_api_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut buf = [0; 4096];
            let len = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..len]);
            assert!(request.starts_with("GET /api?"));
            assert!(request.contains("q=hello+world%26test"));
            assert!(request.contains("apikey=private-key"));
            let body="<rss><channel><item><title>Example</title><enclosure url=\"https://example.org/file.nzb\" length=\"42\"/></item></channel></rss>";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let i = Indexer {
            name: "Local fixture".into(),
            url: format!("http://{addr}"),
            api_key: "private-key".into(),
        };
        let (rows, errors) = search_all(vec![i], "hello world&test".into());
        assert!(errors.is_empty());
        server.join().unwrap();
        assert_eq!(rows[0].size, 42);
    }
}
