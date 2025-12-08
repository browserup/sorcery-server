use axum::http::Uri;

#[derive(Debug, Clone, PartialEq)]
pub enum SubdomainMode {
    DirectProtocol,
    WwwRedirect,
    EnterpriseTenant(String),
}

pub fn detect_mode(host: &str, uri: &Uri) -> SubdomainMode {
    if is_localhost(host) {
        if let Some(override_mode) = check_query_override(uri) {
            return override_mode;
        }
    }

    detect_mode_from_host(host)
}

fn is_localhost(host: &str) -> bool {
    let host_without_port = host.split(':').next().unwrap_or(host);
    host_without_port == "localhost"
        || host_without_port == "127.0.0.1"
        || host_without_port == "::1"
}

fn check_query_override(uri: &Uri) -> Option<SubdomainMode> {
    let query = uri.query()?;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("_subdomain=") {
            return Some(match value {
                "www" => SubdomainMode::WwwRedirect,
                "direct" | "" => SubdomainMode::DirectProtocol,
                tenant => SubdomainMode::EnterpriseTenant(tenant.to_string()),
            });
        }
    }
    None
}

fn detect_mode_from_host(host: &str) -> SubdomainMode {
    let host_without_port = host.split(':').next().unwrap_or(host);

    let parts: Vec<&str> = host_without_port.split('.').collect();

    // localhost or IP address - treat as direct protocol
    if parts.len() == 1 || host_without_port.parse::<std::net::Ipv4Addr>().is_ok() {
        return SubdomainMode::DirectProtocol;
    }

    // srcuri.com or just domain.tld (2 parts)
    if parts.len() == 2 {
        return SubdomainMode::DirectProtocol;
    }

    // subdomain.srcuri.com (3+ parts)
    let subdomain = parts[0];

    match subdomain {
        "www" => SubdomainMode::WwwRedirect,
        tenant => SubdomainMode::EnterpriseTenant(tenant.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    #[test]
    fn test_srcuri_com_is_direct() {
        assert_eq!(
            detect_mode("srcuri.com", &uri("/")),
            SubdomainMode::DirectProtocol
        );
    }

    #[test]
    fn test_www_srcuri_com_redirects() {
        assert_eq!(
            detect_mode("www.srcuri.com", &uri("/")),
            SubdomainMode::WwwRedirect
        );
    }

    #[test]
    fn test_tenant_subdomain() {
        assert_eq!(
            detect_mode("fedex.srcuri.com", &uri("/")),
            SubdomainMode::EnterpriseTenant("fedex".to_string())
        );
    }

    #[test]
    fn test_localhost_is_direct() {
        assert_eq!(
            detect_mode("localhost:3000", &uri("/")),
            SubdomainMode::DirectProtocol
        );
    }

    #[test]
    fn test_query_override_tenant() {
        assert_eq!(
            detect_mode("localhost:3000", &uri("/?_subdomain=acme")),
            SubdomainMode::EnterpriseTenant("acme".to_string())
        );
    }

    #[test]
    fn test_query_override_with_other_params() {
        assert_eq!(
            detect_mode("localhost:3000", &uri("/path?foo=bar&_subdomain=acme&baz=qux")),
            SubdomainMode::EnterpriseTenant("acme".to_string())
        );
    }

    #[test]
    fn test_host_with_port() {
        assert_eq!(
            detect_mode("acme.srcuri.com:443", &uri("/")),
            SubdomainMode::EnterpriseTenant("acme".to_string())
        );
    }

    #[test]
    fn test_query_override_ignored_on_production() {
        assert_eq!(
            detect_mode("srcuri.com", &uri("/?_subdomain=acme")),
            SubdomainMode::DirectProtocol
        );
    }

    #[test]
    fn test_query_override_works_on_127_0_0_1() {
        assert_eq!(
            detect_mode("127.0.0.1:3000", &uri("/?_subdomain=acme")),
            SubdomainMode::EnterpriseTenant("acme".to_string())
        );
    }
}
