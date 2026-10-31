# Contributing

Thanks for helping. Bug reports, fixes and small focused features are all welcome.

## Build and run

```sh
cargo run                         # connection screen
cargo run -- ~/data/app.db        # open a SQLite file
```

On Linux, saving passwords needs a Secret Service keyring (GNOME Keyring, KWallet). Without one, sqlsea still works but asks for passwords each time.

## Before sending a change

```sh
cargo fmt
cargo clippy --all-targets
cargo test
```

The TLS tests need live PostgreSQL servers, so they are skipped by default. To run them, start one server with TLS and one without, then:

```sh
SQLSEA_TLS_PORT=55433 SQLSEA_TLS_CA=path/to/ca.crt SQLSEA_PLAIN_PORT=55432 \
  cargo test -- --include-ignored
```

The TLS server's certificate must be signed by that CA and valid for `localhost` and `127.0.0.1`; the password is `sqlsea` for the `postgres` user.

## Where things are

| Path | What it holds |
| --- | --- |
| `src/main.rs` | The app: state, messages and views |
| `src/db.rs` | PostgreSQL and SQLite access, row edits |
| `src/tls.rs` | TLS settings for PostgreSQL |
| `src/store.rs` | Saved connections, settings, keyring |
| `src/grid.rs` | The result grid |
| `src/query.rs` | The Query tab: query tabs, running and cancelling, history |
| `src/export.rs` | CSV, JSON and clipboard output |
| `src/sql.rs` | SQL syntax highlighting |
| `src/chrome.rs` | The Linux title bar controls and resize edges |
| `src/theme.rs` | Every colour, font and size, light and dark |
| `brand/` | Icons and logos; see `brand/BRAND.md` |
| `packaging/` | Release packages, one folder per platform: `linux/`, `macos/`, `windows/` |

Views never name a colour directly; add it to both palettes in `src/theme.rs` and use it through a style function.

## Screenshots

`cargo run --features shot` adds a scripted mode that drives the app and saves screenshots, which helps when changing the interface. The README lists the steps it understands.

## Licence

By contributing, you agree that your contributions are dual licensed under MIT OR Apache-2.0, as described in the README.
