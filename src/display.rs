use comfy_table::{Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};

use crate::connections::Connection;
use crate::geoip::GeoInfo;

/// A row of enriched connection data ready for display.
pub struct DisplayRow {
    pub connection: Connection,
    pub hostname: Option<String>,
    pub geo: GeoInfo,
}

/// Build and print a formatted table of connections.
pub fn print_table(rows: &[DisplayRow]) {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);

    table.set_header(vec![
        Cell::new("Proto").add_attribute(Attribute::Bold),
        Cell::new("Local Address").add_attribute(Attribute::Bold),
        Cell::new("Remote Address").add_attribute(Attribute::Bold),
        Cell::new("State").add_attribute(Attribute::Bold),
        Cell::new("PID").add_attribute(Attribute::Bold),
        Cell::new("Hostname").add_attribute(Attribute::Bold),
        Cell::new("Country").add_attribute(Attribute::Bold),
        Cell::new("City").add_attribute(Attribute::Bold),
    ]);

    for row in rows {
        let conn = &row.connection;

        let local = format!("{}:{}", conn.local_addr, conn.local_port);
        let remote = match (conn.remote_addr, conn.remote_port) {
            (Some(addr), Some(port)) => format!("{}:{}", addr, port),
            _ => String::from("*:*"),
        };

        let pids = if conn.pids.is_empty() {
            String::from("-")
        } else {
            conn.pids
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };

        let hostname = row.hostname.as_deref().unwrap_or("-");
        let country = match (&row.geo.country_name, &row.geo.country_code) {
            (Some(name), Some(code)) => format!("{} ({})", name, code),
            (Some(name), None) => name.clone(),
            (None, Some(code)) => code.clone(),
            (None, None) => String::from("-"),
        };
        let city = row.geo.city.as_deref().unwrap_or("-");

        let state_cell = colorized_state(&conn.state);

        table.add_row(vec![
            Cell::new(&conn.protocol.to_string()),
            Cell::new(&local),
            Cell::new(&remote),
            state_cell,
            Cell::new(&pids).set_alignment(CellAlignment::Right),
            Cell::new(truncate(hostname, 30)),
            Cell::new(&country),
            Cell::new(city),
        ]);
    }

    println!("{table}");
    println!("\n{} connections shown.", rows.len());
}

fn colorized_state(state: &str) -> Cell {
    let s = state.to_uppercase();
    let color = match s.as_str() {
        "ESTABLISHED" => Some(Color::Green),
        "LISTEN" => Some(Color::Cyan),
        "TIME_WAIT" | "TIMEWAIT" | "CLOSE_WAIT" | "CLOSEWAIT" => Some(Color::Yellow),
        "CLOSE" | "CLOSING" | "CLOSED" => Some(Color::Red),
        "SYN_SENT" | "SYNSENT" | "SYN_RECV" | "SYNRECV" => Some(Color::Yellow),
        _ => None,
    };

    let mut cell = Cell::new(state);
    if let Some(c) = color {
        cell = cell.fg(c);
    }
    cell
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
