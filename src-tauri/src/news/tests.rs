use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>News</title><link>https://example.com/</link><description>Test</description>
<item><title>New agent workflow</title><link>https://example.com/agent?utm_source=rss#top</link><pubDate>Sun, 18 Oct 2026 08:00:00 GMT</pubDate><description><![CDATA[<p>Tool use &amp; MCP</p>]]></description></item>
<item><title>Duplicate</title><link>https://example.com/agent</link></item>
<item><title>Unsafe</title><link>javascript:alert(1)</link></item>
<item><title>Undated</title><link>https://example.com/unknown</link></item>
</channel></rss>"#;

#[test]
fn rss_normalizes_links_deduplicates_and_preserves_missing_dates() {
    let items = parse(RSS.as_bytes(), &catalog()[0]).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].url, "https://example.com/agent");
    assert_eq!(items[0].category, "technology");
    assert_eq!(
        items[0].published_at.unwrap().to_rfc3339(),
        "2026-10-18T08:00:00+00:00"
    );
    assert!(items[1].published_at.is_none());
    assert!(items[0].summary.contains("Tool use"));
}

#[test]
fn atom_uses_alternate_link_and_updated_date() {
    let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Releases</title><id>urn:feed</id><updated>2026-10-18T10:00:00Z</updated><entry><id>urn:release</id><title>New model</title><updated>2026-10-17T20:00:00-04:00</updated><link rel="self" href="https://example.com/api"/><link rel="alternate" href="https://example.com/release"/><content type="html">&lt;p&gt;Release notes&lt;/p&gt;</content></entry></feed>"#;
    let items = parse(atom.as_bytes(), &catalog()[3]).unwrap();
    assert_eq!(items[0].url, "https://example.com/release");
    assert_eq!(items[0].category, "models");
    assert_eq!(
        items[0].published_at.unwrap().to_rfc3339(),
        "2026-10-18T00:00:00+00:00"
    );
    assert!(parse(b"<html>blocked</html>", &catalog()[0]).is_err());
    assert!(parse(b"not xml", &catalog()[0]).is_err());
    assert!(canonical_url("file:///C:/secret", "https://example.com").is_none());
    assert!(canonical_url("https://user:password@example.com", "https://example.com").is_none());
}

pub(super) fn server(
    status: &str,
    body: &str,
    headers: &str,
) -> (String, std::thread::JoinHandle<String>) {
    server_bytes(status, body.as_bytes(), headers)
}

fn server_bytes(
    status: &str,
    body: &[u8],
    headers: &str,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/feed", listener.local_addr().unwrap());
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        let mut buffer = [0; 2048];
        loop {
            let size = stream.read(&mut buffer).unwrap();
            if size == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..size]);
            if request.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                break;
            }
        }
        if let Err(error) = stream.write_all(&response) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ));
        }
        String::from_utf8(request).unwrap()
    });
    (address, thread)
}

#[tokio::test]
async fn conditional_http_errors_cache_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let service = NewsService::new(directory.path()).unwrap();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut source = catalog()[0].clone();
    let (url, thread) = server(
        "200 OK",
        RSS,
        "ETag: \"v1\"\r\nLast-Modified: Sun, 18 Oct 2026 08:00:00 GMT\r\n",
    );
    source.url = url;
    let result = fetch(&client, &source).await;
    thread.join().unwrap();
    apply_result(&mut source, result, Utc::now());
    assert!(source.error.is_none());
    assert_eq!(source.items.len(), 2);
    let success = source.last_success;

    let (url, thread) = server("304 Not Modified", "", "");
    source.url = url;
    let result = fetch(&client, &source).await;
    let request = thread.join().unwrap().to_lowercase();
    assert!(request.contains("if-none-match: \"v1\""));
    assert!(request.contains("if-modified-since:"));
    apply_result(&mut source, result, Utc::now());
    assert_eq!(source.items.len(), 2);

    let (url, thread) = server("503 Service Unavailable", "unavailable", "");
    source.url = url;
    let result = fetch(&client, &source).await;
    thread.join().unwrap();
    let last_success = source.last_success;
    apply_result(&mut source, result, Utc::now());
    assert!(source.error.as_ref().unwrap().contains("503"));
    assert_eq!(source.last_success, last_success);
    assert!(source.last_success >= success);
    assert_eq!(source.items.len(), 2);

    let mut snapshot = service.snapshot().unwrap();
    snapshot.sources[0] = source;
    snapshot.automatic = false;
    service.save(&snapshot).unwrap();
    drop(service);
    let reopened = NewsService::new(directory.path()).unwrap();
    let cached = reopened.snapshot().unwrap();
    assert!(!cached.automatic);
    assert_eq!(cached.sources[0].items.len(), 2);
    assert_eq!(cached.sources[0].etag.as_deref(), Some("\"v1\""));
    assert_eq!(cached.sources[0].url, catalog()[0].url);
    assert!(cached.sources[0].error.is_some());
    let result = reopened.refresh(false).await.unwrap();
    assert_eq!(
        result.sources[0].last_attempt,
        cached.sources[0].last_attempt
    );
    assert!(reopened
        .preferences(true, vec!["unknown".into()])
        .await
        .is_err());
}

#[tokio::test]
async fn rejects_oversized_responses_and_invalid_xml_without_losing_items() {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut source = catalog()[0].clone();
    source.items = parse(RSS.as_bytes(), &source).unwrap();
    let (url, thread) = server("200 OK", "<html>blocked</html>", "");
    source.url = url;
    let result = fetch(&client, &source).await;
    thread.join().unwrap();
    apply_result(&mut source, result, Utc::now());
    assert!(source.error.as_ref().unwrap().contains("格式"));
    assert_eq!(source.items.len(), 2);
    let (url, thread) = server("200 OK", &"x".repeat(MAX_BYTES + 1), "");
    source.url = url;
    assert!(fetch(&client, &source)
        .await
        .err()
        .unwrap()
        .contains("4 MiB"));
    thread.join().unwrap();
}

#[tokio::test]
async fn decodes_gzip_feed_before_parsing() {
    let body = [
        31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 179, 41, 42, 46, 86, 40, 75, 45, 42, 206, 204, 207, 179,
        85, 50, 210, 51, 80, 178, 179, 73, 206, 72, 204, 203, 75, 205, 177, 179, 41, 201, 44, 201,
        73, 181, 243, 75, 45, 47, 182, 209, 135, 176, 109, 244, 225, 146, 250, 69, 197, 197, 118,
        0, 121, 110, 5, 75, 63, 0, 0, 0,
    ];
    let (url, thread) = server_bytes(
        "200 OK",
        &body,
        "Content-Encoding: gzip\r\nContent-Type: application/xml\r\n",
    );
    let mut source = catalog()[0].clone();
    source.url = url;
    let client = Client::builder().no_proxy().build().unwrap();
    let result = fetch(&client, &source).await.unwrap();
    thread.join().unwrap();
    assert!(result.items.unwrap().is_empty());
}

#[tokio::test]
async fn automatic_refresh_skips_recent_attempts_and_disabled_sources() {
    let directory = tempfile::tempdir().unwrap();
    let service = NewsService::new(directory.path()).unwrap();
    let mut snapshot = service.snapshot().unwrap();
    for source in &mut snapshot.sources {
        source.last_attempt = Some(Utc::now());
    }
    snapshot.sources[0].enabled = false;
    snapshot.sources[0].last_attempt = None;
    service.save(&snapshot).unwrap();
    let refreshed = service.refresh(false).await.unwrap();
    assert_eq!(
        refreshed.sources[1].last_attempt,
        snapshot.sources[1].last_attempt
    );
    assert!(refreshed
        .sources
        .iter()
        .all(|source| source.last_success.is_none()));
    service.preferences(true, vec![]).await.unwrap();
    let manual = service.refresh(true).await.unwrap();
    assert!(manual.sources.iter().all(|source| !source.enabled));
    assert_eq!(
        manual.sources[1].last_attempt,
        snapshot.sources[1].last_attempt
    );
}
