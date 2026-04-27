use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState};

use crate::connections::{self, is_local_ip, Connection, ConnectionFilter};
use crate::geoip::{GeoInfo, GeoIp};
use crate::resolver::Resolver;

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Column identifiers for sorting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Proto,
    LocalAddr,
    RemoteAddr,
    State,
    Pid,
    Hostname,
    Country,
}

const COLUMNS: &[SortColumn] = &[
    SortColumn::Proto,
    SortColumn::LocalAddr,
    SortColumn::RemoteAddr,
    SortColumn::State,
    SortColumn::Pid,
    SortColumn::Hostname,
    SortColumn::Country,
];

struct EnrichedConnection {
    conn: Connection,
    hostname: Option<String>,
    geo: GeoInfo,
}

pub struct App {
    geoip: GeoIp,
    resolver: Resolver,
    rows: Vec<EnrichedConnection>,
    table_state: TableState,
    // Filters
    show_tcp: bool,
    show_udp: bool,
    show_ipv4: bool,
    show_ipv6: bool,
    established_only: bool,
    // Sort
    sort_col: SortColumn,
    sort_ascending: bool,
    // UI state
    should_quit: bool,
    do_resolve: bool,
    last_refresh: Instant,
    status_msg: String,
}

impl App {
    pub fn new(geoip: GeoIp, do_resolve: bool, established_only: bool) -> Self {
        Self {
            geoip,
            resolver: Resolver::new(),
            rows: Vec::new(),
            table_state: TableState::default(),
            show_tcp: true,
            show_udp: true,
            show_ipv4: true,
            show_ipv6: true,
            established_only,
            sort_col: SortColumn::State,
            sort_ascending: true,
            should_quit: false,
            do_resolve,
            last_refresh: Instant::now() - REFRESH_INTERVAL,
            status_msg: String::new(),
        }
    }

    fn refresh_connections(&mut self) {
        let filter = ConnectionFilter {
            tcp: self.show_tcp,
            udp: self.show_udp,
            ipv4: self.show_ipv4,
            ipv6: self.show_ipv6,
            established_only: self.established_only,
        };

        let conns = match connections::get_connections(&filter) {
            Ok(c) => c,
            Err(e) => {
                self.status_msg = format!("Error: {}", e);
                return;
            }
        };

        self.rows = conns
            .into_iter()
            .map(|conn| {
                let (hostname, geo) = match conn.remote_addr {
                    Some(ref addr) if !is_local_ip(addr) => {
                        let hostname = if self.do_resolve {
                            self.resolver.resolve(addr)
                        } else {
                            None
                        };
                        let geo = self.geoip.lookup(addr);
                        (hostname, geo)
                    }
                    _ => (None, GeoInfo::default()),
                };
                EnrichedConnection {
                    conn,
                    hostname,
                    geo,
                }
            })
            .collect();

        self.sort_rows();
        self.status_msg = format!("{} connections", self.rows.len());
        self.last_refresh = Instant::now();
    }

    fn sort_rows(&mut self) {
        let col = self.sort_col;
        let asc = self.sort_ascending;

        self.rows.sort_by(|a, b| {
            let cmp = match col {
                SortColumn::Proto => a.conn.protocol.to_string().cmp(&b.conn.protocol.to_string()),
                SortColumn::LocalAddr => {
                    let ak = (a.conn.local_addr, a.conn.local_port);
                    let bk = (b.conn.local_addr, b.conn.local_port);
                    format!("{:?}", ak).cmp(&format!("{:?}", bk))
                }
                SortColumn::RemoteAddr => {
                    let ak = format!("{:?}{:?}", a.conn.remote_addr, a.conn.remote_port);
                    let bk = format!("{:?}{:?}", b.conn.remote_addr, b.conn.remote_port);
                    ak.cmp(&bk)
                }
                SortColumn::State => a.conn.state.cmp(&b.conn.state),
                SortColumn::Pid => {
                    let ap = a.conn.pids.first().unwrap_or(&0);
                    let bp = b.conn.pids.first().unwrap_or(&0);
                    ap.cmp(bp)
                }
                SortColumn::Hostname => {
                    let ah = a.hostname.as_deref().unwrap_or("");
                    let bh = b.hostname.as_deref().unwrap_or("");
                    ah.cmp(bh)
                }
                SortColumn::Country => {
                    let ac = a.geo.country_code.as_deref().unwrap_or("");
                    let bc = b.geo.country_code.as_deref().unwrap_or("");
                    ac.cmp(bc)
                }
            };
            if asc { cmp } else { cmp.reverse() }
        });
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // Navigation
            KeyCode::Down | KeyCode::Char('j') => self.next_row(),
            KeyCode::Up | KeyCode::Char('k') => self.prev_row(),
            KeyCode::Home | KeyCode::Char('g') => self.table_state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => {
                if !self.rows.is_empty() {
                    self.table_state.select(Some(self.rows.len() - 1));
                }
            }
            // Filter toggles
            KeyCode::Char('t') => { self.show_tcp = !self.show_tcp; self.force_refresh(); }
            KeyCode::Char('u') => { self.show_udp = !self.show_udp; self.force_refresh(); }
            KeyCode::Char('4') => { self.show_ipv4 = !self.show_ipv4; self.force_refresh(); }
            KeyCode::Char('6') => { self.show_ipv6 = !self.show_ipv6; self.force_refresh(); }
            KeyCode::Char('e') => { self.established_only = !self.established_only; self.force_refresh(); }
            KeyCode::Char('d') => { self.do_resolve = !self.do_resolve; self.force_refresh(); }
            // Sort by column number (1-7)
            KeyCode::Char(c @ '1'..='7') => {
                let idx = (c as u8 - b'1') as usize;
                let col = COLUMNS[idx];
                if self.sort_col == col {
                    self.sort_ascending = !self.sort_ascending;
                } else {
                    self.sort_col = col;
                    self.sort_ascending = true;
                }
                self.sort_rows();
            }
            // Manual refresh
            KeyCode::Char('r') => self.force_refresh(),
            _ => {}
        }
    }

    fn force_refresh(&mut self) {
        self.last_refresh = Instant::now() - REFRESH_INTERVAL;
    }

    fn next_row(&mut self) {
        if self.rows.is_empty() { return; }
        let i = match self.table_state.selected() {
            Some(i) if i >= self.rows.len() - 1 => i,
            Some(i) => i + 1,
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn prev_row(&mut self) {
        let i = match self.table_state.selected() {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.table_state.select(Some(i));
    }
}

pub fn run(geoip: GeoIp, do_resolve: bool, established_only: bool) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut app = App::new(geoip, do_resolve, established_only);
    app.refresh_connections();
    if !app.rows.is_empty() {
        app.table_state.select(Some(0));
    }

    loop {
        // Auto-refresh
        if app.last_refresh.elapsed() >= REFRESH_INTERVAL {
            app.refresh_connections();
        }

        terminal.draw(|f| ui(f, &mut app))?;

        // Poll for events with a short timeout so auto-refresh works
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header / filters
            Constraint::Min(5),    // table
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // Header with filter status
    render_header(f, layout[0], app);

    // Connection table
    render_table(f, layout[1], app);

    // Status bar
    render_status(f, layout[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let tcp_style = if app.show_tcp { Style::default().fg(Color::Green).bold() } else { Style::default().fg(Color::DarkGray) };
    let udp_style = if app.show_udp { Style::default().fg(Color::Green).bold() } else { Style::default().fg(Color::DarkGray) };
    let v4_style = if app.show_ipv4 { Style::default().fg(Color::Green).bold() } else { Style::default().fg(Color::DarkGray) };
    let v6_style = if app.show_ipv6 { Style::default().fg(Color::Green).bold() } else { Style::default().fg(Color::DarkGray) };
    let est_style = if app.established_only { Style::default().fg(Color::Yellow).bold() } else { Style::default().fg(Color::DarkGray) };
    let dns_style = if app.do_resolve { Style::default().fg(Color::Green).bold() } else { Style::default().fg(Color::DarkGray) };

    let header = Line::from(vec![
        Span::raw(" Filters: "),
        Span::styled("[t]", tcp_style), Span::styled("CP ", tcp_style),
        Span::styled("[u]", udp_style), Span::styled("DP ", udp_style),
        Span::styled("IPv[4] ", v4_style),
        Span::styled("IPv[6] ", v6_style),
        Span::raw("  "),
        Span::styled("[e]", est_style), Span::styled("stablished ", est_style),
        Span::styled("[d]", dns_style), Span::styled("ns ", dns_style),
        Span::raw("  "),
        Span::styled("[r]efresh  [q]uit", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .title(" NetWatch ")
        .title_style(Style::default().fg(Color::Cyan).bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(header).block(block);
    f.render_widget(para, area);
}

fn render_table(f: &mut Frame, area: Rect, app: &mut App) {
    let header_titles = ["Proto", "Local Address", "Remote Address", "State", "PID", "Hostname", "Country"];

    let header_cells: Vec<Cell> = header_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let col = COLUMNS[i];
            let arrow = if app.sort_col == col {
                if app.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            };
            let label = format!("{}{}{}", i + 1, ".", title);
            let text = format!("{}{}", label, arrow);
            if app.sort_col == col {
                Cell::from(text).style(Style::default().fg(Color::Cyan).bold())
            } else {
                Cell::from(text).style(Style::default().fg(Color::White).bold())
            }
        })
        .collect();

    let header = Row::new(header_cells).height(1).bottom_margin(0);

    let rows: Vec<Row> = app
        .rows
        .iter()
        .map(|r| {
            let conn = &r.conn;
            let local = format!("{}:{}", conn.local_addr, conn.local_port);
            let remote = match (conn.remote_addr, conn.remote_port) {
                (Some(addr), Some(port)) => format!("{}:{}", addr, port),
                _ => String::from("*:*"),
            };
            let pids = if conn.pids.is_empty() {
                String::from("-")
            } else {
                conn.pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
            };
            let hostname = r.hostname.as_deref().unwrap_or("-");
            let country = r.geo.country_code.as_deref().unwrap_or("-");

            let state_style = match conn.state.to_uppercase().as_str() {
                "ESTABLISHED" => Style::default().fg(Color::Green),
                "LISTEN" => Style::default().fg(Color::Cyan),
                "TIME_WAIT" | "TIMEWAIT" | "CLOSE_WAIT" | "CLOSEWAIT" => Style::default().fg(Color::Yellow),
                "CLOSE" | "CLOSING" | "CLOSED" => Style::default().fg(Color::Red),
                "SYN_SENT" | "SYN_RECV" => Style::default().fg(Color::Yellow),
                _ => Style::default(),
            };

            Row::new(vec![
                Cell::from(conn.protocol.to_string()),
                Cell::from(local),
                Cell::from(remote),
                Cell::from(conn.state.clone()).style(state_style),
                Cell::from(pids),
                Cell::from(hostname.to_string()),
                Cell::from(country.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Min(15),
        Constraint::Min(15),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Min(15),
        Constraint::Length(7),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut app.table_state);

    // Scrollbar
    if !app.rows.is_empty() {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(app.rows.len())
            .position(app.table_state.selected().unwrap_or(0));
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let sort_name = match app.sort_col {
        SortColumn::Proto => "Proto",
        SortColumn::LocalAddr => "Local",
        SortColumn::RemoteAddr => "Remote",
        SortColumn::State => "State",
        SortColumn::Pid => "PID",
        SortColumn::Hostname => "Host",
        SortColumn::Country => "Country",
    };
    let dir = if app.sort_ascending { "▲" } else { "▼" };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", app.status_msg),
            Style::default().fg(Color::White),
        ),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" Sort: {}{} ", sort_name, dir),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" ↑↓/jk:navigate  1-7:sort  "),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let bar = Paragraph::new(line).style(Style::default().bg(Color::Black));
    f.render_widget(bar, area);
}
