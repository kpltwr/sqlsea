<img src="brand/png/lockup-528.png" alt="sqlsea" width="264">

sqlsea is a small desktop database client, written in Rust with iced. You can browse tables, edit rows and run SQL on PostgreSQL and SQLite databases. It runs on Linux, macOS and Windows.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/dark.png">
  <img src="docs/screenshots/light.png" alt="sqlsea showing a customers table with one row open for editing">
</picture>

## Install

Build the package on the system that you want to use it on. Each script writes the package to the `dist/` folder.

On Linux, run this command:

```sh
packaging/linux/package.sh    # dist/sqlsea-<version>-linux-<arch>.tar.gz
```

Unpack the file and run `./install.sh` in the folder. The script puts sqlsea in `~/.local/bin` and in your app launcher. To remove it, run `./install.sh --remove`.

On macOS, install the Xcode command line tools first, then run this command:

```sh
packaging/macos/package.sh    # dist/sqlsea-<version>-macos.dmg
```

Open the disk image and drag sqlsea to Applications. The app works on Apple silicon and on Intel. If you do not sign the app, macOS asks before the first start on other computers. To start it, right-click the app and choose Open. To sign the app, read the comments at the top of `packaging/macos/package.sh`.

On Windows, install Rust with the MSVC toolchain and the Visual Studio C++ build tools first. Then run this command:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\package.ps1
```

The command writes `dist\sqlsea-<version>-windows-<arch>.zip`. The `sqlsea.exe` file in it is the complete app. To add sqlsea to the Start menu, run `install.ps1` in the unpacked folder. To remove it, run `install.ps1 -Remove`. The first time, SmartScreen warns about the unsigned app. To start it, choose More info, then Run anyway.

## Run from source

```sh
cargo run --release                     # the connection screen
cargo run --release -- ~/data/app.db    # open a SQLite file
cargo run --release -- "My database"    # open a saved connection by name
```

On Linux, `cargo build --release && packaging/linux/install.sh` adds the build to your app launcher.

## Use

To connect, fill in the form and click Connect. For a SQLite file or a CA certificate, you can type the path or click Browse. A connection that works is saved.

If your provider gives you a PostgreSQL connection URL, paste it in Connection URL and click Fill in. sqlsea reads URLs such as `postgres://user:password@host:5432/db?sslmode=require` and lists such as `host=... dbname=... sslmode=verify-full`. sqlsea does not save the URL. It saves the password in the keyring, as for all connections.

Select Read-only for a connection to a database that you must not change, such as a production database. The database then refuses all changes on that connection.

The search box above the table list finds tables by table name and by column name. If only a column name matches, the column shows below the table name.

The Data tab shows the open table, 200 rows on each page. Click a column head to sort by that column. To show only some rows, type a condition in the filter box, for example `status = 'paid'`, and press Enter. Click a row to edit it in the panel on the right. Save writes only the fields that you changed. Add row inserts a row, and empty fields get their default values. You can edit rows only in tables that have a primary key. You cannot edit views.

The Structure tab shows the columns, their types and defaults, and the primary key.

In the Query tab, write SQL and press Ctrl+Enter. If you select text, only that text runs. The result shows up to 5,000 rows. Each query tab keeps its own SQL and result. To stop a running query, click Cancel or press Esc. To stop long queries automatically, set a time limit in seconds for the connection.

Before sqlsea runs an `UPDATE` or `DELETE` without a `WHERE` clause, a `TRUNCATE`, or a `DROP` of a table, schema or database, it asks you to confirm. These statements change or remove all rows. The check reads the SQL text only, so it does not find every risk.

History keeps each query that you run, for each connection. Click a query in the history to put it back in the editor.

Export saves a query result, or a full table from the Data tab, as CSV or JSON. Ctrl+C copies the selected row as tab-separated values.

The top bar has the Auto, Light and Dark themes. Auto uses the theme of your desktop.

| Keys | Action |
| --- | --- |
| Ctrl+Enter | Run the query |
| Ctrl+T, Ctrl+W | Open or close a query tab |
| Ctrl+C | Copy the selected row |
| Ctrl+P | Search tables and columns |
| Ctrl+F | Filter the rows of the open table |
| Ctrl+R | Reload the tables and the open table |
| Ctrl+1, 2, 3 | Go to Data, Structure or Query |
| Up, Down, Page Up, Page Down, Home, End | Move the row selection |
| Enter | Open the selected row |
| Esc | Stop the running query, or close the row panel |
| Tab, Shift+Tab | Go to the next or previous field |

On macOS, use Cmd instead of Ctrl.

## Saved connections and passwords

sqlsea saves connections in `connections.json` in its configuration folder. This folder is `~/.config/sqlsea` on Linux, `~/Library/Application Support/sqlsea` on macOS and `%APPDATA%\sqlsea` on Windows. The query history is in the `history/` folder there, with one file for each connection. On Linux and macOS, only you can read these files.

sqlsea never writes passwords to a file. It keeps them in the system keyring. This is the Secret Service on Linux (GNOME Keyring or KWallet), the Keychain on macOS and Credential Manager on Windows. If no keyring is available, sqlsea tells you and asks for the password each time.

## Remote databases

PostgreSQL connections can use TLS. Choose one of these modes under Encryption:

| Mode | What it does | Same as |
| --- | --- | --- |
| Off | A plain connection. Use it on your computer or through an SSH tunnel. | `sslmode=disable` |
| If available | Encrypts if the server supports TLS. Does not check the certificate. This is the default. | `sslmode=prefer` |
| Required | Always encrypts. Does not check the certificate. | `sslmode=require` |
| Verified | Always encrypts, and checks the certificate and the host name. Use it for remote servers. | `sslmode=verify-full` |

Verified trusts the certificate authorities that your system trusts. Some providers, for example Amazon RDS, use their own authority. For these providers, download their CA file and choose it as the CA certificate file. After you connect, the top bar tells you if the connection is encrypted.

## Brand

The logos, icons, colors and fonts are in [`brand/`](brand/BRAND.md). To make the files again, run `cargo run --example brand`.

The app includes the IBM Plex Sans, Familjen Grotesk and JetBrains Mono fonts from `assets/fonts`. Each font has the SIL Open Font License, which is in the same folder.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md).

## License

You can use sqlsea under the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or the MIT license ([LICENSE-MIT](LICENSE-MIT)).

Unless you state otherwise, you license each contribution that you submit for inclusion under both licenses, as the Apache-2.0 license defines. There are no other terms or conditions.
