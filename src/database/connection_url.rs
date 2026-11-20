//! Parses `postgres://user:password@host:5432/database` so a pasted URL can fill the form.

use crate::constants::POSTGRES_URL_SCHEMES;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConnectionUrl {
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// `None` when the text is not a PostgreSQL connection URL.
pub fn parse_connection_url(text: &str) -> Option<ConnectionUrl> {
    let text = text.trim();
    let after_scheme = POSTGRES_URL_SCHEMES
        .iter()
        .find_map(|scheme| text.strip_prefix(scheme))?;
    // Query (`?sslmode=…`) and fragment are not needed to fill the form.
    let without_query = after_scheme.split(['?', '#']).next().unwrap_or_default();

    let (authority, database) = match without_query.split_once('/') {
        Some((authority, database)) => (authority, Some(database)),
        None => (without_query, None),
    };
    // The password may itself contain '@', so split at the last one.
    let (credentials, host_and_port) = match authority.rsplit_once('@') {
        Some((credentials, host_and_port)) => (Some(credentials), host_and_port),
        None => (None, authority),
    };
    let (user, password) = match credentials.map(|credentials| credentials.split_once(':')) {
        Some(Some((user, password))) => (Some(user), Some(password)),
        Some(None) => (credentials, None),
        None => (None, None),
    };
    let (host, port) = split_host_and_port(host_and_port)?;

    Some(ConnectionUrl {
        host: percent_decode(host),
        port,
        user: filter_non_empty(user).map(percent_decode),
        password: password.map(percent_decode),
        database: filter_non_empty(database).map(percent_decode),
    })
}

/// `localhost:5432` -> (`localhost`, 5432); `[::1]:5432` -> (`::1`, 5432).
fn split_host_and_port(host_and_port: &str) -> Option<(&str, Option<u16>)> {
    let (host, port) = match host_and_port.rsplit_once(':') {
        // A ':' inside brackets belongs to an IPv6 address, not to a port.
        Some((host, port)) if !port.contains(']') => (host, Some(port.parse().ok()?)),
        _ => (host_and_port, None),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return None;
    }
    Some((host, port))
}

fn filter_non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// `p%40ss` -> `p@ss`. Invalid escapes are kept as typed.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped_byte = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped_byte {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_url() {
        let parsed =
            parse_connection_url("postgres://app_user:secret@localhost:5433/app_db").unwrap();
        assert_eq!(
            parsed,
            ConnectionUrl {
                host: "localhost".to_string(),
                port: Some(5433),
                user: Some("app_user".to_string()),
                password: Some("secret".to_string()),
                database: Some("app_db".to_string()),
            }
        );
    }

    #[test]
    fn parses_minimal_url_and_postgresql_scheme() {
        let parsed = parse_connection_url("postgresql://db.internal").unwrap();
        assert_eq!(parsed.host, "db.internal");
        assert_eq!(
            (parsed.port, parsed.user, parsed.database),
            (None, None, None)
        );
    }

    #[test]
    fn decodes_escaped_password_and_ignores_query() {
        let parsed =
            parse_connection_url("postgres://me:p%40ss%3Aword@host/app?sslmode=disable").unwrap();
        assert_eq!(parsed.password.as_deref(), Some("p@ss:word"));
        assert_eq!(parsed.database.as_deref(), Some("app"));
    }

    #[test]
    fn handles_ipv6_hosts() {
        let parsed = parse_connection_url("postgres://u@[::1]:5432/db").unwrap();
        assert_eq!((parsed.host.as_str(), parsed.port), ("::1", Some(5432)));
    }

    #[test]
    fn rejects_other_text() {
        assert_eq!(parse_connection_url("localhost:5432"), None);
        assert_eq!(parse_connection_url("mysql://u@h/db"), None);
        assert_eq!(parse_connection_url("postgres://u@h:notaport/db"), None);
        assert_eq!(parse_connection_url("postgres://"), None);
    }
}
