//! Outbound URL guard (SPEC 36 R0.3 / LLD G03, directive §4 "Do now").
//!
//! Every outbound request whose URL comes from a user, an agent card, a stored
//! config or an LLM tool argument goes through [`check`] and is sent with a
//! client from [`client_for`]. Before this nothing validated such URLs: remote
//! MCP endpoints (`src/agent_backend/mcp_client.rs`, `rpc`), A2A push webhooks
//! (`src/a2a_webhook.rs`, `deliver`), Web Push endpoints
//! (`src/handlers/push.rs`, `send_push_to_user`) and the `edit_image` tool's
//! `image_url` (`src/agent_backend/tools/domains/media.rs`) could all be
//! pointed at loopback, the cloud metadata address or private networks, and
//! every client followed up to 10 redirects [V: inventory of all 73 outbound
//! call sites, recorded on the PR].
//!
//! What [`check`] enforces:
//! * scheme `https` (or `http` only when `ABW_ALLOW_HTTP_OUTBOUND=1`);
//! * no userinfo in the URL;
//! * port 443 / 80, or one listed in `ABW_OUTBOUND_EXTRA_PORTS`;
//! * every address the host resolves to is public — not loopback,
//!   unspecified, private (RFC 1918), CGNAT (100.64/10), link-local
//!   (169.254/16, fe80::/10), unique-local (fc00::/7), multicast, broadcast,
//!   benchmarking, documentation, reserved, or an IPv4-mapped/compatible/
//!   NAT64 IPv6 form of any of these.
//!
//! What [`client_for`] adds:
//! * the connection is **pinned** to the address `check` approved
//!   (`ClientBuilder::resolve`), so a DNS answer that changes between check and
//!   connect (rebinding) cannot redirect the request;
//! * redirects are refused (a 3xx is an error carrying the `Location`);
//! * `no_proxy()`, because a proxy would resolve the host itself, bypassing
//!   the pin.
//!
//! IP-literal hosts never reach a DNS resolver (hyper parses them directly),
//! which is why the address check runs on the parsed URL, not in a resolver.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

/// A URL that passed [`check`], and the one address it may be fetched from.
/// Fields are private: the only way to get one is through `check`.
#[derive(Debug, Clone)]
pub struct GuardedUrl {
    url: url::Url,
    host: String,
    pinned: SocketAddr,
}

impl GuardedUrl {
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }
    pub fn url(&self) -> &url::Url {
        &self.url
    }
    pub fn pinned(&self) -> SocketAddr {
        self.pinned
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetGuardError {
    Parse(String),
    Scheme(String),
    Userinfo,
    Port(u16),
    NoHost,
    Unresolvable(String),
    /// The host is, or resolves to, an address that must not be reached.
    Forbidden {
        addr: IpAddr,
        class: &'static str,
    },
}

impl std::fmt::Display for NetGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetGuardError::Parse(e) => write!(f, "invalid URL: {e}"),
            NetGuardError::Scheme(s) => write!(f, "URL scheme '{s}' is not allowed (https only)"),
            NetGuardError::Userinfo => write!(f, "URLs with embedded credentials are not allowed"),
            NetGuardError::Port(p) => write!(f, "port {p} is not allowed"),
            NetGuardError::NoHost => write!(f, "URL has no host"),
            NetGuardError::Unresolvable(h) => write!(f, "host '{h}' does not resolve"),
            NetGuardError::Forbidden { addr, class } => {
                write!(
                    f,
                    "destination {addr} is a {class} address and is not reachable from here"
                )
            }
        }
    }
}

impl std::error::Error for NetGuardError {}

/// Classify an address. `None` = public, reachable.
pub fn forbidden_class(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => v4_class(v4),
        IpAddr::V6(v6) => v6_class(v6),
    }
}

fn v4_class(ip: Ipv4Addr) -> Option<&'static str> {
    let o = ip.octets();
    if ip.is_unspecified() || o[0] == 0 {
        return Some("unspecified/this-network");
    }
    if ip.is_loopback() {
        return Some("loopback");
    }
    if ip.is_private() {
        return Some("private");
    }
    if ip.is_link_local() {
        return Some("link-local");
    }
    if o[0] == 100 && (o[1] & 0b1100_0000) == 64 {
        return Some("carrier-grade NAT");
    }
    if o[0] == 192 && o[1] == 0 && o[2] == 0 {
        return Some("IETF protocol assignment");
    }
    if (o[0] == 192 && o[1] == 0 && o[2] == 2)
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)
    {
        return Some("documentation");
    }
    if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
        return Some("benchmarking");
    }
    if ip.is_multicast() {
        return Some("multicast");
    }
    if ip.is_broadcast() || o[0] >= 240 {
        return Some("reserved/broadcast");
    }
    None
}

fn v6_class(ip: Ipv6Addr) -> Option<&'static str> {
    if ip.is_unspecified() {
        return Some("unspecified");
    }
    if ip.is_loopback() {
        return Some("loopback");
    }
    // Embedded IPv4 forms are judged by the IPv4 they carry.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_class(v4);
    }
    let s = ip.segments();
    // IPv4-compatible (deprecated ::a.b.c.d) and NAT64 (64:ff9b::/96).
    if s[0..6] == [0, 0, 0, 0, 0, 0] || (s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0])
    {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return v4_class(v4).map(|_| "IPv4-embedded non-public");
    }
    if (s[0] & 0xfe00) == 0xfc00 {
        return Some("unique-local");
    }
    if (s[0] & 0xffc0) == 0xfe80 {
        return Some("link-local");
    }
    if (s[0] & 0xff00) == 0xff00 {
        return Some("multicast");
    }
    if s[0] == 0x2001 && s[1] == 0x0db8 {
        return Some("documentation");
    }
    None
}

fn allow_http() -> bool {
    std::env::var("ABW_ALLOW_HTTP_OUTBOUND").as_deref() == Ok("1")
}

fn port_allowed(port: u16) -> bool {
    if port == 443 || port == 80 {
        return true;
    }
    std::env::var("ABW_OUTBOUND_EXTRA_PORTS")
        .ok()
        .map(|v| {
            v.split(',')
                .filter_map(|p| p.trim().parse::<u16>().ok())
                .any(|p| p == port)
        })
        .unwrap_or(false)
}

/// Parse and validate a URL, resolve its host, and return it pinned to the
/// first address. Rejects if **any** resolved address is forbidden: a host
/// whose name resolves to both a public and a private address is refused
/// rather than trusted to pick the public one.
pub async fn check(raw: &str) -> Result<GuardedUrl, NetGuardError> {
    let url = url::Url::parse(raw.trim()).map_err(|e| NetGuardError::Parse(e.to_string()))?;
    match url.scheme() {
        "https" => {}
        "http" if allow_http() => {}
        other => return Err(NetGuardError::Scheme(other.to_string())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetGuardError::Userinfo);
    }
    let port = url.port_or_known_default().ok_or(NetGuardError::Port(0))?;
    if !port_allowed(port) {
        return Err(NetGuardError::Port(port));
    }
    let host = url.host_str().ok_or(NetGuardError::NoHost)?.to_string();

    let addrs: Vec<IpAddr> = match url.host() {
        Some(url::Host::Ipv4(v4)) => vec![IpAddr::V4(v4)],
        Some(url::Host::Ipv6(v6)) => vec![IpAddr::V6(v6)],
        Some(url::Host::Domain(d)) => {
            let target = format!("{d}:{port}");
            tokio::net::lookup_host(target)
                .await
                .map_err(|_| NetGuardError::Unresolvable(d.to_string()))?
                .map(|sa| sa.ip())
                .collect()
        }
        None => return Err(NetGuardError::NoHost),
    };
    if addrs.is_empty() {
        return Err(NetGuardError::Unresolvable(host));
    }
    for a in &addrs {
        if let Some(class) = forbidden_class(*a) {
            return Err(NetGuardError::Forbidden { addr: *a, class });
        }
    }
    Ok(GuardedUrl {
        url,
        host,
        pinned: SocketAddr::new(addrs[0], port),
    })
}

/// A client that can reach only `g`'s pinned address: DNS for its host is
/// overridden, redirects are refused, proxies are ignored.
pub fn client_for(g: &GuardedUrl, timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .resolve(&g.host, g.pinned)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent("AgentBestiary/1.0 (+https://agent-bestiary.world)")
        .build()
        .map_err(|e| format!("http client build failed: {e}"))
}

/// Check, then build the pinned client: the one-call form for call sites.
pub async fn guarded_client(
    raw: &str,
    timeout: Duration,
) -> Result<(GuardedUrl, reqwest::Client), String> {
    let g = check(raw).await.map_err(|e| e.to_string())?;
    let c = client_for(&g, timeout)?;
    Ok((g, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_non_public_addresses() {
        for (ip, want) in [
            ("127.0.0.1", true),
            ("0.0.0.0", true),
            ("10.0.0.1", true),
            ("172.16.5.4", true),
            ("192.168.1.1", true),
            ("169.254.169.254", true),
            ("100.64.0.1", true),
            ("100.127.255.255", true),
            ("192.0.0.170", true),
            ("198.18.0.1", true),
            ("224.0.0.1", true),
            ("255.255.255.255", true),
            ("240.0.0.1", true),
            ("::1", true),
            ("::", true),
            ("fe80::1", true),
            ("fd00::1", true),
            ("ff02::1", true),
            ("::ffff:10.0.0.1", true),
            ("::ffff:169.254.169.254", true),
            ("::127.0.0.1", true),
            ("64:ff9b::a9fe:a9fe", true),
            ("2001:db8::1", true),
            ("8.8.8.8", false),
            ("1.1.1.1", false),
            ("100.128.0.1", false),
            ("2606:4700:4700::1111", false),
            ("::ffff:8.8.8.8", false),
        ] {
            let addr: IpAddr = ip.parse().unwrap();
            assert_eq!(
                forbidden_class(addr).is_some(),
                want,
                "{ip}: {:?}",
                forbidden_class(addr)
            );
        }
    }

    #[tokio::test]
    async fn rejects_forbidden_urls() {
        for u in [
            "http://127.0.0.1/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://169.254.169.254/latest/meta-data/",
            "https://10.0.0.1/",
            "https://[::ffff:10.0.0.1]/",
            "https://localhost/",
            "https://user:pass@example.com/",
            "https://example.com:8080/",
            "ftp://example.com/",
            "file:///etc/passwd",
            "gopher://example.com/",
            "not a url",
        ] {
            assert!(check(u).await.is_err(), "{u} must be rejected");
        }
    }

    #[test]
    fn plain_http_is_refused_by_default() {
        // Without ABW_ALLOW_HTTP_OUTBOUND=1 the scheme check fails before any
        // DNS, so this needs no network.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let r = rt.block_on(check("http://example.com/"));
        assert_eq!(r.unwrap_err(), NetGuardError::Scheme("http".into()));
    }
}
