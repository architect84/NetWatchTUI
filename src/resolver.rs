use dns_lookup::lookup_addr;
use std::collections::HashMap;
use std::net::IpAddr;

/// Caching reverse DNS resolver.
pub struct Resolver {
    cache: HashMap<IpAddr, Option<String>>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Resolve an IP address to a hostname.
    /// Returns None if resolution fails or times out.
    pub fn resolve(&mut self, addr: &IpAddr) -> Option<String> {
        if let Some(cached) = self.cache.get(addr) {
            return cached.clone();
        }

        let result = lookup_addr(addr).ok();

        // Don't cache if the result is just the IP address string back
        let result = result.filter(|name| name != &addr.to_string());

        self.cache.insert(*addr, result.clone());
        result
    }
}
