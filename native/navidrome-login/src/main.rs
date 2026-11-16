// A local sign-in page for Navidrome. The plugin starts this binary, reads the
// page URL from its first stdout line, and opens it in a browser. The page's
// Done button answers the plugin's callback with the account it signed in to.

mod navidrome;
mod output;
mod random;
mod store;
mod web;

use std::net::TcpListener;
use std::path::PathBuf;

/// Where the helper sends the account once Done is clicked. The plugin
/// listens there, so the helper only finds the port for it.
const CALLBACK_PATH: &str = "/navidrome";
use std::sync::Arc;

use web::Web;

const HELP: &str = "\
usage: navidrome-login [--dir <path>] [--port <number>] [--open]

  --dir <path>   where auth.json is kept (default: next to the binary)
  --port <n>     port to listen on; 0 picks a free one (default: 0)
  --open         open the page in the default browser as well
";

struct Args {
    dir: Option<PathBuf>,
    port: u16,
    open: bool,
}

enum ParseError {
    Help,
    Message(String),
}

fn main() {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(ParseError::Help) => {
            println!("{HELP}");
            return;
        }
        Err(ParseError::Message(message)) => {
            eprintln!("navidrome-login: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    if let Err(message) = serve(args) {
        output::error(&message);
        eprintln!("navidrome-login: {message}");
        std::process::exit(1);
    }
}

fn parse(mut rest: impl Iterator<Item = String>) -> Result<Args, ParseError> {
    let mut args = Args { dir: None, port: 0, open: false };
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--dir" | "--auth-dir" => {
                let value = rest.next().ok_or_else(|| ParseError::Message("--dir needs a path".into()))?;
                args.dir = Some(PathBuf::from(value));
            }
            "--port" => {
                let value = rest.next().ok_or_else(|| ParseError::Message("--port needs a number".into()))?;
                args.port =
                    value.parse().map_err(|_| ParseError::Message(format!("--port needs a number, got {value}")))?;
            }
            "--open" => args.open = true,
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::Message(format!("unknown flag {other}"))),
        }
    }
    Ok(args)
}

struct Page {
    web: Arc<Web>,
    listener: TcpListener,
    url: String,
}

/// Binds the page's port and says where it is, before anything opens it.
fn page(args: &Args) -> Result<Page, String> {
    let dir = args.dir.clone().unwrap_or_else(store::default_dir);
    let path = dir.join("auth.json");
    let token = random_token();

    let listener = TcpListener::bind(("127.0.0.1", args.port))
        .map_err(|error| format!("cannot listen on 127.0.0.1:{}: {error}", args.port))?;
    let address = listener.local_addr().map_err(|error| format!("cannot read the listening address: {error}"))?;

    // A free port for the plugin to listen on, found here so no one has to
    // guess: the plugin binds it before the browser can click Done.
    let callback = format!("http://127.0.0.1:{}{CALLBACK_PATH}", free_port()?);
    let web = Arc::new(Web::new(token.clone(), path, callback.clone()));

    let url = format!("http://{address}/?token={token}");
    output::ready(&url, &callback);
    Ok(Page { web, listener, url })
}

/// Serves the page until the plugin stops the process or the user interrupts
/// it. Done does not end it: the browser still has to follow the callback.
fn serve(args: Args) -> Result<(), String> {
    let page = page(&args)?;
    if args.open {
        open_browser(&page.url);
    }

    // The port is already bound and named by `ready`, so the server only has
    // to take the listener over. It runs until the plugin stops the process.
    let server = tiny_http::Server::from_listener(page.listener, None)
        .map_err(|error| format!("cannot serve the page at {}: {error}", page.url))?;
    web::serve(&server, page.web);
    Ok(())
}

fn free_port() -> Result<u16, String> {
    let probe = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| format!("cannot find a free port: {error}"))?;
    let port = probe.local_addr().map_err(|error| format!("cannot read the free port: {error}"))?.port();
    drop(probe);
    Ok(port)
}

fn random_token() -> String {
    random::pick(b"abcdefghijklmnopqrstuvwxyz0123456789", 32)
}

fn open_browser(url: &str) {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(error) = std::process::Command::new(command).arg(url).spawn() {
        eprintln!("cannot open a browser: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(values: &[&str]) -> Result<Args, ParseError> {
        parse(values.iter().map(|value| value.to_string()))
    }

    #[test]
    fn defaults_listen_on_a_free_port() {
        let args = flags(&[]).ok().expect("parsed");
        assert_eq!(args.port, 0);
        assert!(args.dir.is_none());
        assert!(!args.open);
    }

    #[test]
    fn flags_are_read() {
        let args = flags(&["--dir", "/tmp/x", "--port", "8080", "--open"]).ok().expect("parsed");
        assert_eq!(args.dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(args.port, 8080);
        assert!(args.open);
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(matches!(flags(&["--dir"]), Err(ParseError::Message(_))));
        assert!(matches!(flags(&["--port"]), Err(ParseError::Message(_))));
        assert!(matches!(flags(&["--port", "eighty"]), Err(ParseError::Message(_))));
        assert!(matches!(flags(&["--nope"]), Err(ParseError::Message(_))));
        assert!(matches!(flags(&["--help"]), Err(ParseError::Help)));
    }
}
