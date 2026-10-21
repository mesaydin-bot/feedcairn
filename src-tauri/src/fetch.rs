use crate::error::Result;
use reqwest::header::{HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, USER_AGENT};
use reqwest::StatusCode;

const USER_AGENT_VAL: &str =
    "Feedcairn/0.1 (local RSS reader; +https://github.com/feedcairn)";

pub struct Fetched {
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub enum FetchOutcome {
    NotModified,
    Body(Fetched),
}

pub async fn fetch_feed(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<FetchOutcome> {
    let client = client();
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VAL));
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(
            "application/rss+xml, application/atom+xml, application/xml, text/xml;q=0.9, */*;q=0.8",
        ),
    );
    if let Some(etag) = etag.filter(|s| !s.is_empty()) {
        if let Ok(v) = HeaderValue::from_str(etag) {
            headers.insert(IF_NONE_MATCH, v);
        }
    }
    if let Some(lm) = last_modified.filter(|s| !s.is_empty()) {
        if let Ok(v) = HeaderValue::from_str(lm) {
            headers.insert(IF_MODIFIED_SINCE, v);
        }
    }

    let response = client
        .get(url)
        .headers(headers)
        .timeout(std::time::Duration::from_secs(25))
        .send()
        .await?
        .error_for_status()?;

    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(FetchOutcome::NotModified);
    }

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = response.bytes().await?.to_vec();
    Ok(FetchOutcome::Body(Fetched {
        body,
        etag,
        last_modified,
    }))
}

/// Desktop Chrome UA for article page fetches (avoid bot/teaser HTML).
const HTML_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub async fn fetch_html(url: &str) -> Result<String> {
    let client = client();
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(HTML_USER_AGENT));
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("tr-TR,tr;q=0.9,en-US;q=0.8,en;q=0.7"),
    );
    let response = client
        .get(url)
        .headers(headers)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| crate::error::Error::msg(format!("Sayfa indirilemedi: {e}")))?
        .error_for_status()
        .map_err(|e| crate::error::Error::msg(format!("Sayfa indirilemedi (HTTP): {e}")))?;
    let bytes = response.bytes().await.map_err(|e| {
        crate::error::Error::msg(format!("Sayfa gövdesi okunamadı: {e}"))
    })?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Resolve publisher URL when `url` is a Google News `articles` / `rss/articles` link.
/// Non-Google URLs are returned unchanged. Port of googlenewsdecoder `new_decoderv1`.
pub async fn resolve_article_url(url: &str) -> Result<String> {
    let Some(base64_str) = google_news_article_id(url) else {
        return Ok(url.to_string());
    };

    let (signature, timestamp) = match fetch_google_news_decoding_params(&base64_str).await {
        Ok(v) => v,
        Err(e) => {
            return Err(crate::error::Error::msg(format!(
                "Google News yayıncı adresi çözülemedi: {e}"
            )));
        }
    };

    match decode_google_news_url(&signature, &timestamp, &base64_str).await {
        Ok(decoded) if !decoded.trim().is_empty() => Ok(decoded),
        Ok(_) => Err(crate::error::Error::msg(
            "Google News yayıncı adresi boş döndü; tam metin yüklenemedi.",
        )),
        Err(e) => Err(crate::error::Error::msg(format!(
            "Google News yayıncı adresi çözülemedi: {e}"
        ))),
    }
}

/// True when URL is a Google News article / rss/articles / read link.
pub fn is_google_news_article_url(url: &str) -> bool {
    google_news_article_id(url).is_some()
}

fn google_news_article_id(source_url: &str) -> Option<String> {
    let parsed = url::Url::parse(source_url).ok()?;
    let host = parsed.host_str()?;
    if host != "news.google.com" {
        return None;
    }
    let segs: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();
    // …/articles/{id} or …/rss/articles/{id} or …/read/{id}
    let n = segs.len();
    if n >= 2 {
        let kind = segs[n - 2];
        if kind == "articles" || kind == "read" {
            let id = segs[n - 1].trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

async fn fetch_google_news_decoding_params(base64_str: &str) -> Result<(String, String)> {
    let urls = [
        format!("https://news.google.com/articles/{base64_str}"),
        format!("https://news.google.com/rss/articles/{base64_str}"),
    ];
    let mut last_err = String::from("imza/zaman damgası bulunamadı");
    for u in urls {
        match fetch_html(&u).await {
            Ok(html) => match extract_google_news_params(&html) {
                Some(pair) => return Ok(pair),
                None => {
                    last_err = format!("HTML içinde data-n-a-sg/ts yok ({u})");
                }
            },
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }
    Err(crate::error::Error::msg(last_err))
}

fn extract_google_news_params(html: &str) -> Option<(String, String)> {
    let sg = html_attr_value(html, "data-n-a-sg")?;
    let ts = html_attr_value(html, "data-n-a-ts")?;
    if sg.is_empty() || ts.is_empty() {
        return None;
    }
    Some((sg, ts))
}

fn html_attr_value(html: &str, name: &str) -> Option<String> {
    // data-n-a-sg="…" or data-n-a-sg='…'
    let key = format!("{name}=");
    let bytes = html.as_bytes();
    let mut start = 0usize;
    while let Some(rel) = html[start..].find(&key) {
        let i = start + rel + key.len();
        let quote = *bytes.get(i)? as char;
        if quote != '"' && quote != '\'' {
            start = i;
            continue;
        }
        let rest = &html[i + 1..];
        let end = rest.find(quote)?;
        return Some(rest[..end].to_string());
    }
    None
}

async fn decode_google_news_url(
    signature: &str,
    timestamp: &str,
    base64_str: &str,
) -> Result<String> {
    // Matches googlenewsdecoder new_decoderv1.decode_url:
    //   payload = ["Fbv4je", "<garturlreq json string>"]
    //   body = f.req=quote(json.dumps([[payload]]))  → three nested arrays
    let inner = format!(
        r#"["garturlreq",[["X","X",["X","X"],null,null,1,1,"US:en",null,1,null,null,null,null,null,0,1],"X","X",1,[1,1,1],1,1,null,0,0,null,0],"{base64_str}",{timestamp},"{signature}"]"#
    );
    // Python json.dumps uses ", " separators; Google accepts compact, but keep spaces to match.
    let payload_str = format!(r#"[[["Fbv4je", {inner_json}]]]"#, inner_json = serde_json::to_string(&inner).unwrap());
    let body = format!("f.req={}", percent_encode_quote(&payload_str));

    let client = client();
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(HTML_USER_AGENT));
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded;charset=UTF-8"),
    );
    let response = client
        .post("https://news.google.com/_/DotsSplashUi/data/batchexecute")
        .headers(headers)
        .body(body)
        .timeout(std::time::Duration::from_secs(25))
        .send()
        .await
        .map_err(|e| crate::error::Error::msg(format!("batchexecute isteği: {e}")))?
        .error_for_status()
        .map_err(|e| crate::error::Error::msg(format!("batchexecute HTTP: {e}")))?;
    let text = response
        .text()
        .await
        .map_err(|e| crate::error::Error::msg(format!("batchexecute yanıtı: {e}")))?;

    parse_batchexecute_decoded_url(&text)
}

/// urllib.parse.quote equivalent (safe='/') for batchexecute f.req.
fn percent_encode_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0xf) as usize]));
            }
        }
    }
    out
}

fn parse_batchexecute_decoded_url(text: &str) -> Result<String> {
    // Response is typically: )]}'\n\n<json>
    let json_part = text
        .split("\n\n")
        .nth(1)
        .unwrap_or(text)
        .trim()
        .trim_start_matches(")]}'")
        .trim();
    let parsed: serde_json::Value = serde_json::from_str(json_part).map_err(|e| {
        crate::error::Error::msg(format!("batchexecute JSON parse: {e}"))
    })?;
    // Python: json.loads(response.text.split("\n\n")[1])[:-2] → drop last two envelope rows
    let arr = parsed
        .as_array()
        .ok_or_else(|| crate::error::Error::msg("batchexecute: dizi bekleniyordu"))?;
    let first = arr
        .first()
        .ok_or_else(|| crate::error::Error::msg("batchexecute: boş yanıt"))?;
    let inner_str = first
        .get(2)
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::Error::msg("batchexecute: iç yük yok"))?;
    let inner: serde_json::Value = serde_json::from_str(inner_str).map_err(|e| {
        crate::error::Error::msg(format!("batchexecute iç JSON: {e}"))
    })?;
    let decoded = inner
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::Error::msg("batchexecute: yayıncı URL yok"))?;
    Ok(decoded.to_string())
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .expect("reqwest client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_etag_is_skipped() {
        assert!(Some("").filter(|s: &&str| !s.is_empty()).is_none());
        assert!(Some("W/\"abc\"").filter(|s: &&str| !s.is_empty()).is_some());
    }

    #[test]
    fn google_news_id_from_rss_articles() {
        let u = "https://news.google.com/rss/articles/CBMiTEST123?oc=5";
        assert_eq!(
            google_news_article_id(u).as_deref(),
            Some("CBMiTEST123")
        );
        assert!(is_google_news_article_url(u));
        assert!(is_google_news_article_url(
            "https://news.google.com/articles/CBMiABC"
        ));
        assert!(!is_google_news_article_url("https://example.com/a"));
        assert!(google_news_article_id("https://example.com/a").is_none());
    }

    #[tokio::test]
    async fn resolve_article_url_passthrough_non_google() {
        let u = "https://profit.example.com/story/123";
        let out = resolve_article_url(u).await.unwrap();
        assert_eq!(out, u);
    }

    #[test]
    fn extract_params_from_div() {
        let html = r#"<c-wiz><div jscontroller="X" data-n-a-sg="SIG123" data-n-a-ts="1700000000"></div></c-wiz>"#;
        let (sg, ts) = extract_google_news_params(html).unwrap();
        assert_eq!(sg, "SIG123");
        assert_eq!(ts, "1700000000");
    }

    #[tokio::test]
    async fn resolve_google_news_live_article_250() {
        let src = "https://news.google.com/rss/articles/CBMixwFBVV95cUxQM1JNN21xYUs4NV94UHpQOTA3eklVRTM0aVUwSnAyWV9VVExJbjNuOUwyUVQ4NnBWN0pZMm5OaE1seWE2SXZtMy01SXNKbHVGcTJaWllMMGJSc1laVVdfWUVWNVNqSC1rTHg2YVJqV0VsRHYzaHpUeU1TU1FHZVJIWUt3eXJJQy1lNHJxTXhndDRmYWtobWdldkxnS0xzY1ZwdHpNTDdzRFpGTVJzczI5eGozbjZ5a0RMZFlzekZ3OGpsS1RtdWdn?oc=5";
        let resolved = resolve_article_url(src).await.expect("resolve");
        assert!(
            resolved.contains("profit.pakistantoday.com.pk"),
            "got {resolved}"
        );
        assert!(
            resolved.contains("fbr-to-expand-track-trace"),
            "got {resolved}"
        );
    }
}
