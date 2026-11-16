// One JSON object per line on stdout. The plugin reads these lines: the page
// URL first, and an error if the page can't be served at all.

use serde::Serialize;
use std::io::Write;

#[derive(Serialize)]
struct Ready<'a> {
    r#type: &'a str,
    url: &'a str,
    /// Where the helper sends the account once Done is clicked. The port is
    /// free: the plugin listens on it before anything can answer.
    callback: &'a str,
}

#[derive(Serialize)]
struct Failed<'a> {
    r#type: &'a str,
    message: &'a str,
}

fn line<T: Serialize>(value: &T) {
    match serde_json::to_string(value) {
        Ok(json) => {
            println!("{json}");
            let _ = std::io::stdout().flush();
        }
        Err(error) => eprintln!("cannot write a line of output: {error}"),
    }
}

pub fn ready(url: &str, callback: &str) {
    line(&Ready { r#type: "ready", url, callback });
}

pub fn error(message: &str) {
    line(&Failed { r#type: "error", message });
}
