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
pub fn search_all(
    indexers: Vec<Indexer>,
    query: String,
    category: u32,
) -> (Vec<Release>, Vec<String>) {
    let client = match reqwest::blocking::Client::builder()
        .user_agent(concat!("Starburst/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(25))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (vec![], vec!["Could not create search client".into()]),
    };
    let mut rows = vec![];
    let mut errors = vec![];
    for indexer in indexers {
        match search_filtered(&client, &indexer, &query, category) {
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
    pub category: u32,
}
pub fn search(
    client: &reqwest::blocking::Client,
    indexer: &Indexer,
    query: &str,
) -> Result<Vec<Release>> {
    search_filtered(client, indexer, query, 0)
}
pub fn search_filtered(
    client: &reqwest::blocking::Client,
    indexer: &Indexer,
    query: &str,
    category: u32,
) -> Result<Vec<Release>> {
    anyhow::ensure!(
        !indexer.api_key.trim().is_empty(),
        "Enter an API key in Settings first"
    );
    let mut url = reqwest::Url::parse(indexer.url.trim()).context("Invalid indexer URL")?;
    if !url.path().trim_end_matches('/').ends_with("/api") {
        url.set_path(&format!("{}/api", url.path().trim_end_matches('/')));
    }
    url.query_pairs_mut()
        .clear()
        .append_pair("t", "search")
        .append_pair("apikey", indexer.api_key.trim())
        .append_pair("limit", "100")
        .append_pair("o", "xml");
    if !query.trim().is_empty() {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    if category != 0 {
        url.query_pairs_mut()
            .append_pair("cat", &category.to_string());
    }
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
    parse_response(&text, &indexer.name, Some(&indexer.url))
        .map_err(|e| anyhow::anyhow!(e.to_string().replace(indexer.api_key.trim(), "[redacted]")))
}
#[cfg(test)]
pub fn parse(text: &str, source: &str) -> Result<Vec<Release>> {
    parse_response(text, source, None)
}
fn api_error(code: &str) -> anyhow::Error {
    anyhow::anyhow!(
        match code {
            "100" => "Invalid API key. Copy the indexer's API key from your account settings.",
            "101" | "102" => "Indexer account is suspended or not permitted to use the API.",
            "910" => "Indexer API quota or download limit reached. Check your account limits.",
            "200" | "201" | "202" => "Indexer rejected the search request or parameters.",
            _ => "Indexer rejected the request. Check API access and account limits.",
        }
        .to_string()
            + &format!(" (code {code})")
    )
}
fn download_url(raw: &str, base: Option<&str>) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim())
        .or_else(|_| reqwest::Url::parse(base.unwrap_or("")).and_then(|b| b.join(raw.trim())))
        .ok()?;
    ["https", "http"]
        .contains(&url.scheme())
        .then(|| url.to_string())
}
fn parse_response(text: &str, source: &str, base: Option<&str>) -> Result<Vec<Release>> {
    if text.trim_start().starts_with('{') {
        return parse_json(text, source, base);
    }
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: 200_000,
    };
    let doc = roxmltree::Document::parse_with_options(text, options).map_err(|_| {
        anyhow::anyhow!("Indexer returned invalid XML (possibly a login or error page)")
    })?;
    if let Some(e) = doc.descendants().find(|n| n.has_tag_name("error")) {
        return Err(api_error(e.attribute("code").unwrap_or("unknown")));
    }
    anyhow::ensure!(
        doc.root_element().has_tag_name("rss")
            && doc.descendants().any(|n| n.has_tag_name("channel")),
        "Indexer returned a page instead of a Newznab RSS feed. Check the API endpoint."
    );
    let mut rows = vec![];
    let mut skipped = 0;
    for n in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let title = n
            .children()
            .find(|n| n.has_tag_name("title"))
            .and_then(|n| n.text())
            .unwrap_or("Untitled release")
            .to_owned();
        let enclosure = n.children().find(|n| n.has_tag_name("enclosure"));
        let raw = enclosure.and_then(|e| e.attribute("url")).or_else(|| {
            n.children()
                .find(|n| n.has_tag_name("link"))
                .and_then(|n| n.text())
        });
        let Some(url) = raw.and_then(|u| download_url(u, base)) else {
            skipped += 1;
            continue;
        };
        let size = enclosure
            .and_then(|e| e.attribute("length"))
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                n.children()
                    .find(|n| n.has_tag_name("attr") && n.attribute("name") == Some("size"))
                    .and_then(|n| n.attribute("value"))
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(0);
        rows.push(Release {
            title,
            url,
            size,
            source: source.into(),
            category: n
                .children()
                .filter(|n| n.has_tag_name("attr") && n.attribute("name") == Some("category"))
                .filter_map(|n| n.attribute("value").and_then(|v| v.parse::<u32>().ok()))
                .max()
                .unwrap_or(0),
        });
    }
    anyhow::ensure!(
        skipped == 0 || !rows.is_empty(),
        "Indexer returned {skipped} releases, but none had usable download links"
    );
    Ok(rows)
}
fn parse_json(text: &str, source: &str, base: Option<&str>) -> Result<Vec<Release>> {
    let json: serde_json::Value =
        serde_json::from_str(text).context("Indexer returned invalid JSON")?;
    if let Some(e) = json.get("error") {
        return Err(api_error(
            e.get("code")
                .or_else(|| e.pointer("/@attributes/code"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
        ));
    }
    let channel = json
        .pointer("/rss/channel")
        .or_else(|| json.get("channel"))
        .context("Indexer returned JSON without a Newznab channel")?;
    let Some(items) = channel.get("item") else {
        return Ok(vec![]);
    };
    let entries = items
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![items.clone()]);
    let mut rows = vec![];
    for item in &entries {
        let enclosure = item
            .get("enclosure")
            .and_then(|v| v.get("@attributes").or(Some(v)));
        let raw = enclosure
            .and_then(|v| v.get("url"))
            .or_else(|| item.get("link"))
            .and_then(|v| v.as_str());
        if let Some(url) = raw.and_then(|u| download_url(u, base)) {
            let size = enclosure
                .and_then(|v| v.get("length"))
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .unwrap_or(0);
            rows.push(Release {
                title: item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled release")
                    .into(),
                url,
                size,
                source: source.into(),
                category: item
                    .get("category")
                    .and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse().ok())
                            .or_else(|| v.as_u64().map(|n| n as u32))
                    })
                    .unwrap_or(0),
            });
        }
    }
    anyhow::ensure!(
        entries.is_empty() || !rows.is_empty(),
        "Indexer returned releases without usable download links"
    );
    Ok(rows)
}
pub fn test(indexer: &Indexer) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("Starburst/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(25))
        .build()?;
    let rows = search(&client, indexer, "")?;
    Ok(format!(
        "Search API accepted the key. Retrieved {} recent releases.",
        rows.len()
    ))
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
        assert!(parse("<rss><channel><item><title>x</title><link>file:///etc/passwd</link></item></channel></rss>","test").is_err());
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
            assert!(request.contains("cat=5040"));
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
        let (rows, errors) = search_all(vec![i], "hello world&test".into(), 5040);
        assert!(errors.is_empty());
        server.join().unwrap();
        assert_eq!(rows[0].size, 42);
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;
    #[test]
    fn accepts_realistic_newznab_dtd_and_cdata() {
        let xml = r#"<?xml version="1.0"?><!DOCTYPE rss SYSTEM "http://example.org/rss.dtd"><rss xmlns:newznab="http://www.newznab.com/DTD/2010/feeds/attributes/"><channel><item><title><![CDATA[Star Wars 2160p]]></title><enclosure url="https://example.org/get?id=1&amp;apikey=key" length="123456"/><newznab:attr name="size" value="123456"/><newznab:attr name="category" value="5045"/></item></channel></rss>"#;
        let rows = parse(xml, "NZBGeek").unwrap();
        assert_eq!(rows[0].title, "Star Wars 2160p");
        assert_eq!(rows[0].size, 123456);
        assert_eq!(rows[0].category, 5045);
    }
    #[test]
    fn html_login_is_error_not_zero_results() {
        assert!(parse("<html><body>Login required</body></html>", "test").is_err());
    }
    #[test]
    fn errors_explain_invalid_key() {
        assert!(parse(
            r#"<error code="100" description="Invalid API Key"/>"#,
            "test"
        )
        .unwrap_err()
        .to_string()
        .contains("Invalid API key"));
    }
    #[test]
    fn accepts_json_single_item_and_relative_url() {
        let rows=parse_response(r#"{"channel":{"item":{"title":"Star Wars","enclosure":{"@attributes":{"url":"/get?id=1","length":"42"}}}}}"#,"test",Some("https://example.org/api")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "https://example.org/get?id=1");
    }
    #[test]
    fn valid_empty_feed_is_empty() {
        assert!(parse("<rss><channel/></rss>", "test").unwrap().is_empty());
    }
}
