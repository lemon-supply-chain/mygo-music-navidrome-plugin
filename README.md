# Navidrome for MyGo Music

A [MyGo Music](https://github.com/egoist/mygo-music) plugin that plays a
[Navidrome](https://www.navidrome.org) library through its Subsonic API.

It needs MyGo Music 0.1.20 or later, and speaks English or Simplified Chinese
as the app does. Your library's titles and names keep their original language.

## Install

Open MyGo Music → Settings → Plugins → `+` → from GitHub, and enter this
repository. Or open:

```
mygo-music://install-plugin?repo=OWNER/REPO
```

Then sign in: a local page opens in your browser where you add a server, with
its name, address, username and password. Add as many servers as you like, and
switch between the saved ones on that same page.

## What it does

- Home with a For you mix and recently added, recently played, most played and
  starred albums
- Artists, genres, album and artist pages, search and suggestions
- Liked songs and starred albums
- Playlists, including creating, renaming, deleting, adding, removing and
  reordering tracks
- Playback through Navidrome's `stream` endpoint: plays the original file
  directly for formats MyGo decodes, and transcodes other formats on this Mac
  with ffmpeg
- Lyrics from Better Lyrics, with word timing, falling back to Navidrome's own
  synchronized lyrics
- Endless radio that shuffles your library, skipping songs you've heard recently
- Internet radio stations added in Navidrome

Not supported: podcasts, comments and following artists, which Navidrome's
Subsonic API doesn't expose.

Playback plays native audio formats directly; formats MyGo can't decode need
ffmpeg on this Mac:

```bash
brew install ffmpeg
```

The plugin asks for permission to start it, and only starts it from the PATH,
`/opt/homebrew/bin`, `/usr/local/bin` or `/opt/local/bin`. If it's somewhere
else, link it into one of those. Formats MyGo decodes directly without ffmpeg:
mp3, flac, ogg, wav, m4a and aac.

If playback stops immediately with EOF after an earlier failed attempt, clear
the app's audio cache in Settings. MyGo keeps whatever the server returned as
the song, including an XML error, so a stale failed response keeps ending in
EOF until it is removed.

## Signing in

The plugin starts a small helper binary, `bin/navidrome-login-<platform>`,
which serves one local page on `127.0.0.1` and prints its URL, plus a free
callback port, on the first line of stdout. The plugin opens that URL in your
browser and listens on that port before the page opens. The page signs in with a
server name (optional), URL, username and password, keeps every saved server in
`auth.json` in the plugin's data directory, and lists them so a click switches
to one; each can be edited or removed from that list. Its Done button shows a
card naming the server in use, tries to close the tab after a second, and hands
that server back to the plugin through a callback the plugin is waiting on;
the plugin then closes the helper. The helper has no window of its own.

The helper is written in Rust, in `native/navidrome-login`; `bin/` holds the
built binaries. Password auth stays local: the plugin signs Subsonic requests
with `md5(password + salt)` exactly as the page's own check does.

The plugin asks for network access to any host because the server is chosen at
runtime, and for the helper and ffmpeg in its process permission. Cover art and
audio URLs use a fixed token salt so they can be loaded by the app directly.

## Development

```bash
pnpm install
pnpm helper   # builds bin/navidrome-login-<platform> from native/navidrome-login
pnpm check    # typecheck, lint, format, tests, package
```

`pnpm helper` builds for the machine it runs on. In CI, GitHub Actions builds
the native helpers for Windows and macOS and packages them into four platform
release archives. The plugin test runs against a mock Subsonic server, and a mocked
sign-in helper that answers the callback as the page's Done button would, and
checks the mapping.

## Publishing

`plugin.json` must be at the top of a public GitHub repository. The app takes
the latest release zip for its platform (for example `plugin-darwin-arm64.zip`),
or the source. Zips are made per platform from the binaries in `bin/`.
Add the `mygo-music-plugin` topic to have it listed on the plugins page.

## References

- [MyGo Music plugin API](doc/mygo-plugin-api.md)
- [Navidrome / Subsonic API used here](doc/navidrome-api.md)
