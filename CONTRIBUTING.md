# Contributing

You can send bug reports, fixes and small features.

## Build and run

```sh
cargo run                         # the connection screen
cargo run -- ~/data/app.db        # open a SQLite file
```

On Linux, sqlsea needs a Secret Service keyring (GNOME Keyring or KWallet) to save passwords. Without a keyring, sqlsea asks for the password each time.

## Before you send a change

Run these commands. Fix all errors and warnings.

```sh
cargo fmt
cargo clippy --all-targets
cargo test
```

The TLS tests need two PostgreSQL servers, so `cargo test` skips them. To run them, follow these steps:

1. Start one PostgreSQL server with TLS and one without TLS. The password of the `postgres` user must be `sqlsea` on both.
2. Make sure that a CA signed the certificate of the TLS server, and that the certificate is for `localhost` and `127.0.0.1`.
3. Run this command with your ports and the path to the CA file:

```sh
SQLSEA_TLS_PORT=55433 SQLSEA_TLS_CA=path/to/ca.crt SQLSEA_PLAIN_PORT=55432 \
  cargo test -- --include-ignored
```

The SSH test needs an SSH server that can reach a PostgreSQL server with the host name `sqlsea-pgdb`. To run it, follow these steps:

1. Put a private key `id` without a passphrase and a private key `id_pass` with the passphrase `open sesame` in a folder.
2. Start PostgreSQL in a Docker network with the name `sqlsea-pgdb` and the password `sqlsea`.
3. Start an SSH server in the same network. It must accept both keys and the password `tunnel-pw` for the user `tunnel`, and allow TCP forwarding.
4. Write the host key of the SSH server to `known_hosts` in the key folder with `ssh-keyscan`.
5. Run this command with your port and folder. If `SSH_AUTH_SOCK` is set, the test also uses the SSH agent.

```sh
SQLSEA_SSH_PORT=2222 SQLSEA_SSH_DIR=path/to/keys cargo test -- --ignored ssh_tunnel
```

## Where the code is

| Path | What it holds |
| --- | --- |
| `src/main.rs` | The app: state, messages and views |
| `src/db.rs` | PostgreSQL and SQLite access, and row edits |
| `src/tls.rs` | TLS for PostgreSQL |
| `src/store.rs` | Saved connections, settings, the keyring and the history |
| `src/query.rs` | The Query tab: query tabs, runs, cancel and history |
| `src/grid.rs` | The result grid |
| `src/fit.rs` | One line of text that ends in "…" if it does not fit |
| `src/export.rs` | CSV, JSON and clipboard output |
| `src/sql.rs` | SQL highlighting |
| `src/tunnel.rs` | SSH tunnels for PostgreSQL |
| `src/dsn.rs` | Reads connection URLs and libpq connection strings |
| `src/complete.rs` | Suggestions for the SQL editor |
| `src/guard.rs` | Splits SQL into statements, and finds statements that change all rows |
| `src/chrome.rs` | The title bar controls and resize edges on Linux |
| `src/mark.rs` | The close mark, drawn as two lines |
| `src/diagram.rs` | The relationship diagram: its layout, drawing, drag and zoom |
| `src/theme.rs` | All colors, fonts and sizes, for light and dark |
| `brand/` | Icons and logos. Read `brand/BRAND.md`. |
| `packaging/` | Release packages, one folder for each system |

Views do not name colors. To add a color, add it to both color sets in `src/theme.rs`. Then use it through a style function.

## Screenshots

`cargo run --features shot` adds a mode that runs a script of steps and saves screenshots. Use it to test interface changes. Set the steps in `SQLSEA_SCRIPT`, and the window size in `SQLSEA_SIZE`:

```sh
SQLSEA_SIZE=1024x700 SQLSEA_SCRIPT="table:users;row:2;shot:/tmp/a.png;quit" \
  cargo run --features shot -- ~/data/app.db
```

The app runs one step every 1.2 seconds. The steps are `table:<name>`, `tab:data|structure|query|diagram`, `schema:<name>` (the schema of the diagram), `arrange`, `row:<n>` (open a row in the row panel), `add`, `edit:<field>=<value>`, `save`, `delete`, `sql:<text>`, `run`, `cancel`, `newtab`, `closetab:<id>`, `history`, `usehist:<n>`, `copy`, `move:up|down|pageup|pagedown|first|last`, `filter:<text>`, `applyfilter`, `anyway`, `readonly:on|off`, `url:<text>`, `applyurl`, `search:<text>`, `structure:columns|indexes|keys|refs|sql`, `complete:<n>`, `select:<n>`, `editcell:<row>,<column>`, `cellvalue:<text>`, `commitcell`, `cellnull`, `markdelete`, `review`, `savechanges`, `discardchanges`, `formend`, `scrollquery:<y>`, `pick:<n>`, `kind:pg|sqlite`, `connect`, `forget`, `mode:auto|light|dark`, `shot:<path.png>` and `quit`.

If the screenshots fail, set `ICED_BACKEND=tiny-skia` to use the CPU renderer.

The grid builds only the rows near the view. To build all rows and compare, set `SQLSEA_ALL_ROWS=1`.

A release build takes a few minutes, because it optimizes the whole program at once (`lto = "fat"` in `Cargo.toml`).

## License

You agree that your contributions are under the MIT and the Apache-2.0 licenses, as the README tells.
