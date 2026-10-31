<img src="brand/png/lockup-528.png" alt="sqlsea" width="264">

A small native database client in Rust. Browse tables, edit rows, run SQL. Built with iced.

Supports PostgreSQL and SQLite, on Linux, macOS and Windows.

## Install

Build a package on the platform you want it for; each script writes to `dist/`.

**Linux**

```sh
packaging/linux/package.sh    # dist/sqlsea-<version>-linux-<arch>.tar.gz
```

Unpack it anywhere and run `./install.sh` inside. That puts sqlsea in `~/.local/bin` and in your app launcher; `./install.sh --remove` takes it out. From the source tree, `cargo build --release && packaging/linux/install.sh` does the same.

**macOS** (needs Xcode command line tools)

```sh
packaging/macos/package.sh    # dist/sqlsea-<version>-macos.dmg, for Apple silicon and Intel
```

Open the disk image and drag sqlsea to Applications. Unless you sign and notarize it (see the top of `packaging/macos/package.sh`), macOS will ask before the first launch on other Macs: open it with right-click, Open.

**Windows** (needs Rust with the MSVC toolchain and the Visual Studio C++ build tools)

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\package.ps1
```

That writes `dist\sqlsea-<version>-windows-<arch>.zip`. The `sqlsea.exe` inside is the whole app and runs from anywhere. To add it to the Start menu, run `install.ps1` from the unpacked folder (`-Remove` undoes it). Windows SmartScreen warns about unsigned programs the first time; choose More info, Run anyway.

## Run from source

```sh
cargo run --release                     # connection screen
cargo run --release -- ~/data/app.db    # open a SQLite file directly
cargo run --release -- "My database"    # open a saved connection by name
```

The release binary is `target/release/sqlsea`. To put it in your app launcher with its icon:

```sh
cargo build --release && packaging/linux/install.sh     # --remove undoes it
```

## Use

- **Data**: the open table, 200 rows a page, paged by primary key. Click a column head to sort (ascending, descending, off). Click a row to edit it in the side panel. Changed fields turn blue; Save writes only those. NULL toggles a nullable field. Add row inserts; columns left empty take their defaults.
- **Structure**: columns, types, nullability, defaults and the primary key.
- **Query**: write SQL and press Ctrl+Enter (Cmd+Enter on macOS), or Run. If text is selected, only the selection runs. Several statements can run at once; the last result set is shown, up to 5,000 rows.

Rows can only be edited in tables with a primary key, and never in views.

Light and dark themes are in the top bar. Auto follows your desktop's setting and switches when it does; your choice is remembered.

| Keys | |
| --- | --- |
| Ctrl+Enter | Run the query (Cmd on macOS, as for all of these) |
| Ctrl+R | Reload the schema and the open table |
| Ctrl+1, 2, 3 | Data, Structure, Query |
| Esc | Close the row panel |
| Tab, Shift+Tab | Move between fields |

## Saved connections

Connection details are saved in `connections.json` in sqlsea's config folder: `~/.config/sqlsea` on Linux, `~/Library/Application Support/sqlsea` on macOS, `%APPDATA%\sqlsea` on Windows. On Linux and macOS the file is readable only by you.

**Passwords are never written to that file.** They go into the system keyring, where they are encrypted and unlocked by your login: the Secret Service on Linux (GNOME Keyring, KWallet), the Keychain on macOS, and Credential Manager on Windows. A file from an older sqlsea that still holds passwords is moved into the keyring on first launch. If no keyring is available, sqlsea says so and asks for the password each time instead of saving it.

## Remote databases

PostgreSQL connections can use TLS. Choose under Encryption:

| Mode | What it does | Use it for |
| --- | --- | --- |
| Off | Plain connection | This computer, or an SSH tunnel |
| If available | Encrypts when the server supports it; the certificate is not checked | The default; matches `sslmode=prefer` |
| Required | Always encrypts; the certificate is not checked | Servers that insist on TLS; matches `sslmode=require` |
| Verified | Always encrypts, and checks the certificate chain and host name | Remote servers; matches `sslmode=verify-full` |

Verified trusts the certificate authorities your system trusts. Providers that sign with their own authority (Amazon RDS, for one) publish a CA bundle: download it and set it as the CA certificate file. After connecting, the top bar says whether the connection is encrypted.

TLS is rustls with the ring provider, so there is no OpenSSL dependency.

## Design screenshots

`cargo run --features shot` adds a scripted mode for design review:

```sh
ICED_BACKEND=tiny-skia SQLSEA_SCRIPT="table:users;row:2;shot:/tmp/a.png;quit" \
  cargo run --features shot -- ~/data/app.db
```

Steps run one every 1.2 seconds: `table:<name>`, `tab:data|structure|query`, `row:<n>`, `add`, `edit:<field>=<value>`, `save`, `delete`, `sql:<text>`, `run`, `shot:<path.png>`, `quit`.

## Brand

Logos, icons, colours and type are in [`brand/`](brand/BRAND.md). Rebuild them with `cargo run --example brand`.

## Fonts

IBM Plex Sans, Familjen Grotesk and JetBrains Mono are embedded from `assets/fonts`, each under the SIL Open Font License (licence files beside them).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

The bundled fonts are under the SIL Open Font License; see `assets/fonts`.
