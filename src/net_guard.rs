//! Outbound URL guard (SPEC 36 R0.3 / LLD G03, directive §4 "Do now").
//!
//! Every outbound request whose URL comes from a user, an agent card, a stored
//! config or an LLM tool argument goes through [`check`] and is sent with a
//! client from [`client_for`]. Before this nothing validated such URLs: remote
//! MCP endpoints (`src/agent_backend/mcp_client.rs`, `rpc`), A2A push webhooks
//! (`src/a2a_webhook.rs`, `deliver`), Web Push endpoints
//! (`src/handlers/push.rs`, `send_push_to_user`) and the `edit_image` tool's
//! `image_url` (`src/agent_backend/tools/domains/media.rs`) could all target
//! loopback, the cloud metadata address or private networks, and every client
//! followed up to 10 redirects. [V: the live prototype at 1d3cf4d3 accepted
//! webhooks to 169.254.169.254, 127.0.0.1:5432 and 10.0.0.1,
//! `tests/claims/ssrf_deployed.rs`.]
//!
//! What [`check`] enforces:
//! * scheme `https`; plain `http` only with `ABW_ALLOW_HTTP_OUTBOUND=1`
//!   (SPEC 36 names it `ABW_ALLOW_HTTP`; this is the deployed name);
//! * no userinfo in the URL;
//! * port 443, or 80 only with http allowed, or one listed in
//!   `ABW_OUTBOUND_EXTRA_PORTS` (comma list);
//! * every address the host resolves to is public — see [`forbidden_class`].
//!   A name with any non-public answer is refused, not trusted to pick the
//!   public one.
//!
//! What [`client_for`] adds:
//! * the connection is **pinned** to the approved addresses
//!   (`ClientBuilder::resolve_to_addrs`), so an answer that changes between
//!   check and connect (rebinding) cannot redirect the request;
//! * redirects are refused; [`refuse_redirect`] turns a 3xx into an error that
//!   carries its `Location`;
//! * `no_proxy()`, because a proxy would resolve the host itself.
//!
//! IP-literal hosts never reach a DNS resolver — hyper parses them directly
//! (hyper 0.14 `client/connect/http.rs`, `HttpConnector::call_async`) — which
//! is why the address check runs on the parsed URL, not inside a resolver.
//! `url::Url` normalises decimal/octal/hex/short IPv4 forms (`2130706433`,
//! `0x7f.1`, `127.1`) to `Host::Ipv4` before this sees them.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

/// A URL that passed [`check`], and the addresses it may be fetched from.
/// Fields are private: the only way to get one is through `check`.
#[derive(Debug, Clone)]
pub struct GuardedUrl {
    url: url::Url,
    host: String,
    addrs: Vec<SocketAddr>,
}

impl GuardedUrl {
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }
    pub fn url(&self) -> &url::Url {
        &self.url
    }
    pub fn addrs(&self) -> &[SocketAddr] {
        &self.addrs
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
    if o[0] == 192 && o[1] == 88 && o[2] == 99 {
        return Some("6to4 relay anycast");
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

fn v4_from(hi: u16, lo: u16) -> Ipv4Addr {
    Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8)
}

fn v6_class(ip: Ipv6Addr) -> Option<&'static str> {
    if ip.is_unspecified() {
        return Some("unspecified");
    }
    if ip.is_loopback() {
        return Some("loopback");
    }
    let s = ip.segments();
    // Forms that embed an IPv4 address are judged by the address they carry.
    // ::ffff:a.b.c.d (mapped)
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_class(v4);
    }
    // ::ffff:0:a.b.c.d (SIIT)
    if s[0..4] == [0, 0, 0, 0] && s[4] == 0xffff && s[5] == 0 {
        return v4_class(v4_from(s[6], s[7])).map(|_| "IPv4-translated non-public");
    }
    // ::a.b.c.d (deprecated IPv4-compatible)
    if s[0..6] == [0, 0, 0, 0, 0, 0] {
        return v4_class(v4_from(s[6], s[7])).map(|_| "IPv4-compatible non-public");
    }
    // 64:ff9b::/96 well-known NAT64
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return v4_class(v4_from(s[6], s[7])).map(|_| "NAT64 non-public");
    }
    // 64:ff9b:1::/48 local-use NAT64 (RFC 8215): translates to operator-internal v4.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2] == 0x0001 {
        return Some("local-use NAT64");
    }
    // 2002::/16 6to4: judged by the embedded v4 in segments 1..3.
    if s[0] == 0x2002 {
        return v4_class(v4_from(s[1], s[2])).map(|_| "6to4 non-public");
    }
    // 2001::/32 Teredo: the client address is obfuscated; refuse outright.
    if s[0] == 0x2001 && s[1] == 0x0000 {
        return Some("Teredo");
    }
    if s[0] == 0x0100 && s[1..4] == [0, 0, 0] {
        return Some("discard-only");
    }
    if (s[0] & 0xfe00) == 0xfc00 {
        return Some("unique-local");
    }
    if (s[0] & 0xffc0) == 0xfe80 {
        return Some("link-local");
    }
    if (s[0] & 0xffc0) == 0xfec0 {
        return Some("site-local");
    }
    if (s[0] & 0xff00) == 0xff00 {
        return Some("multicast");
    }
    if (s[0] == 0x2001 && s[1] == 0x0db8) || (s[0] & 0xfff0) == 0x3ff0 {
        return Some("documentation");
    }
    None
}

fn allow_http() -> bool {
    std::env::var("ABW_ALLOW_HTTP_OUTBOUND").as_deref() == Ok("1")
}

fn port_allowed(scheme: &str, port: u16) -> bool {
    if port == 443 || (port == 80 && scheme == "http") {
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

/// Parse and validate a URL, resolve its host with the system resolver, and
/// return it pinned to the approved addresses.
pub async fn check(raw: &str) -> Result<GuardedUrl, NetGuardError> {
    check_with(raw, |host, port| async move {
        tokio::net::lookup_host((host.as_str(), port))
            .await
            .map(|it| it.map(|sa| sa.ip()).collect::<Vec<_>>())
            .unwrap_or_default()
    })
    .await
}

/// [`check`] with an injected resolver (`host, port -> addresses`), so the
/// DNS cases — private answer, mixed answer, no answer — are testable.
pub async fn check_with<F, Fut>(raw: &str, resolve: F) -> Result<GuardedUrl, NetGuardError>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = Vec<IpAddr>>,
{
    let url = url::Url::parse(raw.trim()).map_err(|e| NetGuardError::Parse(e.to_string()))?;
    let scheme = url.scheme().to_string();
    match scheme.as_str() {
        "https" => {}
        "http" if allow_http() => {}
        other => return Err(NetGuardError::Scheme(other.to_string())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetGuardError::Userinfo);
    }
    let port = url.port_or_known_default().ok_or(NetGuardError::Port(0))?;
    if !port_allowed(&scheme, port) {
        return Err(NetGuardError::Port(port));
    }
    let host = url.host_str().ok_or(NetGuardError::NoHost)?.to_string();

    let ips: Vec<IpAddr> = match url.host() {
        Some(url::Host::Ipv4(v4)) => vec![IpAddr::V4(v4)],
        Some(url::Host::Ipv6(v6)) => vec![IpAddr::V6(v6)],
        Some(url::Host::Domain(d)) => resolve(d.to_string(), port).await,
        None => return Err(NetGuardError::NoHost),
    };
    if ips.is_empty() {
        return Err(NetGuardError::Unresolvable(host));
    }
    for a in &ips {
        if let Some(class) = forbidden_class(*a) {
            return Err(NetGuardError::Forbidden { addr: *a, class });
        }
    }
    let addrs = ips
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect();
    Ok(GuardedUrl { url, host, addrs })
}

/// A client that can reach only `g`'s approved addresses: DNS for its host is
/// overridden with them, redirects are refused, proxies are ignored.
///
/// reqwest looks the override up by exact host string; `g.host` is taken from
/// the same parsed URL the caller must send (`g.as_str()`), so they agree.
pub fn client_for(g: &GuardedUrl, timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .resolve_to_addrs(&g.host, &g.addrs)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent("AgentBestiary/1.0 (+https://agent-bestiary.world)")
        .build()
        .map_err(|e| format!("http client build failed: {e}"))
}

/// Check, then build the pinned client: the one-call form for call sites.
/// Call sites MUST send to `GuardedUrl::as_str()`, not the raw string.
pub async fn guarded_client(
    raw: &str,
    timeout: Duration,
) -> Result<(GuardedUrl, reqwest::Client), String> {
    let g = check(raw).await.map_err(|e| e.to_string())?;
    let c = client_for(&g, timeout)?;
    Ok((g, c))
}

/// A 3xx from a guarded client is an error naming where it tried to go
/// (redirects are never followed, so a caller must not read it as success).
pub fn refuse_redirect(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    if resp.status().is_redirection() {
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<none>");
        return Err(format!(
            "refused redirect (HTTP {}) to {loc}",
            resp.status().as_u16()
        ));
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_non_public_addresses() {
        for (ip, want) in [
            ("127.0.0.1", true),
            ("0.0.0.0", true),
            ("0.1.2.3", true),
            ("10.0.0.1", true),
            ("172.16.5.4", true),
            ("192.168.1.1", true),
            ("169.254.169.254", true),
            ("100.64.0.1", true),
            ("100.127.255.255", true),
            ("192.0.0.170", true),
            ("192.88.99.1", true),
            ("198.18.0.1", true),
            ("224.0.0.1", true),
            ("255.255.255.255", true),
            ("240.0.0.1", true),
            ("::1", true),
            ("::", true),
            ("fe80::1", true),
            ("fec0::1", true),
            ("fd00::1", true),
            ("ff02::1", true),
            ("::ffff:10.0.0.1", true),
            ("::ffff:169.254.169.254", true),
            ("::ffff:0:a00:1", true),
            ("::127.0.0.1", true),
            ("64:ff9b::a9fe:a9fe", true),
            ("64:ff9b:1::1", true),
            ("2002:7f00:1::", true),
            ("2002:a00:1::", true),
            ("2001:0:4136:e378::1", true),
            ("100::1", true),
            ("2001:db8::1", true),
            ("3fff::1", true),
            ("8.8.8.8", false),
            ("1.1.1.1", false),
            ("100.63.255.255", false),
            ("100.128.0.1", false),
            ("2606:4700:4700::1111", false),
            ("::ffff:8.8.8.8", false),
            ("2002:808:808::", false),
            ("64:ff9b::808:808", false),
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
    async fn rejects_forbidden_urls_without_dns() {
        for u in [
            "http://127.0.0.1/",
            "https://127.0.0.1/",
            "https://2130706433/",
            "https://0x7f.1/",
            "https://127.1/",
            "https://[::1]/",
            "https://169.254.169.254/latest/meta-data/",
            "https://[::ffff:10.0.0.1]/",
            "https://user:pass@example.com/",
            "https://example.com:8080/",
            "ftp://example.com/",
            "file:///etc/passwd",
            "not a url",
        ] {
            let r = check_with(u, |_, _| async { vec!["8.8.8.8".parse().unwrap()] }).await;
            assert!(r.is_err(), "{u} must be rejected, got {r:?}");
        }
    }

    /// Stub DNS (SPEC 36 R0.3 done-when): a name resolving to loopback, to a
    /// mix of public and private, or to nothing, is refused; a public-only
    /// name passes and is pinned to every address it resolved to.
    #[tokio::test]
    async fn dns_answers_are_checked_with_a_stub_resolver() {
        let to = |ips: &'static [&'static str]| {
            move |_: String, _: u16| async move {
                ips.iter()
                    .map(|s| s.parse::<IpAddr>().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        let private = check_with("https://rebind.test/", to(&["127.0.0.1"])).await;
        assert!(
            matches!(private, Err(NetGuardError::Forbidden { .. })),
            "{private:?}"
        );
        let mixed = check_with("https://mixed.test/", to(&["93.184.216.34", "10.0.0.5"])).await;
        assert!(
            matches!(mixed, Err(NetGuardError::Forbidden { .. })),
            "{mixed:?}"
        );
        let none = check_with("https://nx.test/", to(&[])).await;
        assert!(
            matches!(none, Err(NetGuardError::Unresolvable(_))),
            "{none:?}"
        );
        let ok = check_with(
            "https://ok.test/",
            to(&["93.184.216.34", "2606:2800:220:1::1"]),
        )
        .await
        .unwrap();
        assert_eq!(ok.addrs().len(), 2, "pinned to every approved address");
    }

    #[test]
    fn plain_http_is_refused_by_default() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let r = rt.block_on(check_with("http://example.com/", |_, _| async { vec![] }));
        assert_eq!(r.unwrap_err(), NetGuardError::Scheme("http".into()));
    }

    /// `client_for` end to end: it connects to the pinned address (not
    /// whatever the system resolver says for the name), and a redirect comes
    /// back unfollowed and becomes an error carrying its Location.
    #[tokio::test]
    async fn client_for_pins_and_refuses_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local = l.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _read = s.read(&mut buf).await;
            let _write = s
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/x\r\nContent-Length: 0\r\n\r\n")
                .await;
        });
        // Built directly (the test listener is on loopback, which `check`
        // would refuse); the name does not exist in DNS, so reaching the
        // listener at all proves the pin is what routed the connection.
        let g = GuardedUrl {
            url: url::Url::parse(&format!("http://guard-pin.invalid:{}/", local.port())).unwrap(),
            host: "guard-pin.invalid".into(),
            addrs: vec![local],
        };
        let c = client_for(&g, Duration::from_secs(5)).unwrap();
        let resp = c
            .get(g.as_str())
            .send()
            .await
            .expect("pinned connection reached the listener");
        assert_eq!(
            resp.status().as_u16(),
            302,
            "redirect returned, not followed"
        );
        let err = refuse_redirect(resp).unwrap_err();
        assert!(err.contains("169.254.169.254"), "{err}");
    }
}
