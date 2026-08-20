use anyhow::{Context, Result};
use reqwest::header::USER_AGENT;
use reqwest::{Client, Url};
use std::time::Duration;

use crate::api::FeedEntry;

pub fn webpage_client() -> Client {
    Client::new()
}

pub async fn apply_twitter_workarounds(client: &Client, entry: &mut FeedEntry) -> Result<bool> {
    if !unhelpful_title(entry.title.as_deref()) {
        return Ok(false);
    }
    let Some(url) = entry.url.as_deref() else {
        return Ok(false);
    };
    let Some(handle) = twitter_status_handle(url) else {
        return Ok(false);
    };

    let html = client
        .get(url)
        .header(
            USER_AGENT,
            concat!("feedbinctl/", env!("CARGO_PKG_VERSION")),
        )
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to fetch {url}"))?
        .text()
        .await
        .with_context(|| format!("failed to read {url}"))?;
    let Some(page_title) = html_title(&html) else {
        return Ok(false);
    };
    let Some(title) = corrected_twitter_title(&handle, &page_title) else {
        return Ok(false);
    };

    entry.title = Some(title);
    Ok(true)
}

fn unhelpful_title(title: Option<&str>) -> bool {
    let title = title.unwrap_or_default().trim();
    title.is_empty()
        || title == "JavaScript is not available."
        || (title.ends_with(" on X") && title.contains("(@"))
        || (title.ends_with(" on Twitter") && title.contains("(@"))
}

fn twitter_status_handle(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?.trim_start_matches("www.");
    if !matches!(host, "x.com" | "twitter.com" | "mobile.twitter.com") {
        return None;
    }

    let segments = url.path_segments()?.collect::<Vec<_>>();
    if segments.len() < 3 || segments[1] != "status" || segments[0].is_empty() {
        return None;
    }
    Some(segments[0].to_string())
}

fn html_title(html: &str) -> Option<String> {
    let title_start = find_ascii_case_insensitive(html, "<title")?;
    let content_start = html[title_start..].find('>')? + title_start + 1;
    let content_end =
        find_ascii_case_insensitive(&html[content_start..], "</title>")? + content_start;
    let title = html_escape::decode_html_entities(&html[content_start..content_end]);
    let title = normalize_whitespace(&title);
    (!title.is_empty()).then_some(title)
}

fn corrected_twitter_title(handle: &str, page_title: &str) -> Option<String> {
    for (separator, suffix) in [(" on X: ", " / X"), (" on Twitter: ", " / Twitter")] {
        if let Some((display_name, text)) = page_title.split_once(separator) {
            let text = text.strip_suffix(suffix).unwrap_or(text).trim();
            let text = text.strip_prefix('"').unwrap_or(text);
            let text = text.strip_suffix('"').unwrap_or(text);
            let text = clean_post_text(text);
            if !display_name.trim().is_empty() && !text.is_empty() {
                return Some(format!("{} (@{}): {}", display_name.trim(), handle, text));
            }
        }
    }

    for suffix in [" on X / X", " on Twitter / Twitter"] {
        if let Some(display_name) = page_title.strip_suffix(suffix)
            && !display_name.trim().is_empty()
        {
            return Some(format!("{} (@{}): Media post", display_name.trim(), handle));
        }
    }
    None
}

fn clean_post_text(text: &str) -> String {
    let words = text.split_whitespace().filter(|word| {
        let candidate = word.trim_matches(|character: char| {
            matches!(
                character,
                '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | '.' | '!' | '?'
            )
        });
        !candidate.starts_with("https://t.co/")
            && !candidate.starts_with("http://t.co/")
            && !candidate.starts_with("pic.twitter.com/")
    });
    normalize_whitespace(&words.collect::<Vec<_>>().join(" "))
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_a_title_from_x_webpage_metadata() {
        let html = r#"<html><head><title>Parker Conley on X: &quot;Ex-Googler @ants_everywhere does &quot;code archeology&quot;. https://t.co/example&quot; / X</title></head></html>"#;

        let page_title = html_title(html).unwrap();
        assert_eq!(
            corrected_twitter_title("parconley", &page_title).as_deref(),
            Some(
                "Parker Conley (@parconley): Ex-Googler @ants_everywhere does \"code archeology\"."
            )
        );
    }

    #[test]
    fn derives_a_media_fallback_from_x_webpage_metadata() {
        assert_eq!(
            corrected_twitter_title("detahq", "Deta on X / X").as_deref(),
            Some("Deta (@detahq): Media post")
        );
    }

    #[test]
    fn recognizes_only_status_urls_on_x_and_twitter() {
        assert_eq!(
            twitter_status_handle("https://x.com/parconley/status/1938340666804998229?s=12")
                .as_deref(),
            Some("parconley")
        );
        assert_eq!(
            twitter_status_handle("https://mobile.twitter.com/example/status/1").as_deref(),
            Some("example")
        );
        assert_eq!(twitter_status_handle("https://x.com/geoffreylitt"), None);
        assert_eq!(
            twitter_status_handle("https://example.com/person/status/1"),
            None
        );
    }
}
