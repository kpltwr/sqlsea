//! Reads connection strings that database providers give out, so that the
//! connection form can be filled in from one paste.
//!
//! Supported forms:
//! `postgres://user:password@host:5432/db?sslmode=require`,
//! `host=db.example.com port=5432 user=app dbname=app sslmode=verify-full`,
//! and `sqlite:///path/to/file.db`.

use crate::db::{Config, Kind, Tls};

/// Fills in `config` from a connection string. Fields that the string does
/// not name keep their values.
pub fn apply(text: &str, config: &mut Config) -> Result<(), String> {
    let text = text.trim();
    if let Some(path) = text
        .strip_prefix("sqlite://")
        .or_else(|| text.strip_prefix("sqlite:"))
    {
        let path = decode(path)?;
        if path.is_empty() {
            return Err("The SQLite URL has no file path.".into());
        }
        config.kind = Kind::Sqlite;
        config.path = path;
        return Ok(());
    }
    let pairs = if let Some(rest) = text
        .strip_prefix("postgres://")
        .or_else(|| text.strip_prefix("postgresql://"))
    {
        url_pairs(rest)?
    } else if text.contains('=') {
        keyword_pairs(text)
    } else {
        return Err(
            "This is not a connection URL. It must start with postgres://, \
             postgresql:// or sqlite:, or be a list such as host=... dbname=...."
                .into(),
        );
    };

    config.kind = Kind::Postgres;
    for (key, value) in pairs {
        match key.as_str() {
            "host" => config.host = value,
            "port" => config.port = value,
            "user" => config.user = value,
            "password" => config.password = value,
            "dbname" => config.database = value,
            "sslrootcert" => config.ca_file = value,
            "sslmode" => {
                config.tls = match value.as_str() {
                    "disable" => Tls::Off,
                    "allow" | "prefer" => Tls::Prefer,
                    "require" => Tls::Require,
                    "verify-ca" | "verify-full" => Tls::Verify,
                    other => return Err(format!("The sslmode {other} is not known.")),
                }
            }
            // Other settings, such as application_name, do not apply here.
            _ => {}
        }
    }
    Ok(())
}

/// The parts of `user:password@host:port/db?key=value` as key and value pairs.
fn url_pairs(rest: &str) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    let (main, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = main.split_once('/').unwrap_or((main, ""));
    // The password can hold "@", so the last "@" ends the user part.
    let (credentials, hostport) = match authority.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, authority),
    };
    if let Some(credentials) = credentials {
        let (user, password) = credentials.split_once(':').unwrap_or((credentials, ""));
        pairs.push(("user".to_string(), decode(user)?));
        if !password.is_empty() {
            pairs.push(("password".to_string(), decode(password)?));
        }
    }
    // An IPv6 host is in brackets: [::1]:5432.
    let (host, port) = if let Some(inner) = hostport.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or("The host in the URL has a \"[\" without a \"]\".")?;
        (host.to_string(), after.strip_prefix(':').unwrap_or(""))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (decode(h)?, p),
            None => (decode(hostport)?, ""),
        }
    };
    if !host.is_empty() {
        pairs.push(("host".to_string(), host));
    }
    if !port.is_empty() {
        if port.parse::<u16>().is_err() {
            return Err(format!("The port {port} in the URL is not a number."));
        }
        pairs.push(("port".to_string(), port.to_string()));
    }
    if !path.is_empty() {
        pairs.push(("dbname".to_string(), decode(path)?));
    }
    for part in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        pairs.push((decode(key)?, decode(value)?));
    }
    Ok(pairs)
}

/// The pairs of a libpq list such as `host=x port=5432 password='a b'`.
fn keyword_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let key: String =
            std::iter::from_fn(|| chars.next_if(|c| *c != '=' && !c.is_whitespace())).collect();
        if key.is_empty() {
            break;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next_if_eq(&'=').is_none() {
            break;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut value = String::new();
        if chars.next_if_eq(&'\'').is_some() {
            // A quoted value. A backslash keeps the next character.
            while let Some(c) = chars.next() {
                match c {
                    '\\' => value.extend(chars.next()),
                    '\'' => break,
                    c => value.push(c),
                }
            }
        } else {
            value = std::iter::from_fn(|| chars.next_if(|c| !c.is_whitespace())).collect();
        }
        pairs.push((key, value));
    }
    pairs
}

/// Decodes `%XX` sequences in a part of a URL.
fn decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or("The URL has a \"%\" that is not followed by two hex digits.")?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "The URL has characters that are not valid UTF-8.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Config {
        let mut c = Config::default();
        apply(text, &mut c).unwrap();
        c
    }

    #[test]
    fn reads_a_full_url() {
        let c = read(
            "postgresql://app%40corp:p%40ss:w@db.example.com:6543/sales%20db\
             ?sslmode=verify-full&sslrootcert=/etc/ssl/rds.pem&application_name=x",
        );
        assert_eq!(c.kind, Kind::Postgres);
        assert_eq!(c.user, "app@corp");
        assert_eq!(c.password, "p@ss:w");
        assert_eq!(c.host, "db.example.com");
        assert_eq!(c.port, "6543");
        assert_eq!(c.database, "sales db");
        assert_eq!(c.tls, Tls::Verify);
        assert_eq!(c.ca_file, "/etc/ssl/rds.pem");
    }

    #[test]
    fn reads_short_urls_and_ipv6_hosts() {
        let c = read("postgres://localhost/app");
        assert_eq!((c.host.as_str(), c.database.as_str()), ("localhost", "app"));
        let c = read("postgres://u@[::1]:5433");
        assert_eq!(
            (c.host.as_str(), c.port.as_str(), c.user.as_str()),
            ("::1", "5433", "u")
        );
    }

    #[test]
    fn reads_a_keyword_list() {
        let c = read(
            "host=db.example.com port=5432 user=app password='a b\\'c' dbname=app sslmode=require",
        );
        assert_eq!(c.password, "a b'c");
        assert_eq!(c.database, "app");
        assert_eq!(c.tls, Tls::Require);
    }

    #[test]
    fn reads_sqlite_urls() {
        assert_eq!(read("sqlite:///home/me/app.db").path, "/home/me/app.db");
        assert_eq!(read("sqlite:data/app.db").kind, Kind::Sqlite);
    }

    #[test]
    fn keeps_fields_that_the_url_does_not_name() {
        let mut c = Config {
            name: "Mine".into(),
            timeout: "30".into(),
            ..Default::default()
        };
        apply("postgres://h/db", &mut c).unwrap();
        assert_eq!((c.name.as_str(), c.timeout.as_str()), ("Mine", "30"));
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        let mut c = Config::default();
        assert!(apply("mysql://h/db", &mut c).is_err());
        assert!(apply("postgres://h:port/db", &mut c).is_err());
        assert!(apply("postgres://h/db?sslmode=sometimes", &mut c).is_err());
        assert!(apply("postgres://h/%zz", &mut c).is_err());
    }
}
