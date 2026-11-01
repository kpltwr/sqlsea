//! Database access. Every value comes back as text or NULL, so that the app
//! can show and edit a table of any type.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The most rows that the app keeps from a query. It counts the rest.
const MAX_ROWS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Kind {
    #[default]
    Postgres,
    Sqlite,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Postgres => "PostgreSQL",
            Kind::Sqlite => "SQLite",
        }
    }
}

/// How a PostgreSQL connection uses TLS. The modes match `sslmode` in libpq.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tls {
    /// Plain TCP.
    Off,
    /// TLS if the server supports it. The certificate is not checked.
    #[default]
    Prefer,
    /// TLS always. The certificate is not checked.
    Require,
    /// TLS always. The certificate chain and the host name are checked.
    Verify,
}

impl Tls {
    pub const ALL: [Tls; 4] = [Tls::Off, Tls::Prefer, Tls::Require, Tls::Verify];

    pub fn label(self) -> &'static str {
        match self {
            Tls::Off => "Off",
            Tls::Prefer => "If available",
            Tls::Require => "Required",
            Tls::Verify => "Verified",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Tls::Off => "A plain connection. Fine on this computer or through an SSH tunnel.",
            Tls::Prefer => "Encrypts when the server supports it. The certificate is not checked.",
            Tls::Require => "Always encrypts. The certificate is not checked.",
            Tls::Verify => {
                "Always encrypts, and checks the certificate and host name. Use this for remote servers."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    /// The key for the password of this connection in the system keyring.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub user: String,
    /// Kept in the system keyring, never in the configuration file. It is read
    /// from the file only to move passwords out of files from older versions.
    #[serde(default, skip_serializing)]
    pub password: String,
    #[serde(default)]
    pub tls: Tls,
    /// A PEM file with more trusted certificates, for `Tls::Verify`.
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub path: String,
    /// The seconds that a query from the Query tab can run before it is
    /// cancelled. Empty means no limit.
    #[serde(default)]
    pub timeout: String,
    /// The database refuses all changes on this connection.
    #[serde(default)]
    pub read_only: bool,
    /// Connect to PostgreSQL through an SSH tunnel.
    #[serde(default)]
    pub ssh: bool,
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default)]
    pub ssh_port: String,
    #[serde(default)]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_auth: crate::tunnel::SshAuth,
    #[serde(default)]
    pub ssh_key_file: String,
    /// The SSH password or key passphrase. Kept in the system keyring, never
    /// in the configuration file.
    #[serde(default, skip_serializing)]
    pub ssh_secret: String,
}

impl Config {
    /// The query time limit, if there is one. Returns an error if the value is
    /// not a positive whole number.
    pub fn timeout_secs(&self) -> Result<Option<u64>, String> {
        let t = self.timeout.trim();
        if t.is_empty() {
            return Ok(None);
        }
        match t.parse::<u64>() {
            Ok(n) if n > 0 => Ok(Some(n)),
            _ => Err("The query time limit must be a whole number of seconds.".into()),
        }
    }

    /// Where this connection points, without the password.
    pub fn target(&self) -> String {
        match self.kind {
            Kind::Postgres => {
                let host = or(&self.host, "localhost");
                let port = or(&self.port, "5432");
                let user = or(&self.user, "postgres");
                let db = or(&self.database, user);
                if self.ssh {
                    let ssh_user = or(&self.ssh_user, "");
                    let at = if ssh_user.is_empty() { "" } else { "@" };
                    format!(
                        "{user}@{host}:{port}/{db} through SSH to {ssh_user}{at}{}",
                        self.ssh_host.trim()
                    )
                } else {
                    format!("{user}@{host}:{port}/{db}")
                }
            }
            Kind::Sqlite => self.path.clone(),
        }
    }
}

fn or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

#[derive(Clone)]
pub enum Db {
    Postgres(Arc<tokio_postgres::Client>),
    Sqlite(Arc<Sqlite>),
}

pub struct Sqlite {
    conn: Mutex<rusqlite::Connection>,
    /// Stops a running statement from a different thread.
    interrupt: rusqlite::InterruptHandle,
}

impl fmt::Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Db::Postgres(_) => f.write_str("Db::Postgres"),
            Db::Sqlite(_) => f.write_str("Db::Sqlite"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    /// The rows that the last statement changed, if it was not a query.
    pub affected: Option<u64>,
    /// The rows after `MAX_ROWS` that the app did not keep.
    pub dropped: usize,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub schema: Option<String>,
    pub name: String,
    pub is_view: bool,
}

impl Table {
    pub fn qualified(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{}.{}", ident(schema), ident(&self.name)),
            None => ident(&self.name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub result: ResultSet,
    pub total: u64,
}

/// Quote an identifier for both PostgreSQL and SQLite.
pub fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a value as a string literal, or NULL.
pub fn literal(value: Option<&str>) -> String {
    match value {
        Some(v) => format!("'{}'", v.replace('\'', "''")),
        None => "NULL".to_string(),
    }
}

/// An open connection, and if its traffic is encrypted (PostgreSQL only).
#[derive(Debug, Clone)]
pub struct Connected {
    pub db: Db,
    pub encrypted: Option<bool>,
    /// The SSH tunnel, if the connection uses one. It closes when dropped.
    pub tunnel: Option<Arc<crate::tunnel::Tunnel>>,
}

pub async fn connect(config: Config) -> Result<Connected, String> {
    let known_hosts = crate::tunnel::known_hosts().unwrap_or_default();
    connect_with(config, &known_hosts).await
}

/// Like `connect`, with the known_hosts file to check SSH servers against.
pub async fn connect_with(
    config: Config,
    known_hosts: &std::path::Path,
) -> Result<Connected, String> {
    match config.kind {
        Kind::Postgres => {
            let mut pg = tokio_postgres::Config::new();
            let user = or(&config.user, "postgres").to_string();
            let port: u16 = or(&config.port, "5432")
                .parse()
                .map_err(|_| "The port must be a number.".to_string())?;
            pg.host(or(&config.host, "localhost"))
                .dbname(or(&config.database, &user))
                .user(&user)
                .application_name("sqlsea")
                .connect_timeout(Duration::from_secs(10))
                .ssl_mode(match config.tls {
                    Tls::Off => tokio_postgres::config::SslMode::Disable,
                    Tls::Prefer => tokio_postgres::config::SslMode::Prefer,
                    Tls::Require | Tls::Verify => tokio_postgres::config::SslMode::Require,
                });
            if !config.password.is_empty() {
                pg.password(&config.password);
            }
            if config.read_only {
                // The server then refuses every statement that changes data.
                pg.options("-c default_transaction_read_only=on");
            }
            let tunnel = if config.ssh {
                let ssh_host = config.ssh_host.trim();
                if ssh_host.is_empty() {
                    return Err("Type the host of the SSH server.".into());
                }
                let tunnel = crate::tunnel::open(
                    crate::tunnel::Ssh {
                        host: ssh_host,
                        port: or(&config.ssh_port, "22")
                            .parse()
                            .map_err(|_| "The SSH port must be a number.".to_string())?,
                        user: or(&config.ssh_user, &whoami_user()),
                        auth: config.ssh_auth,
                        key_file: &crate::normalized_path(&config.ssh_key_file),
                        secret: &config.ssh_secret,
                        known_hosts,
                    },
                    or(&config.host, "localhost").to_string(),
                    port,
                )
                .await?;
                // TLS checks the database host name, but the traffic goes to the tunnel.
                // `port` adds a port each time it is called, so it is called once.
                pg.hostaddr(std::net::Ipv4Addr::LOCALHOST.into())
                    .port(tunnel.port);
                Some(Arc::new(tunnel))
            } else {
                pg.port(port);
                None
            };

            // The two connectors make different stream types, so each branch
            // starts its own connection task.
            let client = if config.tls == Tls::Off {
                let (client, connection) =
                    pg.connect(tokio_postgres::NoTls).await.map_err(pg_error)?;
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("postgres connection closed: {e}");
                    }
                });
                client
            } else {
                let tls = crate::tls::connector(config.tls, &config.ca_file)?;
                let (client, connection) = pg.connect(tls).await.map_err(pg_error)?;
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("postgres connection closed: {e}");
                    }
                });
                client
            };

            let db = Db::Postgres(Arc::new(client));
            let encrypted = execute(
                db.clone(),
                "select ssl::text from pg_stat_ssl where pid = pg_backend_pid()".into(),
            )
            .await
            .ok()
            .and_then(|rs| rs.rows.first().and_then(|r| r[0].clone()))
            .map(|v| v == "true");
            Ok(Connected {
                db,
                encrypted,
                tunnel,
            })
        }
        Kind::Sqlite => {
            let path = config.path.trim().to_string();
            if path.is_empty() {
                return Err("Choose a database file.".into());
            }
            let read_only = config.read_only;
            blocking(move || {
                let conn = if read_only {
                    use rusqlite::OpenFlags;
                    rusqlite::Connection::open_with_flags(
                        &path,
                        OpenFlags::SQLITE_OPEN_READ_ONLY
                            | OpenFlags::SQLITE_OPEN_URI
                            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    )
                } else {
                    rusqlite::Connection::open(&path)
                }
                .map_err(sqlite_open_error)?;
                // SQLite opens any file without an error. The first read finds a bad file.
                conn.query_row("select count(*) from sqlite_master", [], |_| Ok(()))
                    .map_err(sqlite_open_error)?;
                let interrupt = conn.get_interrupt_handle();
                Ok(Connected {
                    db: Db::Sqlite(Arc::new(Sqlite {
                        conn: Mutex::new(conn),
                        interrupt,
                    })),
                    encrypted: None,
                    tunnel: None,
                })
            })
            .await
        }
    }
}

pub async fn tables(db: Db) -> Result<Vec<Table>, String> {
    match db {
        Db::Postgres(_) => {
            let rs = execute(
                db,
                "select n.nspname, c.relname, c.relkind::text \
                 from pg_class c join pg_namespace n on n.oid = c.relnamespace \
                 where c.relkind in ('r','v','m','p','f') \
                   and n.nspname not in ('pg_catalog','information_schema') \
                   and n.nspname not like 'pg_toast%' \
                 order by n.nspname = 'public' desc, 1, 2"
                    .into(),
            )
            .await?;
            Ok(rs
                .rows
                .into_iter()
                .map(|r| Table {
                    schema: r[0].clone(),
                    name: r[1].clone().unwrap_or_default(),
                    is_view: matches!(r[2].as_deref(), Some("v" | "m")),
                })
                .collect())
        }
        Db::Sqlite(_) => {
            let rs = execute(
                db,
                "select name, type from sqlite_master \
                 where type in ('table','view') and name not like 'sqlite_%' order by name"
                    .into(),
            )
            .await?;
            Ok(rs
                .rows
                .into_iter()
                .map(|r| Table {
                    schema: None,
                    name: r[0].clone().unwrap_or_default(),
                    is_view: r[1].as_deref() == Some("view"),
                })
                .collect())
        }
    }
}

/// The column names of every table, for search. The key is the schema and
/// the table name.
pub async fn column_index(
    db: Db,
) -> Result<std::collections::HashMap<(Option<String>, String), Vec<String>>, String> {
    let rs = match db {
        Db::Postgres(_) => {
            execute_all(
                db,
                "select n.nspname, c.relname, a.attname \
                 from pg_attribute a \
                 join pg_class c on c.oid = a.attrelid \
                 join pg_namespace n on n.oid = c.relnamespace \
                 where c.relkind in ('r','v','m','p','f') and a.attnum > 0 \
                   and not a.attisdropped \
                   and n.nspname not in ('pg_catalog','information_schema') \
                   and n.nspname not like 'pg_toast%' \
                 order by 1, 2, a.attnum"
                    .into(),
            )
            .await?
        }
        Db::Sqlite(_) => {
            execute_all(
                db,
                "select null, m.name, p.name from sqlite_master m \
                 join pragma_table_info(m.name) p \
                 where m.type in ('table','view') and m.name not like 'sqlite_%' \
                 order by m.name, p.cid"
                    .into(),
            )
            .await?
        }
    };
    let mut index: std::collections::HashMap<_, Vec<String>> = std::collections::HashMap::new();
    for row in rs.rows {
        let mut row = row.into_iter();
        let (schema, table, column) = (
            row.next().flatten(),
            row.next().flatten(),
            row.next().flatten(),
        );
        if let (Some(table), Some(column)) = (table, column) {
            index.entry((schema, table)).or_default().push(column);
        }
    }
    Ok(index)
}

pub async fn columns(db: Db, table: Table) -> Result<Vec<Column>, String> {
    match db {
        Db::Postgres(_) => {
            let sql = format!(
                "select a.attname, format_type(a.atttypid, a.atttypmod), \
                        (not a.attnotnull)::text, pg_get_expr(d.adbin, d.adrelid), \
                        coalesce(a.attnum = any(i.indkey), false)::text \
                 from pg_attribute a \
                 left join pg_attrdef d on d.adrelid = a.attrelid and d.adnum = a.attnum \
                 left join pg_index i on i.indrelid = a.attrelid and i.indisprimary \
                 where a.attrelid = {}::regclass and a.attnum > 0 and not a.attisdropped \
                 order by a.attnum",
                literal(Some(&table.qualified()))
            );
            let rs = execute(db, sql).await?;
            Ok(rs
                .rows
                .into_iter()
                .map(|r| Column {
                    name: r[0].clone().unwrap_or_default(),
                    data_type: r[1].clone().unwrap_or_default(),
                    nullable: r[2].as_deref() == Some("true"),
                    default: r[3].clone(),
                    primary_key: r[4].as_deref() == Some("true"),
                })
                .collect())
        }
        Db::Sqlite(_) => {
            let sql = format!("pragma table_info({})", ident(&table.name));
            let rs = execute(db, sql).await?;
            Ok(rs
                .rows
                .into_iter()
                .map(|r| Column {
                    name: r[1].clone().unwrap_or_default(),
                    data_type: r[2].clone().unwrap_or_default().to_lowercase(),
                    nullable: r[3].as_deref() == Some("0"),
                    default: r[4].clone(),
                    primary_key: r[5].as_deref().is_some_and(|pk| pk != "0"),
                })
                .collect())
        }
    }
}

/// The indexes and foreign keys of a table, and the SQL that creates it.
#[derive(Debug, Clone, Default)]
pub struct Structure {
    pub indexes: ResultSet,
    /// Foreign keys from this table to other tables.
    pub foreign_keys: ResultSet,
    /// Foreign keys from other tables to this table.
    pub referenced_by: ResultSet,
    /// The CREATE statement. SQLite keeps the original. For PostgreSQL it is
    /// made from the system catalog.
    pub sql: String,
}

pub async fn structure(db: Db, table: Table, columns: Vec<Column>) -> Result<Structure, String> {
    match db {
        Db::Postgres(_) => pg_structure(db, table, columns).await,
        Db::Sqlite(_) => sqlite_structure(db, table).await,
    }
}

async fn pg_structure(db: Db, table: Table, columns: Vec<Column>) -> Result<Structure, String> {
    let oid = format!("{}::regclass", literal(Some(&table.qualified())));
    let indexes = execute_all(
        db.clone(),
        format!(
            "select i.relname as \"Index\", \
               (select string_agg(a.attname, ', ' order by k.n) \
                  from unnest(ix.indkey) with ordinality k(attnum, n) \
                  join pg_attribute a on a.attrelid = ix.indrelid and a.attnum = k.attnum) \
                 as \"Columns\", \
               case when ix.indisprimary then 'yes' else '' end as \"Primary key\", \
               case when ix.indisunique then 'yes' else '' end as \"Unique\", \
               pg_get_indexdef(ix.indexrelid) as \"Definition\" \
             from pg_index ix join pg_class i on i.oid = ix.indexrelid \
             where ix.indrelid = {oid} \
             order by ix.indisprimary desc, i.relname"
        ),
    )
    .await?;
    let foreign_keys = execute_all(
        db.clone(),
        format!(
            "select conname as \"Name\", pg_get_constraintdef(oid) as \"Definition\" \
             from pg_constraint where conrelid = {oid} and contype = 'f' order by 1"
        ),
    )
    .await?;
    let referenced_by = execute_all(
        db.clone(),
        format!(
            "select conrelid::regclass::text as \"Table\", conname as \"Name\", \
               pg_get_constraintdef(oid) as \"Definition\" \
             from pg_constraint where confrelid = {oid} and contype = 'f' order by 1, 2"
        ),
    )
    .await?;

    let kind = execute(
        db.clone(),
        format!("select relkind::text from pg_class where oid = {oid}"),
    )
    .await?
    .rows
    .first()
    .and_then(|r| r[0].clone())
    .unwrap_or_default();
    let sql = if kind == "v" || kind == "m" {
        let definition = execute(db, format!("select pg_get_viewdef({oid}, true)"))
            .await?
            .rows
            .first()
            .and_then(|r| r[0].clone())
            .unwrap_or_default();
        let what = if kind == "m" {
            "materialized view"
        } else {
            "view"
        };
        format!(
            "create {what} {} as\n{}",
            table.qualified(),
            definition.trim_end()
        )
    } else {
        // PostgreSQL does not keep the CREATE TABLE text, so it is rebuilt from
        // the columns, the constraints and the indexes that no constraint owns.
        let constraints = execute_all(
            db.clone(),
            format!(
                "select conname, pg_get_constraintdef(oid) from pg_constraint \
                 where conrelid = {oid} and contype in ('p','u','c','f','x') \
                 order by contype = 'p' desc, contype, conname"
            ),
        )
        .await?;
        let other_indexes = execute_all(
            db,
            format!(
                "select pg_get_indexdef(ix.indexrelid) from pg_index ix \
                 where ix.indrelid = {oid} \
                   and not exists (select 1 from pg_constraint c where c.conindid = ix.indexrelid) \
                 order by 1"
            ),
        )
        .await?;
        let mut lines: Vec<String> = columns
            .iter()
            .map(|c| {
                let mut line = format!("    {} {}", ident(&c.name), c.data_type);
                if let Some(default) = &c.default {
                    line.push_str(&format!(" default {default}"));
                }
                if !c.nullable {
                    line.push_str(" not null");
                }
                line
            })
            .collect();
        for row in &constraints.rows {
            if let (Some(name), Some(definition)) = (&row[0], &row[1]) {
                lines.push(format!("    constraint {} {definition}", ident(name)));
            }
        }
        let mut sql = format!(
            "create table {} (\n{}\n);",
            table.qualified(),
            lines.join(",\n")
        );
        for row in &other_indexes.rows {
            if let Some(definition) = &row[0] {
                sql.push_str(&format!("\n\n{definition};"));
            }
        }
        sql
    };
    Ok(Structure {
        indexes,
        foreign_keys,
        referenced_by,
        sql,
    })
}

async fn sqlite_structure(db: Db, table: Table) -> Result<Structure, String> {
    let name = literal(Some(&table.name));
    let indexes = execute_all(
        db.clone(),
        format!(
            "select il.name as \"Index\", \
               (select group_concat(name, ', ') from \
                  (select name from pragma_index_info(il.name) order by seqno)) as \"Columns\", \
               case when il.origin = 'pk' then 'yes' else '' end as \"Primary key\", \
               case when il.\"unique\" then 'yes' else '' end as \"Unique\", \
               coalesce(m.sql, case il.origin when 'pk' then 'From the PRIMARY KEY' \
                 when 'u' then 'From a UNIQUE constraint' end) as \"Definition\" \
             from pragma_index_list({name}) il \
             left join sqlite_master m on m.type = 'index' and m.name = il.name \
             order by il.origin = 'pk' desc, il.name"
        ),
    )
    .await?;

    // SQLite foreign keys have no names, and each column of a key is one row.
    let keys = execute_all(
        db.clone(),
        format!(
            "select id, \"table\", group_concat(\"from\", ', '), group_concat(\"to\", ', '), \
               on_delete, on_update \
             from (select * from pragma_foreign_key_list({name}) order by id, seq) \
             group by id order by id"
        ),
    )
    .await?;
    let foreign_keys = ResultSet {
        columns: vec!["Columns".into(), "Definition".into()],
        rows: keys
            .rows
            .iter()
            .map(|r| {
                let get = |i: usize| r[i].clone().unwrap_or_default();
                let mut definition = format!("references {}({})", get(1), get(3));
                for (what, action) in [("on delete", get(4)), ("on update", get(5))] {
                    if action != "NO ACTION" && !action.is_empty() {
                        definition.push_str(&format!(" {what} {}", action.to_lowercase()));
                    }
                }
                vec![Some(get(2)), Some(definition)]
            })
            .collect(),
        ..Default::default()
    };
    let referenced_by = execute_all(
        db.clone(),
        format!(
            "select m.name as \"Table\", group_concat(f.\"from\", ', ') as \"Columns\", \
               group_concat(f.\"to\", ', ') as \"References\" \
             from sqlite_master m join pragma_foreign_key_list(m.name) f \
             where m.type = 'table' and f.\"table\" = {name} collate nocase \
             group by m.name, f.id order by 1"
        ),
    )
    .await?;
    let sql = execute_all(
        db,
        format!(
            "select sql from sqlite_master where tbl_name = {name} and sql is not null \
             order by type in ('table', 'view') desc, type, name"
        ),
    )
    .await?
    .rows
    .into_iter()
    .filter_map(|r| r.into_iter().next().flatten())
    .map(|sql| format!("{};", sql.trim_end_matches(';')))
    .collect::<Vec<_>>()
    .join("\n\n");
    Ok(Structure {
        indexes,
        foreign_keys,
        referenced_by,
        sql,
    })
}

/// One page of a table. `filter` is a condition for the `WHERE` clause.
pub async fn page(
    db: Db,
    table: Table,
    filter: Option<String>,
    order: Option<(String, bool)>,
    offset: u64,
    limit: u64,
) -> Result<Page, String> {
    let filter = match filter {
        Some(f) if !crate::guard::is_single(&f) => {
            return Err("The filter must be one condition, without a semicolon.".into());
        }
        Some(f) => format!(" where {f}"),
        None => String::new(),
    };
    let order = match order {
        Some((column, asc)) => {
            format!(
                " order by {} {}",
                ident(&column),
                if asc { "asc" } else { "desc" }
            )
        }
        None => String::new(),
    };
    let sql = format!(
        "select * from {}{filter}{order} limit {limit} offset {offset}",
        table.qualified()
    );
    let result = execute(db.clone(), sql).await?;
    let count = execute(
        db,
        format!("select count(*) from {}{filter}", table.qualified()),
    )
    .await?;
    let total = count
        .rows
        .first()
        .and_then(|r| r[0].as_deref())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    Ok(Page { result, total })
}

/// Runs any SQL. If there are many statements, the result is from the last
/// statement that returns rows, up to `MAX_ROWS` rows.
pub async fn execute(db: Db, sql: String) -> Result<ResultSet, String> {
    execute_capped(db, sql, MAX_ROWS).await
}

/// Like `execute`, but keeps all rows. Exports use it.
pub async fn execute_all(db: Db, sql: String) -> Result<ResultSet, String> {
    execute_capped(db, sql, usize::MAX).await
}

/// Like `execute`. Cancels the query if it runs longer than the time limit.
pub async fn execute_limited(db: Db, config: Config, sql: String) -> Result<ResultSet, String> {
    let Some(secs) = config.timeout_secs()? else {
        return execute(db, sql).await;
    };
    match tokio::time::timeout(Duration::from_secs(secs), execute(db.clone(), sql)).await {
        Ok(result) => result,
        Err(_) => {
            cancel(db, config).await?;
            Err(format!(
                "The query ran past the {secs} s time limit, so it was cancelled."
            ))
        }
    }
}

/// Stops whatever query is running on the connection.
pub async fn cancel(db: Db, config: Config) -> Result<(), String> {
    match db {
        Db::Postgres(client) => {
            let token = client.cancel_token();
            if config.tls == Tls::Off {
                token.cancel_query(tokio_postgres::NoTls).await
            } else {
                token
                    .cancel_query(crate::tls::connector(config.tls, &config.ca_file)?)
                    .await
            }
            .map_err(pg_error)
        }
        Db::Sqlite(sqlite) => {
            sqlite.interrupt.interrupt();
            Ok(())
        }
    }
}

/// True if the error tells that the query was cancelled.
pub fn is_cancelled(error: &str) -> bool {
    error.contains("canceling statement due to user request") || error == "interrupted"
}

async fn execute_capped(db: Db, sql: String, cap: usize) -> Result<ResultSet, String> {
    let started = Instant::now();
    let mut rs = match db {
        Db::Postgres(client) => {
            let messages = client.simple_query(&sql).await.map_err(pg_error)?;
            let mut rs = ResultSet::default();
            let mut fresh = true;
            for message in messages {
                match message {
                    tokio_postgres::SimpleQueryMessage::RowDescription(cols) => {
                        rs = ResultSet {
                            columns: cols.iter().map(|c| c.name().to_string()).collect(),
                            ..Default::default()
                        };
                        fresh = false;
                    }
                    tokio_postgres::SimpleQueryMessage::Row(row) => {
                        if rs.rows.len() < cap {
                            rs.rows.push(
                                (0..row.len())
                                    .map(|i| row.get(i).map(str::to_string))
                                    .collect(),
                            );
                        } else {
                            rs.dropped += 1;
                        }
                    }
                    tokio_postgres::SimpleQueryMessage::CommandComplete(n) => {
                        if fresh {
                            rs.affected = Some(n);
                        }
                        fresh = true;
                    }
                    _ => {}
                }
            }
            rs
        }
        Db::Sqlite(sqlite) => blocking(move || sqlite_execute(&sqlite.conn, &sql, cap)).await?,
    };
    rs.elapsed = started.elapsed();
    Ok(rs)
}

fn sqlite_execute(
    conn: &Mutex<rusqlite::Connection>,
    sql: &str,
    cap: usize,
) -> Result<ResultSet, String> {
    use rusqlite::fallible_iterator::FallibleIterator;
    use rusqlite::types::ValueRef;

    let conn = conn
        .lock()
        .map_err(|_| "The database is busy.".to_string())?;
    let mut batch = rusqlite::Batch::new(&conn, sql);
    let mut out = ResultSet::default();
    while let Some(mut stmt) = batch.next().map_err(sqlite_error)? {
        if stmt.column_count() == 0 {
            let n = stmt.execute([]).map_err(sqlite_error)?;
            out.affected = Some(n as u64);
            continue;
        }
        let columns: Vec<String> = stmt.column_names().map(str::to_string).collect();
        let width = columns.len();
        let mut rs = ResultSet {
            columns,
            ..Default::default()
        };
        let mut rows = stmt.query([]).map_err(sqlite_error)?;
        while let Some(row) = rows.next().map_err(sqlite_error)? {
            if rs.rows.len() >= cap {
                rs.dropped += 1;
                continue;
            }
            let values = (0..width)
                .map(|i| match row.get_ref(i) {
                    Ok(ValueRef::Null) | Err(_) => None,
                    Ok(ValueRef::Integer(n)) => Some(n.to_string()),
                    Ok(ValueRef::Real(x)) => Some(x.to_string()),
                    Ok(ValueRef::Text(t)) => Some(String::from_utf8_lossy(t).into_owned()),
                    Ok(ValueRef::Blob(b)) => Some(format!("<blob, {} bytes>", b.len())),
                })
                .collect();
            rs.rows.push(values);
        }
        out = rs;
    }
    Ok(out)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

fn pg_error(e: tokio_postgres::Error) -> String {
    use tokio_postgres::error::SqlState;

    let (raw, plain) = match e.as_db_error() {
        Some(db) => {
            let mut raw = db.message().to_string();
            if let Some(detail) = db.detail() {
                raw.push_str(". ");
                raw.push_str(detail);
            }
            let plain = match db.code() {
                c if *c == SqlState::INVALID_PASSWORD
                    || *c == SqlState::INVALID_AUTHORIZATION_SPECIFICATION =>
                {
                    Some(
                        "The server did not accept the user name or password. Correct them and connect again.",
                    )
                }
                c if *c == SqlState::READ_ONLY_SQL_TRANSACTION => Some(READ_ONLY),
                c if *c == SqlState::UNDEFINED_DATABASE => {
                    Some("No database with this name is on the server. Correct the database name.")
                }
                _ => None,
            };
            (raw, plain)
        }
        None => {
            let mut raw = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(inner) = source {
                raw = format!("{raw}: {inner}");
                source = inner.source();
            }
            let plain = connection_hint(&raw);
            (raw, plain)
        }
    };
    with_hint(plain, raw)
}

const READ_ONLY: &str =
    "This connection is read-only. To change data, turn off Read-only for the connection.";

/// The name of the user on this computer, the default SSH user.
fn whoami_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Puts a plain sentence before the error text from the driver.
fn with_hint(plain: Option<&str>, raw: String) -> String {
    match plain {
        Some(plain) => format!("{plain}\n{raw}"),
        None => raw,
    }
}

/// A plain sentence for the usual reasons that a connection fails.
fn connection_hint(error: &str) -> Option<&'static str> {
    let hints = [
        (
            "UnknownIssuer",
            "The server certificate is not signed by an authority that this computer trusts. \
             Add the CA certificate file of your provider, or choose Required.",
        ),
        (
            "NotValidForName",
            "The server certificate is for a different host name. \
             Connect with the host name that is on the certificate.",
        ),
        (
            "Expired",
            "The server certificate expired. Ask the owner of the server to renew it.",
        ),
        (
            "server does not support TLS",
            "The server does not accept TLS. Choose If available or Off.",
        ),
        (
            "Connection refused",
            "Nothing answered at this host and port. \
             Make sure that the server runs and that the port is correct.",
        ),
        (
            "failed to lookup address",
            "The host name was not found. Correct the host name.",
        ),
        (
            "timed out",
            "The server did not answer in time. \
             Make sure that the host is correct and that no firewall blocks the port.",
        ),
    ];
    hints
        .into_iter()
        .find(|(needle, _)| error.contains(needle))
        .map(|(_, plain)| plain)
}

/// A SQLite error, with a plain sentence if the cause is a usual one.
fn sqlite_error(e: rusqlite::Error) -> String {
    let raw = e.to_string();
    let plain = raw.contains("readonly database").then_some(READ_ONLY);
    with_hint(plain, raw)
}

/// A plain sentence for SQLite files that do not open.
fn sqlite_open_error(e: rusqlite::Error) -> String {
    let raw = e.to_string();
    let plain = if raw.contains("unable to open database file") {
        Some(
            "The database file did not open. \
             Make sure that the folder exists and that you can read and write the file.",
        )
    } else if raw.contains("file is not a database") {
        Some("This file is not a SQLite database. Choose a different file.")
    } else {
        None
    };
    with_hint(plain, raw)
}

// ---------- row writes ----------

/// The `WHERE` clause that finds one row by its primary key.
fn key_clause(columns: &[Column], original: &[Option<String>]) -> Option<String> {
    let parts: Vec<String> = columns
        .iter()
        .zip(original)
        .filter(|(c, _)| c.primary_key)
        .map(|(c, v)| match v {
            Some(_) => format!("{} = {}", ident(&c.name), literal(v.as_deref())),
            None => format!("{} is null", ident(&c.name)),
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" and "))
}

pub fn update_sql(
    table: &Table,
    columns: &[Column],
    original: &[Option<String>],
    changes: &[(usize, Option<String>)],
) -> Option<String> {
    let set: Vec<String> = changes
        .iter()
        .map(|(i, v)| format!("{} = {}", ident(&columns[*i].name), literal(v.as_deref())))
        .collect();
    let key = key_clause(columns, original)?;
    Some(format!(
        "update {} set {} where {}",
        table.qualified(),
        set.join(", "),
        key
    ))
}

pub fn insert_sql(table: &Table, columns: &[Column], values: &[(usize, Option<String>)]) -> String {
    if values.is_empty() {
        return format!("insert into {} default values", table.qualified());
    }
    let names: Vec<String> = values
        .iter()
        .map(|(i, _)| ident(&columns[*i].name))
        .collect();
    let vals: Vec<String> = values.iter().map(|(_, v)| literal(v.as_deref())).collect();
    format!(
        "insert into {} ({}) values ({})",
        table.qualified(),
        names.join(", "),
        vals.join(", ")
    )
}

pub fn delete_sql(
    table: &Table,
    columns: &[Column],
    original: &[Option<String>],
) -> Option<String> {
    Some(format!(
        "delete from {} where {}",
        table.qualified(),
        key_clause(columns, original)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, pk: bool) -> Column {
        Column {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            default: None,
            primary_key: pk,
        }
    }

    #[test]
    fn quoting() {
        assert_eq!(ident(r#"we"ird"#), r#""we""ird""#);
        assert_eq!(literal(Some("it's")), "'it''s'");
        assert_eq!(literal(None), "NULL");
    }

    #[test]
    fn update_needs_a_key() {
        let t = Table {
            schema: Some("public".into()),
            name: "users".into(),
            is_view: false,
        };
        let cols = [col("id", true), col("name", false)];
        let row = [Some("7".into()), Some("Ann".into())];
        assert_eq!(
            update_sql(&t, &cols, &row, &[(1, Some("O'Neil".into()))]).unwrap(),
            r#"update "public"."users" set "name" = 'O''Neil' where "id" = '7'"#
        );
        let no_pk = [col("id", false), col("name", false)];
        assert!(update_sql(&t, &no_pk, &row, &[(1, None)]).is_none());
    }

    #[tokio::test]
    async fn sqlite_round_trip() {
        let db = connect(Config {
            kind: Kind::Sqlite,
            path: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .db;
        let rs = execute(
            db.clone(),
            "create table t (id integer primary key, v text); \
             insert into t (v) values ('a'), (null); select * from t"
                .into(),
        )
        .await
        .unwrap();
        assert_eq!(rs.columns, ["id", "v"]);
        assert_eq!(
            rs.rows,
            vec![
                vec![Some("1".into()), Some("a".into())],
                vec![Some("2".into()), None]
            ]
        );
        let tables = tables(db.clone()).await.unwrap();
        assert_eq!(tables[0].name, "t");
        let index = column_index(db.clone()).await.unwrap();
        assert_eq!(index[&(None, "t".to_string())], ["id", "v"]);
        let cols = columns(db, tables[0].clone()).await.unwrap();
        assert!(cols[0].primary_key && !cols[1].primary_key);
    }

    const ENDLESS: &str =
        "with recursive c(x) as (select 1 union all select x + 1 from c) select count(*) from c";

    fn memory() -> Config {
        Config {
            kind: Kind::Sqlite,
            path: ":memory:".into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn sqlite_cancel_stops_a_running_query() {
        let db = connect(memory()).await.unwrap().db;
        let running = tokio::spawn(execute(db.clone(), ENDLESS.into()));
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel(db.clone(), memory()).await.unwrap();
        let error = running.await.unwrap().unwrap_err();
        assert!(is_cancelled(&error), "{error}");
        // The connection works after the cancel.
        assert_eq!(execute(db, "select 1".into()).await.unwrap().rows.len(), 1);
    }

    #[tokio::test]
    async fn read_only_sqlite_refuses_changes() {
        let dir = std::env::temp_dir().join(format!("sqlsea-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ro.db").display().to_string();
        let writable = Config {
            kind: Kind::Sqlite,
            path: path.clone(),
            ..Default::default()
        };
        let db = connect(writable.clone()).await.unwrap().db;
        execute(db, "create table t (id integer primary key)".into())
            .await
            .unwrap();
        let ro = Config {
            read_only: true,
            ..writable
        };
        let db = connect(ro).await.unwrap().db;
        assert!(execute(db.clone(), "select * from t".into()).await.is_ok());
        let error = execute(db, "insert into t values (1)".into())
            .await
            .unwrap_err();
        assert!(
            error.starts_with("This connection is read-only."),
            "{error}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn sqlite_structure_lists_indexes_keys_and_sql() {
        let db = connect(memory()).await.unwrap().db;
        execute(
            db.clone(),
            "create table customers (id integer primary key, email text unique); \
             create table orders (id integer primary key, \
               customer_id integer references customers(id) on delete cascade, \
               placed text); \
             create index orders_placed on orders (placed, customer_id);"
                .into(),
        )
        .await
        .unwrap();
        let table = |name: &str| Table {
            schema: None,
            name: name.into(),
            is_view: false,
        };
        let orders = structure(db.clone(), table("orders"), Vec::new())
            .await
            .unwrap();
        assert_eq!(orders.indexes.rows.len(), 1);
        assert_eq!(
            orders.indexes.rows[0][1].as_deref(),
            Some("placed, customer_id")
        );
        assert_eq!(
            orders.foreign_keys.rows[0][1].as_deref(),
            Some("references customers(id) on delete cascade")
        );
        assert!(
            orders.sql.starts_with("CREATE TABLE orders"),
            "{}",
            orders.sql
        );
        assert!(orders.sql.contains("CREATE INDEX orders_placed"));
        let customers = structure(db, table("customers"), Vec::new()).await.unwrap();
        assert_eq!(
            customers.referenced_by.rows[0][0].as_deref(),
            Some("orders")
        );
        assert_eq!(
            customers.indexes.rows[0][4].as_deref(),
            Some("From a UNIQUE constraint")
        );
    }

    #[tokio::test]
    async fn pages_can_be_filtered() {
        let db = connect(memory()).await.unwrap().db;
        execute(
            db.clone(),
            "create table t (id integer primary key, s text); \
             insert into t (s) values ('a'), ('b'), ('a')"
                .into(),
        )
        .await
        .unwrap();
        let table = Table {
            schema: None,
            name: "t".into(),
            is_view: false,
        };
        let page = page(
            db.clone(),
            table.clone(),
            Some("s = 'a'".into()),
            None,
            0,
            10,
        )
        .await
        .unwrap();
        assert_eq!((page.total, page.result.rows.len()), (2, 2));
        let error = page_err(db, table).await;
        assert!(error.contains("one condition"), "{error}");
    }

    async fn page_err(db: Db, table: Table) -> String {
        page(db, table, Some("1 = 1; delete from t".into()), None, 0, 10)
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn time_limit_cancels() {
        let config = Config {
            timeout: "1".into(),
            ..memory()
        };
        let db = connect(config.clone()).await.unwrap().db;
        let started = Instant::now();
        let error = execute_limited(db, config, ENDLESS.into())
            .await
            .unwrap_err();
        assert!(error.contains("1 s time limit"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        let bad = Config {
            timeout: "soon".into(),
            ..memory()
        };
        assert!(bad.timeout_secs().is_err());
    }

    /// Needs live servers. Run it with:
    /// `SQLSEA_TLS_PORT=55433 SQLSEA_TLS_CA=ca.crt SQLSEA_PLAIN_PORT=55432 cargo test -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn tls_modes() {
        let env = |k: &str| std::env::var(k).expect(k);
        let pg = |port: String, host: &str, tls: Tls, ca: &str| Config {
            kind: Kind::Postgres,
            host: host.into(),
            port,
            user: "postgres".into(),
            password: "sqlsea".into(),
            tls,
            ca_file: ca.into(),
            ..Default::default()
        };
        let encrypted = |c: Config| async move { connect(c).await.map(|c| c.encrypted) };
        let (tls, ca, plain) = (
            env("SQLSEA_TLS_PORT"),
            env("SQLSEA_TLS_CA"),
            env("SQLSEA_PLAIN_PORT"),
        );

        assert_eq!(
            encrypted(pg(tls.clone(), "localhost", Tls::Off, "")).await,
            Ok(Some(false))
        );
        assert_eq!(
            encrypted(pg(tls.clone(), "localhost", Tls::Prefer, "")).await,
            Ok(Some(true))
        );
        assert_eq!(
            encrypted(pg(tls.clone(), "localhost", Tls::Require, "")).await,
            Ok(Some(true))
        );
        let untrusted = encrypted(pg(tls.clone(), "localhost", Tls::Verify, ""))
            .await
            .unwrap_err();
        assert!(
            untrusted.contains("not signed by an authority"),
            "{untrusted}"
        );
        assert_eq!(
            encrypted(pg(tls.clone(), "localhost", Tls::Verify, &ca)).await,
            Ok(Some(true))
        );
        assert_eq!(
            encrypted(pg(tls.clone(), "127.0.0.1", Tls::Verify, &ca)).await,
            Ok(Some(true))
        );
        let missing = encrypted(pg(tls.clone(), "localhost", Tls::Verify, "/nope.pem"))
            .await
            .unwrap_err();
        assert!(
            missing.contains("CA certificate file did not open"),
            "{missing}"
        );

        // The structure of a PostgreSQL table, and the CREATE statement made for it.
        let db = connect(pg(plain.clone(), "localhost", Tls::Off, ""))
            .await
            .unwrap()
            .db;
        execute(
            db.clone(),
            "drop table if exists sq_orders, sq_customers cascade; \
             create table sq_customers (id serial primary key, email text not null unique); \
             create table sq_orders (id bigint generated always as identity primary key, \
               customer_id int references sq_customers(id) on delete cascade, \
               qty int not null default 1 check (qty > 0)); \
             create index sq_orders_customer on sq_orders (customer_id); \
             create view sq_big as select * from sq_orders where qty > 10;"
                .into(),
        )
        .await
        .unwrap();
        let table = |name: &str, is_view| Table {
            schema: Some("public".into()),
            name: name.into(),
            is_view,
        };
        let cols = columns(db.clone(), table("sq_orders", false))
            .await
            .unwrap();
        let s = structure(db.clone(), table("sq_orders", false), cols)
            .await
            .unwrap();
        assert_eq!(s.indexes.rows.len(), 2, "{:?}", s.indexes.rows);
        assert!(
            s.foreign_keys.rows[0][1]
                .as_deref()
                .unwrap()
                .contains("ON DELETE CASCADE")
        );
        assert!(
            s.sql.starts_with("create table \"public\".\"sq_orders\" ("),
            "{}",
            s.sql
        );
        assert!(
            s.sql.contains("\"qty\" integer default 1 not null"),
            "{}",
            s.sql
        );
        assert!(s.sql.contains("CHECK ((qty > 0))"), "{}", s.sql);
        assert!(
            s.sql.contains("CREATE INDEX sq_orders_customer"),
            "{}",
            s.sql
        );
        let s = structure(db.clone(), table("sq_customers", false), Vec::new())
            .await
            .unwrap();
        assert_eq!(s.referenced_by.rows[0][0].as_deref(), Some("sq_orders"));
        let s = structure(db.clone(), table("sq_big", true), Vec::new())
            .await
            .unwrap();
        assert!(
            s.sql.starts_with("create view \"public\".\"sq_big\" as\n"),
            "{}",
            s.sql
        );
        execute(db, "drop table sq_orders, sq_customers cascade".into())
            .await
            .unwrap();

        // A read-only connection reads, and the server refuses changes.
        let read_only = Config {
            read_only: true,
            ..pg(plain.clone(), "localhost", Tls::Off, "")
        };
        let db = connect(read_only).await.unwrap().db;
        assert!(execute(db.clone(), "select 1".into()).await.is_ok());
        let refused = execute(db, "create table sqlsea_ro_test (id int)".into())
            .await
            .unwrap_err();
        assert!(
            refused.starts_with("This connection is read-only."),
            "{refused}"
        );

        // Cancel a running query over plain TCP and over verified TLS.
        for config in [
            pg(plain.clone(), "localhost", Tls::Off, ""),
            pg(tls.clone(), "localhost", Tls::Verify, &ca),
        ] {
            let db = connect(config.clone()).await.unwrap().db;
            let running = tokio::spawn(execute(db.clone(), "select pg_sleep(30)".into()));
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel(db, config).await.unwrap();
            let error = running.await.unwrap().unwrap_err();
            assert!(is_cancelled(&error), "{error}");
        }

        assert_eq!(
            encrypted(pg(plain.clone(), "localhost", Tls::Prefer, "")).await,
            Ok(Some(false))
        );
        let refused = encrypted(pg(plain, "localhost", Tls::Require, ""))
            .await
            .unwrap_err();
        assert!(refused.contains("does not accept TLS"), "{refused}");
    }

    /// Needs an SSH server that reaches a PostgreSQL server at the host
    /// `sqlsea-pgdb`. CONTRIBUTING.md tells how to start them. Run it with:
    /// `SQLSEA_SSH_PORT=2222 SQLSEA_SSH_DIR=/path/to/keys SSH_AUTH_SOCK=... cargo test -- --ignored ssh`
    #[tokio::test]
    #[ignore]
    async fn ssh_tunnel() {
        use crate::tunnel::SshAuth;
        let dir =
            std::path::PathBuf::from(std::env::var("SQLSEA_SSH_DIR").expect("SQLSEA_SSH_DIR"));
        let ssh_port = std::env::var("SQLSEA_SSH_PORT").expect("SQLSEA_SSH_PORT");
        let known = dir.join("known_hosts");
        let config = |auth: SshAuth, key: &str, secret: &str| Config {
            kind: Kind::Postgres,
            host: "sqlsea-pgdb".into(),
            user: "postgres".into(),
            password: "sqlsea".into(),
            tls: Tls::Off,
            ssh: true,
            ssh_host: "127.0.0.1".into(),
            ssh_port: ssh_port.clone(),
            ssh_user: "tunnel".into(),
            ssh_auth: auth,
            ssh_key_file: dir.join(key).display().to_string(),
            ssh_secret: secret.into(),
            ..Default::default()
        };
        let works = |c: Config, known: std::path::PathBuf| async move {
            let connected = connect_with(c, &known).await?;
            let rs = execute(connected.db.clone(), "select 'through ssh'".into()).await?;
            assert_eq!(rs.rows[0][0].as_deref(), Some("through ssh"));
            Ok::<_, String>(connected)
        };

        // Each way to sign in.
        works(config(SshAuth::KeyFile, "id", ""), known.clone())
            .await
            .unwrap();
        works(
            config(SshAuth::KeyFile, "id_pass", "open sesame"),
            known.clone(),
        )
        .await
        .unwrap();
        works(config(SshAuth::Password, "", "tunnel-pw"), known.clone())
            .await
            .unwrap();
        if std::env::var("SSH_AUTH_SOCK").is_ok() {
            works(config(SshAuth::Agent, "", ""), known.clone())
                .await
                .unwrap();
        }

        // Wrong secrets.
        let e = works(config(SshAuth::Password, "", "nope"), known.clone())
            .await
            .unwrap_err();
        assert!(e.contains("did not accept the sign-in"), "{e}");
        let e = works(config(SshAuth::KeyFile, "id_pass", "nope"), known.clone())
            .await
            .unwrap_err();
        assert!(e.contains("passphrase"), "{e}");

        // Host keys that are not known, or that changed.
        let empty = dir.join("known_hosts_empty");
        std::fs::write(&empty, "").unwrap();
        let e = works(config(SshAuth::KeyFile, "id", ""), empty)
            .await
            .unwrap_err();
        assert!(e.contains("not in your known_hosts"), "{e}");
        let other = std::fs::read_to_string(dir.join("id.pub")).unwrap();
        let key = other
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let changed = dir.join("known_hosts_changed");
        std::fs::write(&changed, format!("[127.0.0.1]:{ssh_port} {key}\n")).unwrap();
        let e = works(config(SshAuth::KeyFile, "id", ""), changed)
            .await
            .unwrap_err();
        assert!(e.contains("is different from line 1"), "{e}");

        // A cancel goes through the tunnel too.
        let c = config(SshAuth::KeyFile, "id", "");
        let connected = works(c.clone(), known.clone()).await.unwrap();
        let running = tokio::spawn(execute(connected.db.clone(), "select pg_sleep(30)".into()));
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel(connected.db.clone(), c).await.unwrap();
        let e = running.await.unwrap().unwrap_err();
        assert!(is_cancelled(&e), "{e}");

        // Verified TLS through the tunnel checks the database host name, not 127.0.0.1.
        if let Ok(ca) = std::env::var("SQLSEA_SSH_TLS_CA") {
            let tls = Config {
                host: "sqlsea-pgtls".into(),
                tls: Tls::Verify,
                ca_file: ca,
                ..config(SshAuth::KeyFile, "id", "")
            };
            let connected = works(tls, known.clone()).await.unwrap();
            assert_eq!(connected.encrypted, Some(true));
        }

        // The tunnel closes with the connection.
        let port = connected.tunnel.as_ref().unwrap().port;
        drop(connected);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err()
        );
    }
}
