//! Asks a home router to forward a server's port, on a thread of its own: PCP, then NAT-PMP, at
//! the default gateway, then UPnP-IGD. The decisions are in (13-net-stack/port-map.md).

mod control_schema;
mod error;
mod facing_socket;
mod gateway;
mod listening;
mod nat_pmp_map;
mod pcp_map;
mod port_map;
mod port_mapper;
mod reply_error;
mod router_description;
mod router_url;
mod router_xml;
mod soap_call;
mod ssdp_announcement;
mod ssdp_headers;
mod ssdp_search;
mod stop;
mod upnp_gateway;

pub use crate::port_map::PortMap;
