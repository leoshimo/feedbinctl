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
pub struct SavedSearch {
    pub id: i64,
    pub name: String,
    pub query: String,
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

#[derive(Debug)]
pub struct SavedSearchEntryPage {
    pub entry_ids: Vec<i64>,
    pub next: Option<String>,
    pub total: Option<usize>,
}

#[derive(Serialize)]
struct NewPage<'a> {
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
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

    pub async fn saved_searches(&self) -> Result<Vec<SavedSearch>> {
        self.get("/saved_searches.json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("failed to decode saved searches")
    }

    pub async fn saved_search_entry_ids_page(
        &self,
        id: i64,
        next: Option<&str>,
    ) -> Result<SavedSearchEntryPage> {
        let request = if let Some(url) = next {
            if !url.starts_with(&format!("{}/", self.base_url)) {
                bail!("Feedbin returned an unexpected pagination URL: {url}");
            }
            self.authenticated(self.client.get(url))
        } else {
            self.get(&format!("/saved_searches/{id}.json"))
        };

        let response = request.send().await?.error_for_status()?;
        let next = next_link(response.headers());
        let total = record_count(response.headers());
        let entry_ids = response
            .json()
            .await
            .with_context(|| format!("failed to decode saved search {id}"))?;
        Ok(SavedSearchEntryPage {
            entry_ids,
            next,
            total,
        })
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

    pub async fn save_page(&self, url: &str, title: Option<&str>) -> Result<FeedEntry> {
        self.authenticated(
            self.client
                .post(format!("{}/pages.json", self.base_url))
                .json(&NewPage { url, title }),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("failed to decode saved page {url}"))
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
        let total = record_count(response.headers());
        let entries = response.json().await.context("failed to decode entries")?;

        Ok(EntryPage {
            entries,
            next,
            total,
        })
    }
}

fn record_count(headers: &HeaderMap) -> Option<usize> {
    headers
        .get(RECORD_COUNT)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
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
    use wiremock::matchers::{
        basic_auth, body_json, method, path, query_param, query_param_is_missing,
    };
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

    #[tokio::test]
    async fn lists_saved_searches_and_pages_their_entry_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/saved_searches.json"))
            .and(basic_auth("user", "pass"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 7,
                    "name": "Emacs",
                    "query": "emacs is:unread"
                }])),
            )
            .mount(&server)
            .await;

        let next = format!("{}/saved_searches/7.json?page=2", server.uri());
        let link = format!(r#"<{next}>; rel="next""#);
        Mock::given(method("GET"))
            .and(path("/saved_searches/7.json"))
            .and(query_param_is_missing("page"))
            .and(basic_auth("user", "pass"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("link", link.as_str())
                    .insert_header(RECORD_COUNT, "3")
                    .set_body_json(serde_json::json!([30, 20])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/saved_searches/7.json"))
            .and(query_param("page", "2"))
            .and(basic_auth("user", "pass"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([10])))
            .mount(&server)
            .await;

        let client = FeedbinClient::with_base_url("user", "pass", server.uri());
        assert_eq!(
            client.saved_searches().await.unwrap(),
            [SavedSearch {
                id: 7,
                name: "Emacs".to_string(),
                query: "emacs is:unread".to_string(),
            }]
        );

        let first = client.saved_search_entry_ids_page(7, None).await.unwrap();
        assert_eq!(first.entry_ids, [30, 20]);
        assert_eq!(first.total, Some(3));
        let second = client
            .saved_search_entry_ids_page(7, first.next.as_deref())
            .await
            .unwrap();
        assert_eq!(second.entry_ids, [10]);
    }

    #[tokio::test]
    async fn saves_a_page_with_an_optional_title() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/pages.json"))
            .and(basic_auth("user", "pass"))
            .and(body_json(serde_json::json!({
                "url": "https://example.com/article",
                "title": "Example article"
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 10,
                "feed_id": 20,
                "title": "Example article",
                "author": null,
                "summary": null,
                "content": "<p>Article</p>",
                "url": "https://example.com/article",
                "extracted_content_url": null,
                "published": "2026-09-03T12:00:00Z",
                "created_at": "2026-09-03T12:01:00Z"
            })))
            .mount(&server)
            .await;

        let client = FeedbinClient::with_base_url("user", "pass", server.uri());
        let entry = client
            .save_page("https://example.com/article", Some("Example article"))
            .await
            .unwrap();
        assert_eq!(entry.id, 10);
        assert_eq!(entry.feed_id, 20);
    }
}
