// Subsonic API checks. Navidrome authenticates with username plus password only,
// so the page asks for nothing else and the token is derived here.

use std::time::Duration;

use md5::{Digest, Md5};
use serde::Serialize;

use crate::random;

/// Long enough for a slow server on a slow network, short enough that the page
/// gives up before the person in front of it does.
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Serialize)]
pub struct LoginError {
    /// `network`, `credentials` or `server`, translated by the page.
    pub kind: &'static str,
    pub message: String,
}

impl LoginError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

/// One agent for both the Subsonic check and the plugin's callback. It is
/// blocking: the page serves one person, so a runtime would be dead weight.
pub fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        // The page reports the status itself, so 4xx has to reach `check_body`.
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .max_redirects(5)
        // A proxy picked up from the environment would swallow the loopback
        // callback the plugin is waiting on.
        .proxy(None)
        .build()
        .into()
}

pub fn normalize_url(raw: &str) -> String {
    let mut url = raw.trim().trim_end_matches('/').to_string();
    for suffix in ["/rest", "/rest/"] {
        if let Some(stripped) = url.strip_suffix(suffix) {
            url = stripped.trim_end_matches('/').to_string();
            break;
        }
    }
    if !url.to_ascii_lowercase().starts_with("http://") && !url.to_ascii_lowercase().starts_with("https://") {
        url = format!("http://{url}");
    }
    url
}

pub fn salt() -> String {
    random::pick(b"abcdefghijklmnopqrstuvwxyz0123456789", 10)
}

pub fn token(password: &str, salt: &str) -> String {
    let mut digest = Md5::new();
    digest.update(password.as_bytes());
    digest.update(salt.as_bytes());
    hex(&digest.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

pub fn ping(client: &ureq::Agent, url: &str, username: &str, password: &str) -> Result<(), LoginError> {
    let salt = salt();
    let token = token(password, &salt);

    let mut response = client
        .get(&format!("{url}/rest/ping.view"))
        // ureq percent-encodes both halves, so a username with spaces or an `&`
        // still reads as one field on the server.
        .query("u", username)
        .query("t", &token)
        .query("s", &salt)
        .query("v", "1.16.1")
        .query("c", "mygo-music")
        .query("f", "json")
        .call()
        .map_err(|error| LoginError::new("network", error.to_string()))?;

    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    check_body(status, &body)
}

/// Answers the plugin's callback. The status is the plugin's business.
pub fn notify(client: &ureq::Agent, url: &str) -> Result<(), String> {
    client.get(url).call().map(|_| ()).map_err(|error| error.to_string())
}

fn check_body(status: u16, body: &str) -> Result<(), LoginError> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let response = &json["subsonic-response"];
        match response["status"].as_str() {
            Some("ok") => return Ok(()),
            Some("failed") => {
                let code = response["error"]["code"].as_u64().unwrap_or(0);
                let message = response["error"]["message"].as_str().unwrap_or("").trim().to_string();
                return Err(if code == 40 {
                    LoginError::new("credentials", message)
                } else {
                    LoginError::new("server", message)
                });
            }
            _ => {}
        }
    }

    // Older servers can ignore `f=json` and answer in XML.
    if let Some(element) = body.split("<subsonic-response").nth(1) {
        let head = element.split('>').next().unwrap_or("");
        if head.contains("status=\"ok\"") {
            return Ok(());
        }
        if head.contains("status=\"failed\"") {
            let code = attribute(element, "code");
            let message = attribute(element, "message");
            return Err(if code == Some("40".to_string()) {
                LoginError::new("credentials", message.unwrap_or_default())
            } else {
                LoginError::new("server", message.unwrap_or_default())
            });
        }
    }

    let message = if status == 401 || status == 403 {
        "the server refused the request".to_string()
    } else if status >= 400 {
        format!("the server answered with HTTP {status}")
    } else {
        "the answer is not a Subsonic response".to_string()
    };
    Err(LoginError::new("server", message))
}

fn attribute(element: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = element.find(&pattern)? + pattern.len();
    let rest = &element[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_gets_a_scheme_and_no_trailing_slash() {
        assert_eq!(normalize_url("music.example.com/"), "http://music.example.com");
        assert_eq!(normalize_url("https://music.example.com/rest/"), "https://music.example.com");
        assert_eq!(normalize_url(" HTTPS://music.example.com/ "), "HTTPS://music.example.com");
    }

    #[test]
    fn token_matches_the_subsonic_rule() {
        assert_eq!(token("sesame", "c19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn hex_pads_every_byte() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    }

    #[test]
    fn salts_differ_and_are_short() {
        assert_eq!(salt().len(), 10);
        assert_ne!(salt(), salt());
    }

    #[test]
    fn ok_json_passes() {
        let body = r#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;
        assert!(check_body(200, body).is_ok());
    }

    #[test]
    fn wrong_password_is_a_credentials_error() {
        let body =
            r#"{"subsonic-response":{"status":"failed","error":{"code":40,"message":"Wrong username or password"}}}"#;
        assert_eq!(check_body(200, body).unwrap_err().kind, "credentials");
    }

    #[test]
    fn xml_answers_are_understood() {
        assert!(check_body(200, r#"<subsonic-response status="ok" version="1.16.1"/>"#).is_ok());
        assert_eq!(
            check_body(
                200,
                r#"<subsonic-response status="failed"><error code="40" message="Wrong username or password"/></subsonic-response>"#
            )
            .unwrap_err()
            .kind,
            "credentials"
        );
    }

    #[test]
    fn html_is_a_server_error() {
        assert_eq!(check_body(502, "<html>bad gateway</html>").unwrap_err().kind, "server");
    }
}
