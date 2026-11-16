// The local page and the handful of endpoints it talks to.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::navidrome;
use crate::store::{Account, AccountView, Store};

const INDEX: &str = include_str!("../assets/index.html");
const CSS: &str = include_str!("../assets/app.css");
const JS: &str = include_str!("../assets/app.js");
const DONE: &str = include_str!("../assets/done.html");

/// The page sends small JSON bodies; anything larger is not ours.
const MAX_BODY: u64 = 64 * 1024;

pub struct Web {
    pub token: String,
    pub store: Mutex<Store>,
    pub path: PathBuf,
    pub client: ureq::Agent,
    /// Where the Done button sends the account.
    pub callback: String,
}

impl Web {
    pub fn new(token: String, path: PathBuf, callback: String) -> Self {
        let store = Store::load(&path);
        Self { token, store: Mutex::new(store), path, client: navidrome::client(), callback }
    }

    /// The lock guards a whole store, so a poisoned one is still worth reading.
    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn save(&self, store: &Store) {
        if let Err(error) = store.save(&self.path) {
            eprintln!("cannot write {}: {error}", self.path.display());
        }
    }
}

/// Serves the page until the plugin stops the process. Each request that is
/// ready is answered on its own thread: checking a slow server must not hold
/// up the stylesheet the same page is waiting for.
pub fn serve(server: &Server, web: Arc<Web>) {
    for request in server.incoming_requests() {
        let web = Arc::clone(&web);
        if let Err(error) = std::thread::Builder::new().spawn(move || handle(request, &web)) {
            eprintln!("cannot serve a request: {error}");
        }
    }
}

fn handle(request: Request, web: &Web) {
    let url = request.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((path, query)) => (path, query),
        None => (url.as_str(), ""),
    };

    if !authorized(request.headers(), &web.token) {
        // The URL printed by `ready` carries the token once; it becomes a cookie.
        if path == "/" && carries_token(query, &web.token) {
            let cookie = format!("session={}; Path=/; HttpOnly; SameSite=Strict", web.token);
            return respond(
                request,
                303,
                "text/plain; charset=utf-8",
                String::new(),
                vec![("Location", "/"), ("Set-Cookie", &cookie)],
            );
        }
        return respond(
            request,
            403,
            "text/plain; charset=utf-8",
            "Open the link that navidrome-login printed.".to_string(),
            Vec::new(),
        );
    }

    let method = request.method().clone();
    match (method, path) {
        (Method::Get, "/") => asset(request, INDEX, "text/html; charset=utf-8"),
        (Method::Get, "/app.css") => asset(request, CSS, "text/css; charset=utf-8"),
        (Method::Get, "/app.js") => asset(request, JS, "text/javascript; charset=utf-8"),
        (Method::Get, "/api/state") => state(request, web),
        (Method::Post, "/api/login") => login(request, web),
        (Method::Post, "/api/switch") => switch(request, web),
        (Method::Get, "/done") => done(request, web),
        (Method::Delete, path) if path.starts_with(ACCOUNTS) => remove(request, web, &path[ACCOUNTS.len()..]),
        (Method::Put, path) if path.starts_with(ACCOUNTS) => edit(request, web, &path[ACCOUNTS.len()..]),
        _ => respond(request, 404, "text/plain; charset=utf-8", "Not found".to_string(), Vec::new()),
    }
}

/// Everything under here names one saved server.
const ACCOUNTS: &str = "/api/accounts/";

fn asset(request: Request, body: &str, content_type: &str) {
    respond(request, 200, content_type, body.to_string(), Vec::new());
}

#[derive(Serialize)]
struct StateResponse {
    accounts: Vec<AccountView>,
    current: Option<String>,
}

fn state(request: Request, web: &Web) {
    let store = web.store();
    let body = StateResponse {
        accounts: store.accounts.iter().map(|account| account.view()).collect(),
        current: store.current.clone(),
    };
    send(request, 200, &body);
}

#[derive(Deserialize)]
struct LoginRequest {
    #[serde(default)]
    name: String,
    url: String,
    username: String,
    password: String,
}

fn login(mut request: Request, web: &Web) {
    let Some(body) = read_body::<LoginRequest>(&mut request) else {
        return failure(request, 400, "unknown", "Cannot read the request");
    };

    let name = body.name.trim().to_string();
    let url = navidrome::normalize_url(&body.url);
    let username = body.username.trim().to_string();
    let password = body.password;

    if url.is_empty() || username.is_empty() || password.is_empty() {
        return failure(request, 400, "credentials", "Missing a field");
    }

    if let Err(error) = navidrome::ping(&web.client, &url, &username, &password) {
        return failure(request, 400, error.kind, &error.message);
    }

    let account = {
        let mut store = web.store();
        let account = store.upsert(name, url, username, password);
        web.save(&store);
        account
    };
    send(request, 200, &json!({ "ok": true, "account": account.view() }));
}

#[derive(Deserialize)]
struct SwitchRequest {
    id: String,
}

fn switch(mut request: Request, web: &Web) {
    let Some(body) = read_body::<SwitchRequest>(&mut request) else {
        return failure(request, 400, "unknown", "Cannot read the request");
    };

    let account = {
        let mut store = web.store();
        let Some(account) = store.switch(&body.id) else {
            return failure(request, 404, "unknown", "No account with that id");
        };
        web.save(&store);
        account
    };
    send(request, 200, &json!({ "ok": true, "account": account.view() }));
}

#[derive(Deserialize)]
struct EditRequest {
    #[serde(default)]
    name: String,
    url: String,
    username: String,
    #[serde(default)]
    password: String,
}

fn edit(mut request: Request, web: &Web, id: &str) {
    let Some(body) = read_body::<EditRequest>(&mut request) else {
        return failure(request, 400, "unknown", "Cannot read the request");
    };

    let existing = web.store().find(id);
    let Some(existing) = existing else {
        return failure(request, 404, "unknown", "No account with that id");
    };

    let url = navidrome::normalize_url(&body.url);
    let username = body.username.trim().to_string();
    if url.is_empty() || username.is_empty() {
        return failure(request, 400, "credentials", "Missing a field");
    }

    // A blank password keeps the saved one, so a rename doesn't need it again.
    let password = if body.password.is_empty() { existing.password.clone() } else { body.password };

    if let Err(error) = navidrome::ping(&web.client, &url, &username, &password) {
        return failure(request, 400, error.kind, &error.message);
    }

    let account = {
        let mut store = web.store();
        if store.clashes(id, &url, &username) {
            return failure(request, 409, "duplicate", "Another saved server already uses that address and account");
        }
        let Some(account) = store.update(id, body.name.trim().to_string(), url, username, password) else {
            return failure(request, 404, "unknown", "No account with that id");
        };
        web.save(&store);
        account
    };
    send(request, 200, &json!({ "ok": true, "account": account.view() }));
}

fn remove(request: Request, web: &Web, id: &str) {
    let mut store = web.store();
    if !store.remove(id) {
        return failure(request, 404, "unknown", "No account with that id");
    }
    web.save(&store);
    send(request, 200, &json!({ "ok": true }));
}

/// Done shows the way out and hands the account in use to the plugin: as
/// base64 JSON in its callback's query, URL-safe so nothing mangles it.
fn done(request: Request, web: &Web) {
    let account = web.store().current_account().cloned();
    let Some(account) = account else {
        return redirect(request, "/");
    };

    let url = redirect_url(&web.callback, &account);
    let body = DONE.replace("{{server}}", &escape_html(&account.label()));
    respond(request, 200, "text/html; charset=utf-8", body, Vec::new());

    // The card is already answered when the plugin gets this, so closing the
    // helper can't cut the page short.
    std::thread::sleep(Duration::from_millis(300));
    if let Err(error) = navidrome::notify(&web.client, &url) {
        eprintln!("cannot answer the sign-in callback: {error}");
    }
}

/// The server's name goes into the card, so it can't bring markup with it.
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[derive(Serialize)]
struct Payload {
    id: String,
    name: String,
    url: String,
    username: String,
    password: String,
}

pub fn redirect_url(callback: &str, account: &Account) -> String {
    let payload = Payload {
        id: account.id.clone(),
        name: account.label(),
        url: account.url.clone(),
        username: account.username.clone(),
        password: account.password.clone(),
    };
    let json = serde_json::to_vec(&payload).unwrap_or_default();
    let data = base64url(&json);
    let separator = if callback.contains('?') { '&' } else { '?' };
    format!("{callback}{separator}data={data}")
}

/// base64url without padding, so `+`, `/` and `=` stay out of the query.
fn base64url(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let bits = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);
        out.push(ALPHABET[(bits >> 18) as usize & 63] as char);
        out.push(ALPHABET[(bits >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(bits >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[bits as usize & 63] as char);
        }
    }
    out
}

fn failure(request: Request, status: u16, kind: &'static str, message: &str) {
    send(request, status, &json!({ "error": { "kind": kind, "message": message } }));
}

/// Reads the small JSON body the page sends, or says it cannot.
fn read_body<T: DeserializeOwned>(request: &mut Request) -> Option<T> {
    let mut raw = Vec::new();
    request.as_reader().take(MAX_BODY).read_to_end(&mut raw).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn send<T: Serialize>(request: Request, status: u16, body: &T) {
    match serde_json::to_string(body) {
        Ok(body) => respond(request, status, "application/json", body, Vec::new()),
        Err(error) => {
            eprintln!("cannot write an answer: {error}");
            respond(
                request,
                500,
                "application/json",
                "{\"error\":{\"kind\":\"unknown\",\"message\":\"Cannot write an answer\"}}".to_string(),
                Vec::new(),
            )
        }
    }
}

fn redirect(request: Request, location: &str) {
    respond(request, 303, "text/plain; charset=utf-8", String::new(), vec![("Location", location)])
}

fn respond(request: Request, status: u16, content_type: &str, body: String, extra: Vec<(&str, &str)>) {
    let url = request.url().to_string();
    let mut response = Response::from_string(body).with_status_code(StatusCode(status));
    if let Some(header) = header("Content-Type", content_type) {
        response.add_header(header);
    }
    for (name, value) in extra {
        if let Some(header) = header(name, value) {
            response.add_header(header);
        }
    }
    if let Err(error) = request.respond(response) {
        eprintln!("cannot answer {url}: {error}");
    }
}

fn header(name: &str, value: &str) -> Option<Header> {
    match Header::from_bytes(name.as_bytes(), value.as_bytes()) {
        Ok(header) => Some(header),
        Err(()) => {
            eprintln!("cannot set the header {name}");
            None
        }
    }
}

fn authorized(headers: &[Header], token: &str) -> bool {
    headers.iter().filter(|header| header.field.equiv("Cookie")).any(|header| cookie_has(header.value.as_str(), token))
}

fn cookie_has(cookie: &str, token: &str) -> bool {
    cookie.split(';').any(|pair| pair.trim().strip_prefix("session=").is_some_and(|value| value == token))
}

fn carries_token(query: &str, token: &str) -> bool {
    query.split('&').any(|pair| pair.strip_prefix("token=").is_some_and(|value| value == token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn account() -> Account {
        Account {
            id: "cy9lebijpuwr".into(),
            name: "Living room".into(),
            url: "https://music.example.com".into(),
            username: "alice".into(),
            password: "secret pass".into(),
            created_at: 0,
            last_used_at: 0,
        }
    }

    fn decode(data: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut bits = 0u32;
        let mut held = 0;
        for byte in data.bytes() {
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'-' => 62,
                b'_' => 63,
                other => panic!("{other} is not base64url"),
            };
            bits = bits << 6 | u32::from(value);
            held += 6;
            if held >= 8 {
                held -= 8;
                out.push((bits >> held) as u8);
                bits &= (1 << held) - 1;
            }
        }
        out
    }

    #[test]
    fn cookie_must_match_exactly() {
        let header = |value: &str| Header::from_bytes(&b"Cookie"[..], value.as_bytes()).expect("header");
        assert!(cookie_has("session=abc123; other=1", "abc123"));
        assert!(cookie_has("other=1; session=abc123", "abc123"));
        assert!(!cookie_has("session=abc12", "abc123"));
        assert!(!cookie_has("session=abc1234", "abc123"));
        assert!(!cookie_has("other=abc123", "abc123"));
        assert!(!cookie_has("", "abc123"));
        assert!(authorized(&[header("session=abc123")], "abc123"));
        assert!(!authorized(&[header("session=nope")], "abc123"));
        assert!(!authorized(&[], "abc123"));
    }

    #[test]
    fn token_reaches_the_page_once() {
        assert!(carries_token("token=abc123", "abc123"));
        assert!(carries_token("lang=zh&token=abc123", "abc123"));
        assert!(!carries_token("token=abc124", "abc123"));
        assert!(!carries_token("", "abc123"));
    }

    #[test]
    fn base64url_matches_the_standard_encoding() {
        // Compared byte for byte with `URL_SAFE_NO_PAD`, the encoding the
        // plugin already reads.
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
    }

    #[test]
    fn the_callback_carries_the_account_as_base64_json() {
        let url = redirect_url("http://127.0.0.1:41000/navidrome", &account());
        let data = url.strip_prefix("http://127.0.0.1:41000/navidrome?data=").expect("a query");
        let value: Value = serde_json::from_slice(&decode(data)).expect("json");
        assert_eq!(value["id"], "cy9lebijpuwr");
        assert_eq!(value["name"], "Living room");
        assert_eq!(value["url"], "https://music.example.com");
        assert_eq!(value["username"], "alice");
        assert_eq!(value["password"], "secret pass");
    }

    #[test]
    fn the_card_cannot_bring_markup_with_it() {
        assert_eq!(escape_html(r#"<b>&"x"</b>"#), "&lt;b&gt;&amp;&quot;x&quot;&lt;/b&gt;");
        assert_eq!(escape_html("it's"), "it&#39;s");
    }

    #[test]
    fn an_existing_query_is_kept() {
        let url = redirect_url("http://127.0.0.1:41000/navidrome?x=1", &account());
        assert!(url.contains("?x=1&data="));
    }

    #[test]
    fn an_unnamed_server_is_labelled_by_its_url() {
        let mut account = account();
        account.name = "  ".into();
        let url = redirect_url("http://127.0.0.1:41000/navidrome", &account);
        let data = url.split("data=").nth(1).expect("data");
        let value: Value = serde_json::from_slice(&decode(data)).expect("json");
        assert_eq!(value["name"], "https://music.example.com");
    }
}
