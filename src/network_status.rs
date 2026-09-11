use serde::Serialize;
use std::{collections::BTreeMap, net::IpAddr, process::Command};

#[derive(Clone, Debug, Default, Serialize)]
pub struct NetworkStatus {
    pub interfaces: BTreeMap<String, InterfaceStatus>,
    pub routes: Vec<String>,
    pub ipv6_routes: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct InterfaceStatus {
    pub address: String,
    pub state: String,
    pub statistics: String,
}

pub fn snapshot() -> NetworkStatus {
    let mut status = NetworkStatus::default();
    for line in output(&["ip", "-o", "link", "show"]).lines() {
        let Some((index, rest)) = line.split_once(": ") else {
            continue;
        };
        let Some((name, details)) = rest.split_once(':') else {
            continue;
        };
        let name = name
            .trim()
            .split('@')
            .next()
            .unwrap_or(name.trim())
            .to_owned();
        let state = details.trim().to_owned();
        status.interfaces.insert(
            name,
            InterfaceStatus {
                address: index.into(),
                state,
                statistics: String::new(),
            },
        );
    }
    status.routes = output(&["ip", "route", "show"])
        .lines()
        .map(str::to_owned)
        .collect();
    status.ipv6_routes = output(&["ip", "-6", "route", "show"])
        .lines()
        .map(str::to_owned)
        .collect();
    status
}
pub fn has_wan_address() -> bool {
    output(&["ip", "-o", "addr", "show", "dev", "wwan0"])
        .lines()
        .filter_map(|line| line.split_whitespace().nth(3))
        .filter_map(|address| address.split('/').next())
        .filter_map(|address| address.parse::<IpAddr>().ok())
        .any(is_routable_address)
}

fn is_routable_address(ip: IpAddr) -> bool {
    !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_multicast()
        && match ip {
            IpAddr::V4(ip) => !ip.is_link_local(),
            IpAddr::V6(ip) => !ip.is_unicast_link_local(),
        }
}

fn output(args: &[&str]) -> String {
    Command::new(args[0])
        .args(&args[1..])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}
