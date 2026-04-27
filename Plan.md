# NetWatch — Linux Network Connection Viewer with GeoIP
## Problem
Build a native Linux CLI application that:
1. Lists all active network connections (TCP/UDP, IPv4/IPv6)
2. Resolves IP addresses to hostnames via reverse DNS
3. Looks up geolocation data (country, city) for each remote IP
## Current State
* **Toolchain**: GCC and Python3 are available. Rust/Cargo are **not installed** — will need `rustup`.
* **Project directory**: `~/source/warp/NetWatch/` (created, empty).
## Language Choice: Rust
Rust produces a compiled native binary, has strong Linux networking crate support, and memory safety. Key crates:
* `netstat2` — cross-platform socket enumeration; uses `NETLINK_INET_DIAG` on Linux (same kernel interface as `ss`), fast and efficient. Returns local/remote addr+port, state, associated PIDs.
* `maxminddb` — reads MaxMind `.mmdb` format for GeoIP lookups. ~22M downloads, mature.
* `dns-lookup` — synchronous DNS reverse lookups (`getnameinfo`).
* `comfy-table` — terminal table formatting.
## GeoIP Database
Use **IPLocate.io's free IP-to-Country database** (MMDB format, CC BY-SA 4.0, no signup required, compatible with `maxminddb` crate). Alternatively, **MaxMind GeoLite2-City** provides city-level data but requires a free account signup.
The app will accept a `--db` flag pointing to a `.mmdb` file, and also ship a `download-db` subcommand to fetch the free IPLocate DB automatically.
## Proposed Architecture
### Project Structure
```warp-runnable-command
NetWatch/
├── Cargo.toml
├── src/
│   ├── main.rs        # CLI entry point, arg parsing
│   ├── connections.rs # Socket enumeration via netstat2
│   ├── resolver.rs    # Reverse DNS resolution
│   ├── geoip.rs       # GeoIP lookups via maxminddb
│   └── display.rs     # Table formatting and output
```
### Key Dependencies
* `netstat2 = "0.11"` — socket enumeration
* `maxminddb = { version = "0.27", features = ["mmap"] }` — GeoIP
* `dns-lookup = "2"` — reverse DNS
* `comfy-table = "7"` — terminal tables
* `clap = { version = "4", features = ["derive"] }` — CLI arg parsing
* `reqwest = { version = "0.12", features = ["blocking"] }` — DB download
### Core Flow
1. Parse CLI args (optional `--db` path, `--protocol` filter, `--resolve` toggle, `download-db` subcommand)
2. Load the MMDB file into a `maxminddb::Reader` (mmap'd)
3. Call `netstat2::get_sockets_info()` with desired address family and protocol flags
4. For each connection with a non-local remote IP:
   a. Optionally resolve the remote IP via `dns-lookup::lookup_addr()`
   b. Look up geolocation from the MMDB reader
5. Format and print a table:
```warp-runnable-command
Proto  Local Address          Remote Address         State        PID   Hostname            Country  City
TCP    192.168.1.10:43210     142.250.80.46:443      ESTABLISHED  1234  lax17s55-in-f14...  US       Los Angeles
```
### Filtering
* Skip loopback (127.x, ::1) and link-local IPs for geo lookups
* Filter by protocol (TCP/UDP) and address family (IPv4/IPv6) via CLI flags
### `download-db` Subcommand
Fetch the IPLocate MMDB from `https://iplocate.io/downloads/ip-to-country.mmdb` to a default location (`~/.local/share/netwatch/ip-to-country.mmdb`), which the main command uses if `--db` is not specified.
## Steps
1. Install Rust via `rustup`
2. Scaffold the project with `cargo init` inside `NetWatch/`
3. Implement each module (connections, resolver, geoip, display)
4. Wire up CLI with clap in `main.rs`
5. Build and test
