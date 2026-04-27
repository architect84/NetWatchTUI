use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, SocketInfo,
    TcpSocketInfo, UdpSocketInfo,
};
use std::net::IpAddr;

/// Represents a single network connection with all relevant metadata.
#[derive(Debug, Clone)]
pub struct Connection {
    pub protocol: Protocol,
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub state: String,
    pub pids: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Udp => write!(f, "UDP"),
        }
    }
}

/// Filter options for which connections to retrieve.
pub struct ConnectionFilter {
    pub tcp: bool,
    pub udp: bool,
    pub ipv4: bool,
    pub ipv6: bool,
    pub established_only: bool,
}

impl Default for ConnectionFilter {
    fn default() -> Self {
        Self {
            tcp: true,
            udp: true,
            ipv4: true,
            ipv6: true,
            established_only: false,
        }
    }
}

/// Fetch all active network connections matching the given filter.
pub fn get_connections(filter: &ConnectionFilter) -> Result<Vec<Connection>, String> {
    let mut af_flags = AddressFamilyFlags::empty();
    if filter.ipv4 {
        af_flags |= AddressFamilyFlags::IPV4;
    }
    if filter.ipv6 {
        af_flags |= AddressFamilyFlags::IPV6;
    }

    let mut proto_flags = ProtocolFlags::empty();
    if filter.tcp {
        proto_flags |= ProtocolFlags::TCP;
    }
    if filter.udp {
        proto_flags |= ProtocolFlags::UDP;
    }

    if af_flags.is_empty() || proto_flags.is_empty() {
        return Ok(Vec::new());
    }

    let sockets = get_sockets_info(af_flags, proto_flags).map_err(|e| format!("{}", e))?;

    let mut connections: Vec<Connection> = sockets.into_iter().map(|si| from_socket_info(si)).collect();

    if filter.established_only {
        connections.retain(|c| c.state.to_uppercase() == "ESTABLISHED");
    }

    Ok(connections)
}

fn from_socket_info(si: SocketInfo) -> Connection {
    let pids: Vec<u32> = si.associated_pids;

    match si.protocol_socket_info {
        ProtocolSocketInfo::Tcp(tcp) => from_tcp(tcp, pids),
        ProtocolSocketInfo::Udp(udp) => from_udp(udp, pids),
    }
}

fn from_tcp(tcp: TcpSocketInfo, pids: Vec<u32>) -> Connection {
    let remote_addr = Some(tcp.remote_addr);
    let remote_port = Some(tcp.remote_port);

    Connection {
        protocol: Protocol::Tcp,
        local_addr: tcp.local_addr,
        local_port: tcp.local_port,
        remote_addr,
        remote_port,
        state: format!("{}", tcp.state),
        pids,
    }
}

fn from_udp(udp: UdpSocketInfo, pids: Vec<u32>) -> Connection {
    Connection {
        protocol: Protocol::Udp,
        local_addr: udp.local_addr,
        local_port: udp.local_port,
        remote_addr: None,
        remote_port: None,
        state: String::from("-"),
        pids,
    }
}

/// Check if an IP address is a loopback or link-local address.
pub fn is_local_ip(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}
