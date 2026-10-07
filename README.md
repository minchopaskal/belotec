# Belote Club

A small, playable Bulgarian belote game for four friends. The server, game rules,
network protocol, and browser client are written in Rust. The browser client
compiles to WebAssembly; the only handwritten JavaScript loads that module.

## Run

Install [Rust](https://rustup.rs/) (current stable; tested with Rust 1.98.1), then run from this folder:

```sh
./scripts/build.sh
./scripts/run.sh
```

Open **http://localhost:3000**. The first build installs the WebAssembly target
and a local, version-matched `wasm-bindgen` tool in `.tools/`. It can take several
minutes. Subsequent builds reuse Cargo's cache. No Node.js or npm is needed.

1. Select **New game**, enter your name, and **Create a table**.
2. Share the six-character code or invitation link with friends.
3. Partners are in the same team column. Select **Sit here** to change teams.
4. Fill four seats with friends or optional practice bots. The host selects
   **Deal the cards**.
5. Bid, then play highlighted cards. The host starts each subsequent hand.

**Friends on the same Wi-Fi:** use `http://YOUR-COMPUTERS-LAN-IP:3000` on every
device, including the host. A `localhost` invitation only works on your computer.
Allow incoming traffic on port 3000 in the host's firewall if needed. Other
players need only a modern browser. The page also adapts to phone screens.

**Friends over the internet:** run one server on a reachable host and put it
behind HTTPS with WebSocket support. A reverse proxy such as Caddy can handle
TLS. All friends must open that same public address. This project is ready to
self-host; it has not been deployed to a public service.

```caddyfile
belote.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

### Configuration

| Variable | Default | Purpose |
|---|---|---|
| `BELOTE_BIND` | `0.0.0.0:3000` | Listening address and port |
| `BELOTE_WEB_DIR` | Workspace `web/` | Static client files; set when moving the binary |
| `RUST_LOG` | `belote_server=info,tower_http=info` | Logging filter |

Example for a different port:

```sh
BELOTE_BIND=0.0.0.0:8080 ./scripts/run.sh
```

Or build and run with Docker (optional; Docker build not exercised in this workspace):

```sh
docker build -t belote-club .
docker run --rm -p 3000:3000 belote-club
```

## Included

- Main menu, create/join lobby, private invite codes and links, selectable seats.
- Four players in two partnerships, full 32-card deck, counterclockwise play.
- Five-card bidding, then three more cards; clubs, diamonds, hearts, spades,
  no trumps, all trumps, double, redouble, and redealing after four passes.
- Server-enforced turns, suit following, raising, cutting, and overtrumping.
- Sequences and four-of-a-kind, with competing declarations compared by team.
  The UI preselects the highest-point compatible combination; players may change
  it before playing their first card. Overlapping choices are mutually exclusive.
- Automatic belote on the first eligible king/queen, last ten, capot, contract
  success/failure, tied points carried forward, scoring to 151, and rematches.
- Scorebook, last-trick view, turn sounds with saved on/off preference.
- One classic green felt theme; downloaded CC0 cards and fabric bundled locally.
- Simple optional bots; host can replace a disconnected player with a bot.
- Saved seats on refresh and automatic reconnect. Tokens live in per-tab session
  storage so several separate tabs can represent different players for testing.

House-rule decisions are in [docs/RULES.md](docs/RULES.md). Assets and their
licenses are in [web/assets/CREDITS.md](web/assets/CREDITS.md).

## Structure and mobile path

```text
crates/belote-core/     Pure Rust rules and shared JSON types; no UI or networking
crates/belote-server/   Tokio + Axum HTTP/WebSocket server; owns all private hands
crates/belote-client/   Yew Rust/WASM browser UI
web/                   HTML loader, responsive CSS, and bundled artwork
scripts/               Build and run entry points
```

The server shuffles and validates every move. Each connection receives only its
own hand, public plays, and other players' card counts. No game rules depend on
browser storage or client claims.

For Android and iOS, the quickest next step is packaging this responsive client
in a mobile WebView (for example a Tauri mobile shell). A native UI can instead
reuse `belote-core` and the same WebSocket protocol. App-store projects, signing,
native lifecycle handling, and device testing are future work, not included yet.

## Development and verification

```sh
cargo test                         # Rules + real four-client WebSocket tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p belote-client --target wasm32-unknown-unknown -- -D warnings
./scripts/build.sh                 # Rebuild WASM and the release server
```

The integration tests listen on temporary loopback ports. Sandboxed runners must
allow local networking. The engine suite includes 480 seeded complete hands
across all six contracts, a complete match, and focused bidding, declaration,
scoring, illegal-move and privacy tests. Integration tests cover four distinct
WebSockets, reconnecting with the same hand, room limits, host permissions,
rejected origins, and consistent scoring.

### First-version limits

Rooms are in memory: restarting the server ends active games. Empty/disconnected
rooms expire after 30 minutes. This is a single-server app; there is no account
system, database, match history across restarts, public matchmaking, spectator
mode, or chat. Bots know only their own cards and the public trick and are
intentionally basic. A disconnected human is not automatically replaced: the
host can wait for them or choose **Replace with bot**.

The server caps rooms, connections, message size, and per-connection input rate.
Use HTTPS for internet play; invite codes identify rooms, and private random
tokens authorize resuming a seat. Names and sound preferences stay in browser
storage. There are no analytics or third-party requests during play.
