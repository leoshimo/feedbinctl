use anyhow::{Context, Result, bail};
use keyring::Entry as KeyringEntry;
use reqwest::header::{HeaderMap, LINK};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.feedbin.com/v2";
const PAGE_SIZE: usize = 100;
const RECORD_COUNT: &str = "x-feedbin-record-count";

pub(crate) const KEYRING_SERVICE: &str = "feedbinctl";
pub(crate) const KEYRING_ACCOUNT: &str = "feedbin";

#[derive(Clone)]
pub struct FeedbinClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Subscription {
    pub id: i64,
    pub feed_id: i64,
    pub title: String,
    pub feed_url: String,
    pub site_url: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FeedEntry {
    pub id: i64,
    pub feed_id: i64,
    pub title: Option<String>,
    pub author: Option<String>,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub extracted_content_url: Option<String>,
    pub published: String,
    pub created_at: String,
}

#[derive(Debug)]
pub struct EntryPage {
    pub entries: Vec<FeedEntry>,
    pub next: Option<String>,
    pub total: Option<usize>,
}

impl FeedbinClient {
    pub fn from_stored_credentials() -> Result<Self> {
        let credentials = load_credentials()?;
        let (username, password) = credentials
            .split_once(':')
            .context("FEEDBIN_TOKEN must be in 'username:password' format")?;
        Ok(Self::with_credentials(username, password))
    }

    pub fn with_credentials(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::with_base_url(username, password, API_BASE)
    }

    fn with_base_url(
        username: impl Into<String>,
        password: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            username: username.into(),
            password: password.into(),
        }
    }

    fn authenticated(&self, request: RequestBuilder) -> RequestBuilder {
        request.basic_auth(&self.username, Some(&self.password))
    }

    fn get(&self, path: &str) -> RequestBuilder {
        self.authenticated(self.client.get(format!("{}{path}", self.base_url)))
    }

    pub async fn validate(&self) -> Result<()> {
        let response = self.get("/authentication.json").send().await?;
        if response.status() == StatusCode::OK {
            return Ok(());
        }
        response
            .error_for_status()
            .context("Feedbin rejected these credentials")?;
        Ok(())
    }

    pub async fn subscriptions(&self) -> Result<Vec<Subscription>> {
        self.get("/subscriptions.json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("failed to decode subscriptions")
    }

    pub async fn entry(&self, id: i64) -> Result<FeedEntry> {
        self.get(&format!("/entries/{id}.json"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .with_context(|| format!("failed to decode entry {id}"))
    }

    pub async fn entries_page(&self, next: Option<&str>, since: Option<&str>) -> Result<EntryPage> {
        let request = if let Some(url) = next {
            if !url.starts_with(&format!("{}/", self.base_url)) {
                bail!("Feedbin returned an unexpected pagination URL: {url}");
            }
            self.authenticated(self.client.get(url))
        } else {
            let mut query = vec![("per_page", PAGE_SIZE.to_string())];
            if let Some(since) = since {
                query.push(("since", since.to_string()));
            }
            self.get("/entries.json").query(&query)
        };

        let response = request.send().await?.error_for_status()?;
        let next = next_link(response.headers());
        let total = response
            .headers()
            .get(RECORD_COUNT)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let entries = response.json().await.context("failed to decode entries")?;

        Ok(EntryPage {
            entries,
            next,
            total,
        })
    }
}

fn next_link(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(LINK)
        .iter()
        .chain(headers.get_all("links").iter())
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .find_map(|link| {
            let mut parts = link.split(';');
            let target = parts.next()?.trim();
            let is_next = parts.any(|part| part.trim() == r#"rel="next""#);
            if is_next {
                Some(target.trim_matches(['<', '>']).to_string())
            } else {
                None
            }
        })
}

fn load_credentials() -> Result<String> {
    match std::env::var("FEEDBIN_TOKEN") {
        Ok(credentials) => Ok(credentials),
        Err(_) => KeyringEntry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .context("failed to open keyring entry")?
            .get_password()
            .context("FEEDBIN_TOKEN not set and failed to read credentials from keyring"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;
    use wiremock::matchers::{basic_auth, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn extracts_next_pagination_link() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "links",
            HeaderValue::from_static(
                r#"<https://api.feedbin.com/v2/entries.json?page=1>; rel="first", <https://api.feedbin.com/v2/entries.json?page=3>; rel="next""#,
            ),
        );
        assert_eq!(
            next_link(&headers).as_deref(),
            Some("https://api.feedbin.com/v2/entries.json?page=3")
        );
    }

    #[tokio::test]
    async fn page_accepts_nullable_urls_and_reports_pagination() {
        let server = MockServer::start().await;
        let next = format!("{}/entries.json?page=2", server.uri());
        let link = format!(r#"<{next}>; rel="next""#);
        Mock::given(method("GET"))
            .and(path("/entries.json"))
            .and(query_param("per_page", "100"))
            .and(query_param("since", "2026-01-01T00:00:00Z"))
            .and(basic_auth("user", "pass"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("link", link.as_str())
                    .insert_header(RECORD_COUNT, "1")
                    .set_body_json(serde_json::json!([{
                        "id": 1,
                        "feed_id": 2,
                        "title": "Title only",
                        "author": null,
                        "summary": null,
                        "content": null,
                        "url": null,
                        "extracted_content_url": null,
                        "published": "2026-01-01T00:00:00Z",
                        "created_at": "2026-01-01T00:01:00Z"
                    }])),
            )
            .mount(&server)
            .await;

        let client = FeedbinClient::with_base_url("user", "pass", server.uri());
        let page = client
            .entries_page(None, Some("2026-01-01T00:00:00Z"))
            .await
            .unwrap();

        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].url, None);
        assert_eq!(page.next.as_deref(), Some(next.as_str()));
        assert_eq!(page.total, Some(1));
    }
}
