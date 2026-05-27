use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
    },
};

use crate::analyzer::graph::{PortStat, Service, StatusKind, truncate};
use super::Renderer;

pub fn draw(f: &mut Frame, state: &Renderer) {
    let size = f.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(7),
        ])
        .split(size);

    draw_title(f, outer[0], state);
    draw_main(f, outer[1], state);
    draw_event_log(f, outer[2], state);
}

// ── Title bar ─────────────────────────────────────────────────────────────────

fn draw_title(f: &mut Frame, area: Rect, state: &Renderer) {
    let snap = state.last_snapshot.as_ref();
    let total = snap.map(|s| s.services.len()).unwrap_or(0);
    let nb = snap.map(|s| s.node_bun_count()).unwrap_or(0);
    let tap_active = snap.map(|s| s.tap_active()).unwrap_or(false);
    let node_only_label = if state.node_only { " [node/bun only]" } else { "" };

    let mut spans = vec![
        Span::styled("  arch-live ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("│ "),
        Span::styled(format!("services: {}", total), Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(format!("node/bun: {}", nb), Style::default().fg(Color::Green)),
        Span::raw("  "),
    ];

    if tap_active {
        let tap_port = snap.and_then(|s| s.tap_listen_port).unwrap_or(0);
        let tap_total = snap.map(|s| s.tap_total_requests()).unwrap_or(0);
        spans.push(Span::styled(
            format!("tap :{} │ requests: {}", tap_port, tap_total),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    } else {
        let total_inbound = snap.map(|s| s.total_inbound()).unwrap_or(0);
        spans.push(Span::styled(
            format!("inbound: {}", total_inbound),
            Style::default().fg(Color::Yellow),
        ));
    }

    spans.push(Span::styled(node_only_label, Style::default().fg(Color::Yellow)));
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        "q:quit  n:toggle  ↑↓:navigate  r:refresh",
        Style::default().fg(Color::DarkGray),
    ));

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))),
        area,
    );
}

// ── Main layout ───────────────────────────────────────────────────────────────

fn draw_main(f: &mut Frame, area: Rect, state: &Renderer) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(26), Constraint::Percentage(74)])
        .split(area);

    draw_services(f, cols[0], state);

    let tap_active = state.last_snapshot.as_ref().map(|s| s.tap_active()).unwrap_or(false);
    if tap_active {
        draw_tap_column(f, cols[1], state);
    } else {
        draw_connection_column(f, cols[1], state);
    }
}

// ── Right column when tap is active ──────────────────────────────────────────

fn draw_tap_column(f: &mut Frame, area: Rect, state: &Renderer) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    draw_http_request_log(f, rows[0], state);
    draw_path_stats(f, rows[1], state);
}

// ── Right column when no tap ──────────────────────────────────────────────────

fn draw_connection_column(f: &mut Frame, area: Rect, state: &Renderer) {
    let has_ports = state.last_snapshot.as_ref().map(|s| !s.port_stats.is_empty()).unwrap_or(false);
    if has_ports {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        draw_request_monitor(f, rows[0], state);
        draw_calls_table(f, rows[1], state);
    } else {
        draw_calls_table(f, area, state);
    }
}

// ── Service list ──────────────────────────────────────────────────────────────

fn draw_services(f: &mut Frame, area: Rect, state: &Renderer) {
    let block = Block::default()
        .title(" Services ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let services: Vec<&Service> = state
        .last_snapshot
        .as_ref()
        .map(|s| s.services.iter().filter(|svc| !state.node_only || svc.is_node_or_bun()).collect())
        .unwrap_or_default();

    let items: Vec<ListItem> = services.iter().map(|svc| {
        let badge_style = match svc.runtime {
            crate::collector::Runtime::Node => Style::default().fg(Color::Green),
            crate::collector::Runtime::Bun => Style::default().fg(Color::Magenta),
            crate::collector::Runtime::Other(_) => Style::default().fg(Color::DarkGray),
        };
        let name_style = if svc.is_node_or_bun() {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        ListItem::new(Line::from(vec![
            Span::styled(svc.runtime_badge(), badge_style),
            Span::raw(" "),
            Span::styled(truncate(&svc.name, 13), name_style),
            Span::styled(format!(" :{}", svc.ports_display()), Style::default().fg(Color::DarkGray)),
        ]))
    }).collect();

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.selected_idx.min(items.len().saturating_sub(1))));
    }

    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ "),
        area,
        &mut list_state,
    );

    render_service_detail(f, area, state, &services);
}

fn render_service_detail(f: &mut Frame, area: Rect, state: &Renderer, services: &[&Service]) {
    if services.is_empty() { return; }
    let detail_height = 4u16;
    if area.height <= detail_height + 4 { return; }
    let idx = state.selected_idx.min(services.len().saturating_sub(1));
    let svc = services[idx];
    let detail_area = Rect { x: area.x, y: area.y + area.height - detail_height, width: area.width, height: detail_height };
    let text = vec![
        Line::from(vec![Span::styled("pid:    ", Style::default().fg(Color::DarkGray)), Span::raw(svc.pid.to_string())]),
        Line::from(vec![Span::styled("script: ", Style::default().fg(Color::DarkGray)), Span::raw(truncate(svc.script.as_deref().unwrap_or("-"), 22))]),
        Line::from(vec![Span::styled("cwd:    ", Style::default().fg(Color::DarkGray)), Span::raw(truncate(svc.cwd.as_deref().unwrap_or("-"), 22))]),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().title(" Detail ").borders(Borders::TOP | Borders::LEFT | Borders::RIGHT).border_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

// ── HTTP Tap: request log ─────────────────────────────────────────────────────

fn draw_http_request_log(f: &mut Frame, area: Rect, state: &Renderer) {
    let tap_port = state.last_snapshot.as_ref().and_then(|s| s.tap_listen_port).unwrap_or(0);
    let block = Block::default()
        .title(format!(" ↓ HTTP Requests  (tap :{}) ", tap_port))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let snapshot = match &state.last_snapshot {
        Some(s) => s,
        None => { f.render_widget(Paragraph::new("Waiting for tap data…").block(block), area); return; }
    };

    if snapshot.http_log.is_empty() {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Tap is listening on :{} → :{}", tap_port,
                    state.args.tap.as_ref().map(|c| c.target_port).unwrap_or(0)),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Point your frontend at this port and make a request.",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        f.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    let header = Row::new([
        Cell::from("Method").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Path").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Status").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Time").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Duration").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    ]).height(1).bottom_margin(1);

    // Show most recent entries first
    let visible = (area.height as usize).saturating_sub(4); // account for header + borders
    let rows: Vec<Row> = snapshot.http_log.iter().rev().take(visible).map(|entry| {
        let (status_color, status_str) = match entry.status_style_hint() {
            StatusKind::Ok       => (Color::Green,  format!("{}  ✓", entry.status)),
            StatusKind::Redirect => (Color::Cyan,   format!("{}  →", entry.status)),
            StatusKind::ClientError => (Color::Yellow, format!("{}  !", entry.status)),
            StatusKind::ServerError => (Color::Red,    format!("{}  ✗", entry.status)),
            StatusKind::Other    => (Color::Gray,   entry.status.to_string()),
        };
        let method_color = match entry.method.as_str() {
            "GET"    => Color::Green,
            "POST"   => Color::Blue,
            "PUT" | "PATCH" => Color::Yellow,
            "DELETE" => Color::Red,
            _        => Color::Gray,
        };
        let duration_color = if entry.duration_ms > 1000 { Color::Red }
            else if entry.duration_ms > 300 { Color::Yellow }
            else { Color::Green };

        let time_str = entry.timestamp.format("%H:%M:%S").to_string();

        Row::new(vec![
            Cell::from(entry.method.clone()).style(Style::default().fg(method_color).add_modifier(Modifier::BOLD)),
            Cell::from(truncate(&entry.path, 42)).style(Style::default().fg(Color::White)),
            Cell::from(status_str).style(Style::default().fg(status_color)),
            Cell::from(time_str).style(Style::default().fg(Color::DarkGray)),
            Cell::from(entry.duration_label()).style(Style::default().fg(duration_color)),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Length(7),   // method
        Constraint::Min(20),     // path (fills remaining)
        Constraint::Length(9),   // status
        Constraint::Length(9),   // time
        Constraint::Length(9),   // duration
    ])
    .header(header)
    .block(block)
    .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_widget(table, area);
}

// ── HTTP Tap: path aggregates ─────────────────────────────────────────────────

fn draw_path_stats(f: &mut Frame, area: Rect, state: &Renderer) {
    let block = Block::default()
        .title(" Top Paths ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let snapshot = match &state.last_snapshot {
        Some(s) => s,
        None => { f.render_widget(Paragraph::new("").block(block), area); return; }
    };

    let stats = snapshot.sorted_path_stats();
    if stats.is_empty() {
        f.render_widget(Paragraph::new("").block(block), area);
        return;
    }

    let header = Row::new([
        Cell::from("Path").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Total").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Rate").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Avg ms").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Errors").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        Cell::from("Last").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    ]).height(1).bottom_margin(1);

    let rows: Vec<Row> = stats.iter().map(|ps| {
        let error_style = if ps.error_count > 0 { Style::default().fg(Color::Red) } else { Style::default().fg(Color::DarkGray) };
        let last_status_color = match ps.last_status {
            200..=299 => Color::Green,
            300..=399 => Color::Cyan,
            400..=499 => Color::Yellow,
            500..=599 => Color::Red,
            _ => Color::Gray,
        };
        Row::new(vec![
            Cell::from(truncate(&ps.path, 36)).style(Style::default().fg(Color::White)),
            Cell::from(ps.total_calls.to_string()).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(ps.rate_label()).style(Style::default().fg(Color::Yellow)),
            Cell::from(format!("{}ms", ps.avg_duration_ms)).style(Style::default().fg(Color::Green)),
            Cell::from(ps.error_count.to_string()).style(error_style),
            Cell::from(ps.last_status.to_string()).style(Style::default().fg(last_status_color)),
        ])
    }).collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Min(20),
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(6),
        ])
        .header(header)
        .block(block),
        area,
    );
}

// ── Inbound port monitor (no-tap mode) ───────────────────────────────────────

fn draw_request_monitor(f: &mut Frame, area: Rect, state: &Renderer) {
    let block = Block::default()
        .title(" ↓ Inbound Requests (per port) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let snapshot = match &state.last_snapshot {
        Some(s) => s,
        None => { f.render_widget(Paragraph::new("Scanning…").block(block), area); return; }
    };

    let port_stats = snapshot.sorted_port_stats();
    if port_stats.is_empty() {
        f.render_widget(
            Paragraph::new(vec![Line::from(""), Line::from(Span::styled("  Waiting for inbound connections…", Style::default().fg(Color::DarkGray)))]).block(block),
            area,
        );
        return;
    }

    let inner = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(2) };
    let section_height = 3u16;
    let max_ports = (inner.height / section_height) as usize;
    f.render_widget(block, area);

    for (i, stat) in port_stats.iter().take(max_ports).enumerate() {
        let y = inner.y + (i as u16 * section_height);
        if y + section_height > inner.y + inner.height { break; }
        render_port_section(f, stat, inner.x, y, inner.width);
    }
}

fn render_port_section(f: &mut Frame, stat: &PortStat, x: u16, y: u16, width: u16) {
    let is_hot = stat.idle_secs == 0;
    let port_style = if is_hot { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
    let rate = stat.req_per_sec();

    let header = Line::from(vec![
        Span::styled(format!(":{:<5}", stat.port), port_style),
        Span::styled(format!(" {:<16}", truncate(&stat.service_name, 16)), Style::default().fg(Color::Cyan)),
        Span::styled("  Total: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:<7}", stat.total_requests), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("  Active: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", stat.active_connections), Style::default().fg(Color::Green)),
        Span::styled("  Rate: ", Style::default().fg(Color::DarkGray)),
        Span::styled(stat.rate_label(), Style::default().fg(Color::Yellow)),
        Span::styled("  Last: ", Style::default().fg(Color::DarkGray)),
        Span::styled(stat.idle_label(), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(header), Rect { x, y, width, height: 1 });

    let gauge_pct = ((rate / 20.0) * 100.0).min(100.0) as u16;
    let gauge_color = if rate >= 5.0 { Color::Red } else if rate >= 1.0 { Color::Yellow } else if rate > 0.0 { Color::Green } else { Color::DarkGray };
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(gauge_color).bg(Color::Black))
            .percent(gauge_pct)
            .label(format!("{:.1} req/s", rate)),
        Rect { x, y: y + 1, width, height: 1 },
    );
}

// ── Service-to-service call table ─────────────────────────────────────────────

fn draw_calls_table(f: &mut Frame, area: Rect, state: &Renderer) {
    let block = Block::default()
        .title(" ↔ Service API Calls ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let snapshot = match &state.last_snapshot {
        Some(s) => s,
        None => { f.render_widget(Paragraph::new("Scanning…").block(block).style(Style::default().fg(Color::DarkGray)), area); return; }
    };

    let rows_data = snapshot.sorted_edges(state.node_only);
    if rows_data.is_empty() {
        f.render_widget(
            Paragraph::new(vec![Line::from(""), Line::from(Span::styled("  No service-to-service connections detected.", Style::default().fg(Color::DarkGray)))]).block(block),
            area,
        );
        return;
    }

    let header = Row::new(["Source", "→", "Destination", "Total", "Rate (10s)", "Last"].iter().map(|h| {
        Cell::from(*h).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))
    })).height(1).bottom_margin(1);

    let rows: Vec<Row> = rows_data.iter().map(|(edge, src, dst)| {
        let is_hot = edge.idle_secs <= 2;
        let count_style = if is_hot { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
        let rate_style = if edge.req_per_sec() >= 1.0 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::DarkGray) };
        Row::new(vec![
            Cell::from(truncate(&src.name, 17)).style(runtime_style(&src.runtime, is_hot)),
            Cell::from("→").style(Style::default().fg(Color::Yellow)),
            Cell::from(truncate(&dst.name, 17)).style(runtime_style(&dst.runtime, is_hot)),
            Cell::from(edge.total_calls.to_string()).style(count_style),
            Cell::from(edge.rate_label()).style(rate_style),
            Cell::from(edge.idle_label()).style(Style::default().fg(Color::DarkGray)),
        ])
    }).collect();

    f.render_widget(
        Table::new(rows, [Constraint::Length(17), Constraint::Length(2), Constraint::Length(17), Constraint::Length(7), Constraint::Length(10), Constraint::Length(10)])
            .header(header)
            .block(block)
            .highlight_style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

fn runtime_style(runtime: &crate::collector::Runtime, bright: bool) -> Style {
    let color = match runtime {
        crate::collector::Runtime::Node => Color::Green,
        crate::collector::Runtime::Bun => Color::Magenta,
        crate::collector::Runtime::Other(_) => Color::Gray,
    };
    if bright { Style::default().fg(color).add_modifier(Modifier::BOLD) } else { Style::default().fg(color) }
}

// ── Event log ─────────────────────────────────────────────────────────────────

fn draw_event_log(f: &mut Frame, area: Rect, state: &Renderer) {
    let visible = (area.height as usize).saturating_sub(2);
    let log_lines: Vec<Line> = state.event_log.iter().rev().take(visible).rev()
        .map(|msg| Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Gray))))
        .collect();
    f.render_widget(
        Paragraph::new(log_lines)
            .block(Block::default().title(" Live Events ").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
            .wrap(Wrap { trim: true }),
        area,
    );
}
