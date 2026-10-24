use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    redirect::Policy,
    Url,
};
use std::{
    error::Error,
    io,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
};

type ResolveError = Box<dyn Error + Send + Sync>;

pub(super) struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::task::spawn_blocking(move || {
                (host.as_str(), 0)
                    .to_socket_addrs()
                    .map(|items| items.collect::<Vec<_>>())
            })
            .await
            .map_err(|error| Box::new(error) as ResolveError)?
            .map_err(|error| Box::new(error) as ResolveError)?;

            validate_resolved_addresses(&addresses)
                .map_err(|error| Box::new(error) as ResolveError)?;
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

pub(super) fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            attempt.error("资讯请求重定向次数过多")
        } else if validate_public_https_url(attempt.url()).is_ok() {
            attempt.follow()
        } else {
            attempt.error("资讯请求只允许公开 HTTPS 地址")
        }
    })
}

pub(super) fn validate_public_https_url(url: &Url) -> Result<(), &'static str> {
    if url.scheme() != "https" {
        return Err("only HTTPS destinations are allowed");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL credentials are not allowed");
    }
    let host = url.host_str().ok_or("URL host is missing")?;
    let normalized = host
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if is_local_hostname(&normalized) {
        return Err("local hostnames are not allowed");
    }
    if let Ok(address) = normalized.parse::<IpAddr>() {
        if !is_public_ip(address) {
            return Err("non-public IP addresses are not allowed");
        }
    }
    Ok(())
}

fn is_local_hostname(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "local"
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".lan")
        || host == "home.arpa"
        || host.ends_with(".home.arpa")
}

fn validate_resolved_addresses(addresses: &[SocketAddr]) -> io::Result<()> {
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "host did not resolve exclusively to public IP addresses",
        ));
    }
    Ok(())
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [first, second, third, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && first != 0
                && first < 240
                && !(first == 100 && (64..=127).contains(&second))
                && !(first == 192 && second == 0 && third == 0)
                && !(first == 192 && second == 88 && third == 99)
                && !(first == 198 && (18..=19).contains(&second))
        }
        IpAddr::V6(ip) => {
            let bytes = ip.octets();
            let global_unicast = bytes[0] & 0xe0 == 0x20;
            let second_segment = u16::from_be_bytes([bytes[2], bytes[3]]);
            let protocol_assignments =
                bytes[0] == 0x20 && bytes[1] == 0x01 && second_segment <= 0x01ff;
            let documentation =
                bytes[0] == 0x20 && bytes[1] == 0x01 && bytes[2] == 0x0d && bytes[3] == 0xb8;
            let six_to_four = bytes[0] == 0x20 && bytes[1] == 0x02;
            let ipv6_documentation = bytes[0] == 0x3f && bytes[1] == 0xff && bytes[2] & 0xf0 == 0;
            global_unicast
                && !protocol_assignments
                && !documentation
                && !six_to_four
                && !ipv6_documentation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_https_destinations_and_rejects_local_targets() {
        for value in [
            "https://example.com/feed",
            "https://93.184.216.34/feed",
            "https://[2001:4860:4860::8888]/feed",
            "https://[3fff:1000::1]/feed",
        ] {
            let url = Url::parse(value).unwrap();
            assert!(validate_public_https_url(&url).is_ok(), "{value}");
        }
        for value in [
            "http://example.com/feed",
            "https://user@example.com/feed",
            "https://localhost./feed",
            "https://service.internal/feed",
            "https://127.0.0.1/feed",
            "https://100.64.0.1/feed",
            "https://192.0.2.1/feed",
            "https://[::1]/feed",
            "https://[fd00::1]/feed",
            "https://[2001:2::1]/feed",
            "https://[2001:20::1]/feed",
            "https://[2001:db8::1]/feed",
            "https://[3fff::1]/feed",
            "https://[3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff]/feed",
        ] {
            let url = Url::parse(value).unwrap();
            assert!(validate_public_https_url(&url).is_err(), "{value}");
        }
    }

    #[test]
    fn accepts_only_dns_results_that_are_all_public() {
        let public = SocketAddr::from(([93, 184, 216, 34], 443));
        validate_resolved_addresses(&[public]).unwrap();
        assert!(validate_resolved_addresses(&[]).is_err());
        assert!(
            validate_resolved_addresses(&[public, SocketAddr::from(([10, 0, 0, 1], 443))]).is_err()
        );
    }
}
