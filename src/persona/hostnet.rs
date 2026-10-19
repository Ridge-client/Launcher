//! This PC on its home network: its own address and its router's, for the protections and checks.

use std::net::{IpAddr, UdpSocket};

/// This PC's address on its home network: the one its internet traffic leaves from.
/// No packet is sent; connecting a UDP socket only asks the system which route it would take.
pub fn pc_address() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:53").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip.to_string()),
        _ => None,
    }
}

/// The home router: the default gateway in Windows' routing table.
pub fn router_address() -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("route")
            .args(["print", "-4", "0.0.0.0"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        parse_default_gateway(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(windows))]
    None
}

/// The gateway of the first `0.0.0.0 0.0.0.0 <gateway> <interface> <metric>` line of `route print`.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_default_gateway(route_print: &str) -> Option<String> {
    route_print.lines().find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        match cols.as_slice() {
            ["0.0.0.0", "0.0.0.0", gateway, ..]
                if gateway.parse::<std::net::Ipv4Addr>().is_ok() =>
            {
                Some(gateway.to_string())
            }
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_default_gateway() {
        let out = "IPv4 Route Table\n\
                   ===========================================================================\n\
                   Active Routes:\n\
                   Network Destination        Netmask          Gateway       Interface  Metric\n\
                   \x20         0.0.0.0          0.0.0.0   10.228.228.225   10.228.228.245     55\n";
        assert_eq!(
            parse_default_gateway(out).as_deref(),
            Some("10.228.228.225")
        );
        assert_eq!(parse_default_gateway("Active Routes:\n  None\n"), None);
    }
}
