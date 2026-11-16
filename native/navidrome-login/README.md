# navidrome-login

A single Rust binary that serves one local sign-in page for Navidrome servers,
keeps what it learns in `auth.json`, and hands the server in use to the MyGo
Music plugin through a callback. It has no window of its own: the plugin reads
the page URL from its first line of stdout and opens it in a browser.

The page does three things:

- lists every saved server, switches to any of them with one click, and lets
  each one be edited or removed
- signs in through an **Add server** dialog: server name (optional), URL,
  username and password, checked through the Subsonic API before saving
- **Edit** reopens the same dialog with a saved server, password blank to keep
  the stored one, and checks the new credentials before writing them
- **Done** shows a card naming the server in use, tries to close the tab after
  a second, and answers the plugin's callback with that server as base64 JSON
  in the query; the plugin closes the helper after that

The dialog stays out of the way unless it is asked for. It only opens by itself
when nothing is saved yet, because then there is nothing to pick.

Adding a server that is already saved replaces its password instead of
adding a second row. Leaving the name empty when signing in again keeps the
name already stored.

## Run

```bash
cargo build --release
./target/release/navidrome-login --open
```

The first line of stdout is the page URL. `--open` opens it in the default
browser; the plugin does that itself with `host.openURL`.

The plugin ships this binary as `bin/navidrome-login-<goos>-<goarch>`, starts
it with `--dir <plugin data dir> --port 0`, listens on the callback port from
the `ready` line, and opens the page URL from the same line. The manifest asks
for `bin/navidrome-login-*` in `permissions.process`.

| Flag           | Meaning                                                             |
| -------------- | ------------------------------------------------------------------- |
| `--dir <path>` | where `auth.json` lives; default is the directory of the executable |
| `--port <n>`   | port to listen on; `0` picks a free one, which is the default       |
| `--open`       | open the page in the default browser as well                        |

The server only binds `127.0.0.1`. The page URL carries a random token once;
it becomes an `HttpOnly` cookie, and everything else without it is refused.
Adding `?lang=zh` to the URL switches the page to Chinese.

## Footprint

The helper ships inside every plugin archive, so it is kept small on purpose.

- `tiny_http` answers each request on its own thread and `ureq` makes the
  blocking Subsonic call: no tokio, hyper, axum or reqwest
- `getrandom` draws the token, the ids and the salts, `md-5` derives the
  Subsonic token, and the base64url of the callback is a few local lines
- `[profile.release]` strips, LTOs and optimises for size: about 1.4 MB on
  arm64 and 1.5 MB on amd64, down from 5.1 MB

Because `panic = "abort"` is set, a panic ends the process instead of one
request; the store's lock is read through a poisoned guard anyway.

## Output

One JSON object per line on stdout:

```json
{ "type": "ready", "url": "http://127.0.0.1:52341/?token=...", "callback": "http://127.0.0.1:52342/navidrome" }
```

`ready` comes first and always. The callback port is one the helper found free
before printing, so the plugin has a port to listen on without guessing.
`error` says why the page could not be served. There is no `account` line: the
account travels to the callback instead.

## Done

Done opens `/done` on the helper, which serves one card from
`assets/done.html`: a checkmark, the server in use, and a line saying it is
returning to MyGo Music. The card tries `window.close()` after a second, and
says the page can be closed when the browser refuses — a tab opened by another
app can't be closed by script.

While the card is served, the helper calls the callback it named in `ready`
itself:

```
GET http://127.0.0.1:41000/navidrome?data=<base64url JSON>
```

The JSON is
`{"id": "cy9lebijpuwr", "name": "Living room", "url": "https://music.example.com", "username": "alice", "password": "secret pass"}`,
where `name` is the server's name, or its URL when it has none. `base64url`
without padding keeps `+`, `/` and `=` out of the query. The call waits 300ms
so the card is answered first: the plugin closes the helper as soon as it has
the account, and must not cut the page short.

## auth.json

Kept in the directory of the executable unless `--dir` says otherwise. Written
atomically and with `0600` permissions on Unix.

```json
{
    "version": 1,
    "accounts": [
        {
            "id": "cy9lebijpuwr",
            "name": "Living room",
            "url": "https://music.example.com",
            "username": "alice",
            "password": "secret pass",
            "created_at": 1794764993,
            "last_used_at": 1794765015
        }
    ],
    "current": "cy9lebijpuwr"
}
```

Passwords are stored as typed, because the Subsonic token is
`md5(password + salt)` and the salt changes per request. The file never leaves
this device; the page itself never receives a password back.

## Development

```bash
cargo test          # unit tests
cargo clippy --all-targets
./tests/e2e.sh      # drives the binary against two fake Navidrome servers
```

`tests/e2e.sh` needs `python3`. It starts `tests/fake-navidrome.py`, runs the
binary, signs in twice, switches, edits, removes, clicks Done, and checks the
callback payload and `auth.json`.
