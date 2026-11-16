# Shrinking the `navidrome-login` Rust helper

The sign-in helper (`native/navidrome-login`) ships inside every plugin
archive, so its binary size is paid for twice per platform. This note records
where the original 5.1 MB came from and what brought it down to 1.4 MB.

## Where the size was

`cargo bloat --crates` on the 5.4 MB unstripped build, by `.text`:

| Crate                                   | Share | Why it was there                       |
| --------------------------------------- | ----- | -------------------------------------- |
| `std`                                   | 21%   | unavoidable                            |
| `rustls`                                | 17%   | HTTPS to the Subsonic server           |
| `reqwest`                               | 15%   | one GET per sign-in check              |
| `tokio`                                 | 7%    | only a runtime, a sleep and `ctrl_c`   |
| `ring`                                  | 6%    | rustls provider                        |
| `hyper`                                 | 6%    | HTTP/1.1 for the client and the server |
| `axum`                                  | 4%    | seven local routes                     |
| `webpki`                                | 3%    | certificate validation                 |
| `url` / `idna` / `icu_*`                | 3%    | pulled in by `reqwest`                 |
| `hyper-util`, `http`, `tower*`, `bytes` | rest  | plumbing                               |

148 crates in the tree, 9 direct dependencies.

## What replaced them

| Was                        | Now              | Note                                                                                                     |
| -------------------------- | ---------------- | -------------------------------------------------------------------------------------------------------- |
| `axum` + `hyper` + `tokio` | `tiny_http` 0.12 | blocking, one thread per connection, keep-alive and chunked handled                                      |
| `reqwest` (rustls)         | `ureq` 3.5       | blocking, `default-features = false, features = ["rustls"]` keeps out gzip, charset and the cookie store |
| `url` / `icu_*`            | none             | `ureq` builds its own query strings and paths                                                            |
| `rand`                     | `getrandom`      | 44 bytes of code instead of a ChaCha RNG                                                                 |
| `hex`                      | 4 local lines    |                                                                                                          |
| `base64` 0.22              | 20 local lines   | base64url without padding, unit-tested against the standard vectors                                      |

HTTP server and client are both synchronous now, which fits a page one person
opens on loopback. Every request is answered on its own thread, so a slow
Subsonic check cannot hold up the stylesheet (measured: 0.04 s for `/app.css`
while a sign-in hung for 6 s).

## Release profile

```toml
[profile.release]
codegen-units = 1
lto = true
opt-level = "z"
panic = "abort"
strip = true
```

`opt-level` was measured, not guessed, on `aarch64-apple-darwin`:

| `opt-level` | Binary  |
| ----------- | ------- |
| `"z"`       | 1.40 MB |
| `"s"`       | 1.55 MB |
| `3`         | 1.88 MB |

With the old dependency tree the ordering was reversed (`"z"` was larger than
`"s"`); the numbers above are the ones that matter for the final binary, so
re-measure if the tree changes much.

`panic = "abort"` means a panic ends the process instead of one request, which
is why the store is read through `Mutex::lock().unwrap_or_else(PoisonError::into_inner)`
rather than `expect`.

## Result

| Artifact                       | Before     | After     |
| ------------------------------ | ---------- | --------- |
| dependency tree                | 148 crates | 44 crates |
| direct dependencies            | 9          | 6         |
| `navidrome-login-darwin-arm64` | 5.1 MB     | 1.40 MB   |
| `navidrome-login-darwin-amd64` | 5.2 MB     | 1.59 MB   |
| `plugin-darwin-arm64.zip`      | 2.0 MB     | 866 KB    |
| `plugin-darwin-amd64.zip`      | 2.1 MB     | 921 KB    |

## What was left alone

- **rustls + ring + webpki-roots** stay: the helper must reach HTTPS servers
  that Navidrome users run, and the bundled root store keeps trust identical on
  every machine. `rustls-native-certs` or `platform-verifier` would trade
  ~250 KB of root data for platform-specific trust and a new failure mode
  (Linux boxes without `ca-certificates`).
- **`md-5`** stays: Subsonic tokens are `md5(password + salt)`, and hand-rolling
  a hash to save a few kilobytes is not a trade worth making.
- **The assets** stay inline through `include_str!`: 36 KB, one fewer thing to
  install.
