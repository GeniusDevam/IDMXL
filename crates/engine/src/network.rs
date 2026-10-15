//! Browser handoffs cannot use the desktop's explicit intranet access privilege.
use anyhow::{Result, bail};
use curl::easy::{Easy2, Handler, List};
use std::net::{IpAddr, ToSocketAddrs};
use url::Host;

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || a == 100 && (64..=127).contains(&b)
                || a == 169 && b == 254
                || a == 172 && (16..=31).contains(&b)
                || a == 192 && (b == 168 || b == 0 || b == 2)
                || a == 198 && (b == 18 || b == 19 || b == 51 && c == 100)
                || a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8)
                    || s[0] == 0x2002
                    || s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

/// Validate every answer, then pin libcurl to that exact set for this request.
/// The caller repeats this for every redirect and every new request.
pub fn pin_public<H: Handler>(easy: &mut Easy2<H>, source: &str) -> Result<()> {
    let url = crate::platform::checked_url(source)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("invalid_url"))?;
    match url.host().ok_or_else(|| anyhow::anyhow!("invalid_url"))? {
        Host::Ipv4(ip) => {
            if !is_public(ip.into()) {
                bail!("private_network_blocked");
            }
        }
        Host::Ipv6(ip) => {
            if !is_public(ip.into()) {
                bail!("private_network_blocked");
            }
        }
        Host::Domain(host) => {
            let mut addresses: Vec<_> = (host, port)
                .to_socket_addrs()
                .map_err(|_| anyhow::anyhow!("dns_resolution_failed"))?
                .map(|n| n.ip())
                .collect();
            addresses.sort();
            addresses.dedup();
            if addresses.is_empty()
                || addresses.len() > 32
                || addresses.iter().any(|ip| !is_public(*ip))
            {
                bail!("private_network_blocked");
            }
            let ips = addresses
                .iter()
                .map(|ip| match ip {
                    IpAddr::V4(ip) => ip.to_string(),
                    IpAddr::V6(ip) => format!("[{ip}]"),
                })
                .collect::<Vec<_>>()
                .join(",");
            let mut list = List::new();
            list.append(&format!("{host}:{port}:{ips}"))?;
            easy.resolve(list)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_internal_and_special_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.1.1",
            "172.31.4.2",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.1.2.3",
            "192.0.2.1",
            "::1",
            "::ffff:8.8.8.8",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002:0808:0808::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(is_public(ip.parse().unwrap()));
        }
    }
}
