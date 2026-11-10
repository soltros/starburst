//! Validate indexer downloads before sending them to the embedded NNTP engine.
use anyhow::{Context, Result};
use std::io::Read;
pub fn prepare(bytes: &[u8]) -> Result<Vec<u8>> {
    let decoded;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut out)
            .context("Could not decompress the NZB response")?;
        anyhow::ensure!(
            out.len() <= 32 * 1024 * 1024,
            "Decompressed NZB exceeds 32 MiB"
        );
        decoded = out;
        &decoded[..]
    } else {
        bytes
    };
    let text = std::str::from_utf8(bytes)
        .context("Indexer download was not a UTF-8 NZB manifest")?
        .trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') {
        let json: serde_json::Value = serde_json::from_str(text)
            .context("Indexer returned an invalid JSON response instead of an NZB")?;
        if let Some(e) = json.get("error") {
            let code = e.get("code").or_else(|| e.pointer("/@attributes/code"));
            return Err(crate::indexer::api_error(
                code.and_then(|v| v.as_str()).unwrap_or("unknown"),
            ));
        }
        anyhow::bail!("Indexer returned JSON instead of the requested NZB download");
    }
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000_000,
        },
    )
    .map_err(|_| {
        anyhow::anyhow!("Indexer returned an invalid XML response instead of an NZB download")
    })?;
    if let Some(e) = doc.descendants().find(|n| n.has_tag_name("error")) {
        return Err(crate::indexer::api_error(
            e.attribute("code").unwrap_or("unknown"),
        ));
    }
    anyhow::ensure!(doc.root_element().has_tag_name("nzb"),"Indexer returned a web page or feed instead of an NZB download. Check download permissions and the indexer's API endpoint.");
    let mut nzb = nntp_rs::Nzb {
        meta: Default::default(),
        files: vec![],
    };
    if let Some(head) = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("head"))
    {
        for meta in head.children().filter(|n| n.has_tag_name("meta")) {
            if let (Some(kind), Some(value)) = (meta.attribute("type"), meta.text()) {
                nzb.meta.insert(kind.into(), value.into());
            }
        }
    }
    for file in doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("file"))
    {
        let mut item = nntp_rs::NzbFile {
            poster: file.attribute("poster").unwrap_or("").into(),
            date: file
                .attribute("date")
                .unwrap_or("0")
                .parse()
                .context("NZB has an invalid posting date")?,
            subject: file
                .attribute("subject")
                .context("NZB file is missing its subject")?
                .into(),
            groups: vec![],
            segments: vec![],
        };
        if let Some(groups) = file.children().find(|n| n.has_tag_name("groups")) {
            item.groups = groups
                .children()
                .filter(|n| n.has_tag_name("group"))
                .filter_map(|n| n.text())
                .map(|s| s.trim().to_string())
                .collect();
        }
        if let Some(segments) = file.children().find(|n| n.has_tag_name("segments")) {
            for segment in segments.children().filter(|n| n.has_tag_name("segment")) {
                let message_id = segment.text().unwrap_or("").trim().to_string();
                anyhow::ensure!(
                    !message_id.is_empty(),
                    "NZB contains a segment without a message ID"
                );
                item.segments.push(nntp_rs::NzbSegment {
                    bytes: segment
                        .attribute("bytes")
                        .context("NZB segment is missing its size")?
                        .parse()
                        .context("NZB segment has an invalid size")?,
                    number: segment
                        .attribute("number")
                        .context("NZB segment is missing its number")?
                        .parse()
                        .context("NZB segment has an invalid number")?,
                    message_id,
                });
            }
        }
        nzb.files.push(item);
    }
    anyhow::ensure!(
        !nzb.files.is_empty(),
        "The indexer supplied an empty NZB with no file entries"
    );
    nzb.validate()
        .context("The indexer supplied an invalid NZB manifest")?;
    // Normalize namespace prefixes and CDATA for the backend's stricter parser.
    Ok(nzb.to_xml().into_bytes())
}
#[cfg(test)]
mod tests {
    use super::*;
    const NZB: &str = r#"<n:nzb xmlns:n="http://www.newzbin.com/DTD/2003/nzb"><n:head><n:meta type="password">secret</n:meta></n:head><n:file subject="&quot;hello.bin&quot; yEnc" poster="test" date="1700000000"><n:groups><n:group>alt.test</n:group></n:groups><n:segments><n:segment bytes="5" number="1"><![CDATA[part@example.org]]></n:segment></n:segments></n:file></n:nzb>"#;
    #[test]
    fn normalizes_namespaces_and_cdata_for_backend() {
        let bytes = prepare(NZB.as_bytes()).unwrap();
        let parsed = nntp_rs::parse_nzb(std::str::from_utf8(&bytes).unwrap()).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed.files[0].segments[0].message_id, "part@example.org");
        assert_eq!(parsed.meta["password"], "secret");
    }
    #[test]
    fn reports_download_quota_instead_of_empty_nzb() {
        let e =
            prepare(br#"<error code="501" description="Download limit reached"/>"#).unwrap_err();
        assert!(e.to_string().contains("download limit"));
    }
    #[test]
    fn rejects_html_and_search_feeds() {
        assert!(prepare(b"<html><body>Login</body></html>").is_err());
        assert!(prepare(b"<rss><channel/></rss>").is_err());
    }
    #[test]
    fn decompresses_gzip_manifest() {
        use std::io::Write;
        let mut z = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        z.write_all(NZB.as_bytes()).unwrap();
        assert!(prepare(&z.finish().unwrap()).is_ok());
    }
}
