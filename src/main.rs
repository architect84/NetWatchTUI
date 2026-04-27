mod connections;
mod display;
mod geoip;
mod resolver;
mod tui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

use connections::{is_local_ip, ConnectionFilter};
use display::DisplayRow;
use geoip::GeoIp;
use resolver::Resolver;

#[derive(Parser)]
#[command(
    name = "netwatch",
    about = "NetWatch — View active network connections with GeoIP geolocation",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to a MaxMind-format .mmdb GeoIP database file
    #[arg(long, value_name = "FILE")]
    db: Option<PathBuf>,

    /// Show only TCP connections
    #[arg(long)]
    tcp: bool,

    /// Show only UDP connections
    #[arg(long)]
    udp: bool,

    /// Show only IPv4 connections
    #[arg(long, name = "ipv4")]
    ipv4_only: bool,

    /// Show only IPv6 connections
    #[arg(long, name = "ipv6")]
    ipv6_only: bool,

    /// Resolve remote IP addresses to hostnames via reverse DNS
    #[arg(short, long, default_value_t = true)]
    resolve: bool,

    /// Skip DNS resolution for faster output
    #[arg(long)]
    no_resolve: bool,

    /// Show only established connections
    #[arg(short, long)]
    established: bool,

    /// Print output once and exit (no interactive TUI)
    #[arg(long)]
    once: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Download the free IPLocate GeoIP database
    DownloadDb {
        /// Destination path for the database file
        #[arg(long, value_name = "FILE")]
        dest: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Handle subcommands
    if let Some(Commands::DownloadDb { dest }) = &cli.command {
        let dest = dest.clone().unwrap_or_else(geoip::default_db_path);
        if let Err(e) = geoip::download_db(&dest) {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
        return;
    }

    // Determine DB path
    let db_path = cli.db.clone().unwrap_or_else(geoip::default_db_path);

    if !db_path.exists() {
        eprintln!(
            "GeoIP database not found at: {}\n\
             Run `netwatch download-db` to fetch the free IPLocate database,\n\
             or specify a path with --db <FILE>.",
            db_path.display()
        );
        process::exit(1);
    }

    // Load GeoIP database
    let geoip = match GeoIp::open(&db_path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let do_resolve = !cli.no_resolve;

    if cli.once {
        // Single-shot table output (original behavior)
        run_once(&cli, geoip, do_resolve);
    } else {
        // Interactive TUI (default)
        if let Err(e) = tui::run(geoip, do_resolve, cli.established) {
            eprintln!("TUI error: {}", e);
            process::exit(1);
        }
    }
}

fn run_once(cli: &Cli, geoip: GeoIp, do_resolve: bool) {
    let filter = build_filter(cli);

    let conns = match connections::get_connections(&filter) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error fetching connections: {}", e);
            eprintln!("Hint: You may need to run with sudo for full PID information.");
            process::exit(1);
        }
    };

    let mut dns = Resolver::new();

    let rows: Vec<DisplayRow> = conns
        .into_iter()
        .map(|conn| {
            let (hostname, geo) = match conn.remote_addr {
                Some(ref addr) if !is_local_ip(addr) => {
                    let hostname = if do_resolve {
                        dns.resolve(addr)
                    } else {
                        None
                    };
                    let geo = geoip.lookup(addr);
                    (hostname, geo)
                }
                _ => (None, geoip::GeoInfo::default()),
            };

            DisplayRow {
                connection: conn,
                hostname,
                geo,
            }
        })
        .collect();

    display::print_table(&rows);
}

fn build_filter(cli: &Cli) -> ConnectionFilter {
    let (tcp, udp) = match (cli.tcp, cli.udp) {
        (false, false) => (true, true),
        (t, u) => (t, u),
    };

    let (ipv4, ipv6) = match (cli.ipv4_only, cli.ipv6_only) {
        (false, false) => (true, true),
        (v4, v6) => (v4, v6),
    };

    ConnectionFilter {
        tcp,
        udp,
        ipv4,
        ipv6,
        established_only: cli.established,
    }
}
