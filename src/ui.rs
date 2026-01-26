//! UI rendering module - k9s-inspired design
//!
//! This module implements the "View" part of the Model-View-Update pattern.
//! It handles all ratatui rendering logic with a k9s-like aesthetic.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::app::{App, PortEntry, SortColumn, SortOrder, StatusMessage};

// K9s-inspired color palette
const COLOR_BG: Color = Color::Rgb(30, 30, 46); // Dark background
const COLOR_HEADER_BG: Color = Color::Rgb(49, 50, 68); // Header background
const COLOR_BORDER: Color = Color::Rgb(88, 91, 112); // Border color
const COLOR_TEXT: Color = Color::Rgb(205, 214, 244); // Main text
const COLOR_TEXT_DIM: Color = Color::Rgb(108, 112, 134); // Dimmed text
const COLOR_ACCENT: Color = Color::Rgb(137, 180, 250); // Blue accent (like k9s)
const COLOR_ACCENT2: Color = Color::Rgb(166, 227, 161); // Green accent
const COLOR_WARNING: Color = Color::Rgb(249, 226, 175); // Yellow/warning
const COLOR_ERROR: Color = Color::Rgb(243, 139, 168); // Red/error
const COLOR_SELECTED_BG: Color = Color::Rgb(69, 71, 90); // Selected row bg
const COLOR_ROW_ALT: Color = Color::Rgb(39, 39, 55); // Alternating row

/// Main UI rendering function
pub fn render(frame: &mut Frame, app: &App) {
    // Fill background
    let bg_block = Block::default().style(Style::default().bg(COLOR_BG));
    frame.render_widget(bg_block, frame.area());

    // Create the main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top bar
            Constraint::Length(1), // Breadcrumb/context bar
            Constraint::Min(10),   // Table
            Constraint::Length(1), // Command bar
        ])
        .split(frame.area());

    render_top_bar(frame, chunks[0]);
    render_context_bar(frame, app, chunks[1]);
    render_table(frame, app, chunks[2]);
    render_command_bar(frame, app, chunks[3]);

    // Render help popup if active
    if app.show_help {
        render_help_popup(frame);
    }
}

/// Render the top bar with logo and hints
fn render_top_bar(frame: &mut Frame, area: Rect) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(" ⚓ ", Style::default().fg(COLOR_ACCENT).bold()),
        Span::styled("Port-Patrol", Style::default().fg(COLOR_ACCENT).bold()),
        Span::styled(" │ ", Style::default().fg(COLOR_BORDER)),
        Span::styled(
            "Localhost Port Monitor",
            Style::default().fg(COLOR_TEXT_DIM),
        ),
        Span::raw(" ".repeat(area.width.saturating_sub(60) as usize)),
        Span::styled("<?>", Style::default().fg(COLOR_ACCENT)),
        Span::styled(" Help ", Style::default().fg(COLOR_TEXT_DIM)),
        Span::styled("<q>", Style::default().fg(COLOR_ACCENT)),
        Span::styled(" Quit", Style::default().fg(COLOR_TEXT_DIM)),
    ]))
    .style(Style::default().bg(COLOR_HEADER_BG));

    frame.render_widget(bar, area);
}

/// Render the context/breadcrumb bar with sort and filter info
fn render_context_bar(frame: &mut Frame, app: &App, area: Rect) {
    let sort_col = match app.sort_column {
        SortColumn::Port => "Port",
        SortColumn::Protocol => "Protocol",
        SortColumn::Pid => "PID",
        SortColumn::ProcessName => "Name",
        SortColumn::CpuUsage => "CPU%",
        SortColumn::MemoryUsage => "Memory",
    };
    let sort_dir = match app.sort_order {
        SortOrder::Ascending => "↑",
        SortOrder::Descending => "↓",
    };

    let mut spans = vec![Span::styled(" 📡 ", Style::default().fg(COLOR_ACCENT2))];

    // Show remote host or localhost
    if let Some(ref host) = app.remote_host {
        spans.push(Span::styled("Remote: ", Style::default().fg(COLOR_WARNING)));
        spans.push(Span::styled(
            host.clone(),
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            "localhost",
            Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD),
        ));
    }

    spans.extend(vec![
        Span::styled(" │ ", Style::default().fg(COLOR_BORDER)),
        Span::styled(
            format!("{} ", app.entries.len()),
            Style::default().fg(COLOR_ACCENT),
        ),
        Span::styled("listening", Style::default().fg(COLOR_TEXT_DIM)),
        Span::styled(" │ ", Style::default().fg(COLOR_BORDER)),
        Span::styled("Sort: ", Style::default().fg(COLOR_TEXT_DIM)),
        Span::styled(
            format!("{}{}", sort_col, sort_dir),
            Style::default().fg(COLOR_WARNING),
        ),
    ]);

    // Add filter indicator if active
    if !app.filter.is_empty() {
        spans.push(Span::styled(" │ ", Style::default().fg(COLOR_BORDER)));
        if app.filter_is_regex {
            spans.push(Span::styled("Regex: ", Style::default().fg(COLOR_WARNING)));
        } else {
            spans.push(Span::styled(
                "Filter: ",
                Style::default().fg(COLOR_TEXT_DIM),
            ));
        }
        spans.push(Span::styled(
            format!("\"{}\"", app.filter),
            Style::default().fg(COLOR_ACCENT),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(COLOR_BG));

    frame.render_widget(bar, area);
}

/// Render the main process table
fn render_table(frame: &mut Frame, app: &App, area: Rect) {
    // Define table headers with sort indicators and shortcut keys
    // Format: (display_name, sort_column, shortcut_key)
    let headers = [
        ("PORT", SortColumn::Port, "P/1"),
        ("PROTO", SortColumn::Protocol, "O/2"),
        ("PID", SortColumn::Pid, "I/3"),
        ("NAME", SortColumn::ProcessName, "N/4"),
        ("CPU%", SortColumn::CpuUsage, "C/5"),
        ("MEM", SortColumn::MemoryUsage, "M/6"),
    ];

    let header_cells = headers.iter().map(|(name, col, key)| {
        let is_sorted = app.sort_column == *col;
        let indicator = if is_sorted {
            match app.sort_order {
                SortOrder::Ascending => "▲",
                SortOrder::Descending => "▼",
            }
        } else {
            ""
        };

        let style = if is_sorted {
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(COLOR_TEXT_DIM)
                .add_modifier(Modifier::BOLD)
        };

        // Show: "NAME[N/4]" or "NAME[N/4]▲" when sorted
        let text = if is_sorted {
            format!("{}[{}]{}", name, key, indicator)
        } else {
            format!("{}[{}]", name, key)
        };

        Cell::from(text).style(style)
    });

    let header = Row::new(header_cells)
        .style(Style::default().bg(COLOR_HEADER_BG))
        .height(1);

    // Create rows from entries with alternating colors
    let rows: Vec<Row> = app
        .entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_selected = idx == app.selected_index;
            create_row(entry, idx, is_selected)
        })
        .collect();

    // Define column widths (accounting for [key] indicators in headers)
    let widths = [
        Constraint::Length(12), // PORT[P/1]▲
        Constraint::Length(12), // PROTO[O/2]
        Constraint::Length(11), // PID[I/3]
        Constraint::Min(15),    // NAME[N/4] + process name
        Constraint::Length(12), // CPU%[C/5]
        Constraint::Length(12), // MEM[M/6]
    ];

    // Create the table
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_BORDER))
                .style(Style::default().bg(COLOR_BG)),
        )
        .highlight_style(
            Style::default()
                .bg(COLOR_SELECTED_BG)
                .fg(COLOR_TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    // Create table state for selection
    let mut state = TableState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.selected_index));
    }

    frame.render_stateful_widget(table, area, &mut state);

    // Show empty state message if no entries
    if app.entries.is_empty() {
        let msg = if !app.filter.is_empty() {
            format!("No ports matching \"{}\"", app.filter)
        } else {
            "No listening ports found".to_string()
        };

        let empty_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("⚠", Style::default().fg(COLOR_WARNING))),
            Line::from(Span::styled(msg, Style::default().fg(COLOR_TEXT_DIM))),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().bg(COLOR_BG));

        let inner_area = Rect {
            x: area.x + 2,
            y: area.y + area.height / 2 - 1,
            width: area.width.saturating_sub(4),
            height: 3,
        };
        frame.render_widget(empty_msg, inner_area);
    }
}

/// Create a table row from a PortEntry
fn create_row(entry: &PortEntry, idx: usize, is_selected: bool) -> Row<'static> {
    // Alternating row background
    let row_bg = if is_selected {
        COLOR_SELECTED_BG
    } else if idx % 2 == 0 {
        COLOR_BG
    } else {
        COLOR_ROW_ALT
    };

    // Determine text color based on status
    let text_color = if entry.is_zombie {
        COLOR_ERROR
    } else if is_selected {
        COLOR_TEXT
    } else {
        COLOR_TEXT_DIM
    };

    // CPU color coding
    let cpu_color = if entry.cpu_usage > 80.0 {
        COLOR_ERROR
    } else if entry.cpu_usage > 40.0 {
        COLOR_WARNING
    } else if entry.cpu_usage > 10.0 {
        COLOR_ACCENT2
    } else {
        text_color
    };

    // Protocol badge color
    let proto_color = match entry.protocol {
        crate::app::Protocol::Tcp => COLOR_ACCENT,
        crate::app::Protocol::Udp => COLOR_ACCENT2,
    };

    let cells = vec![
        Cell::from(format!("{:>5}", entry.port)).style(Style::default().fg(if is_selected {
            COLOR_ACCENT
        } else {
            text_color
        })),
        Cell::from(entry.protocol.to_string()).style(Style::default().fg(proto_color)),
        Cell::from(format!("{:>6}", entry.pid)).style(Style::default().fg(text_color)),
        Cell::from(entry.process_name.clone()).style(Style::default().fg(if entry.is_zombie {
            COLOR_ERROR
        } else {
            text_color
        })),
        Cell::from(format!("{:>5.1}%", entry.cpu_usage)).style(Style::default().fg(cpu_color)),
        Cell::from(entry.memory_display.clone()).style(Style::default().fg(text_color)),
    ];

    Row::new(cells).style(Style::default().bg(row_bg)).height(1)
}

/// Render the command bar at the bottom
fn render_command_bar(frame: &mut Frame, app: &App, area: Rect) {
    let content = if app.filter_mode {
        // Filter input mode (like vim command mode)
        Line::from(vec![
            Span::styled("/", Style::default().fg(COLOR_ACCENT).bold()),
            Span::styled(&app.filter, Style::default().fg(COLOR_TEXT)),
            Span::styled("█", Style::default().fg(COLOR_ACCENT)), // Cursor
        ])
    } else {
        // Show keybindings or status
        match &app.status_message {
            StatusMessage::Info(msg) => {
                // Show info message if it's actionable, otherwise show quick help
                if msg == "Ready" || msg.is_empty() {
                    Line::from(vec![
                        Span::styled(" <j/k>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Navigate ", Style::default().fg(COLOR_TEXT_DIM)),
                        Span::styled("<Enter>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Kill ", Style::default().fg(COLOR_TEXT_DIM)),
                        Span::styled("<s>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Sort ", Style::default().fg(COLOR_TEXT_DIM)),
                        Span::styled("<1-6>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Column ", Style::default().fg(COLOR_TEXT_DIM)),
                        Span::styled("</>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Filter ", Style::default().fg(COLOR_TEXT_DIM)),
                        Span::styled("<?>", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(" Help", Style::default().fg(COLOR_TEXT_DIM)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(" ℹ ", Style::default().fg(COLOR_ACCENT)),
                        Span::styled(msg.clone(), Style::default().fg(COLOR_TEXT_DIM)),
                    ])
                }
            }
            StatusMessage::Success(msg) => Line::from(vec![
                Span::styled(" ✓ ", Style::default().fg(COLOR_ACCENT2).bold()),
                Span::styled(msg.clone(), Style::default().fg(COLOR_ACCENT2)),
            ]),
            StatusMessage::Error(msg) => Line::from(vec![
                Span::styled(" ✗ ", Style::default().fg(COLOR_ERROR).bold()),
                Span::styled(msg.clone(), Style::default().fg(COLOR_ERROR)),
            ]),
        }
    };

    let bar = Paragraph::new(content).style(Style::default().bg(COLOR_HEADER_BG));

    frame.render_widget(bar, area);
}

/// Render help popup
fn render_help_popup(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());

    // Clear the background
    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  NAVIGATION",
            Style::default().fg(COLOR_ACCENT).bold(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("    ↑/k      ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Move up", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    ↓/j      ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Move down", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    PgUp     ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Page up (10 rows)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    PgDn     ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Page down (10 rows)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Home     ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Go to first", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    End      ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Go to last", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ACTIONS",
            Style::default().fg(COLOR_ACCENT).bold(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("    Enter    ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Kill selected process", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    /        ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Filter (supports regex)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Esc      ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Clear filter / Close help", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    q        ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Quit", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  SORTING (k9s-style)",
            Style::default().fg(COLOR_ACCENT).bold(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("    Shift+P  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by Port", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Shift+O  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by Protocol", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Shift+I  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by PID", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Shift+N  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by Name", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Shift+C  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by CPU %", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    Shift+M  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Sort by Memory", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    1-6      ", Style::default().fg(COLOR_WARNING)),
            Span::styled(
                "Quick sort (same as above)",
                Style::default().fg(COLOR_TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(
                "(Press same key to toggle ↑/↓)",
                Style::default().fg(COLOR_TEXT_DIM),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  LEGEND",
            Style::default().fg(COLOR_ACCENT).bold(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("TCP", Style::default().fg(COLOR_ACCENT)),
            Span::styled("  TCP connections", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("UDP", Style::default().fg(COLOR_ACCENT2)),
            Span::styled("  UDP connections", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("RED", Style::default().fg(COLOR_ERROR)),
            Span::styled(
                "  Zombie process (high CPU + orphaned)",
                Style::default().fg(COLOR_TEXT),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("           Press ", Style::default().fg(COLOR_TEXT_DIM)),
            Span::styled("?", Style::default().fg(COLOR_WARNING)),
            Span::styled(" or ", Style::default().fg(COLOR_TEXT_DIM)),
            Span::styled("Esc", Style::default().fg(COLOR_WARNING)),
            Span::styled(" to close", Style::default().fg(COLOR_TEXT_DIM)),
        ]),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(Span::styled(
                    " ⚓ Port-Patrol Help ",
                    Style::default().fg(COLOR_ACCENT).bold(),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_ACCENT))
                .style(Style::default().bg(COLOR_BG)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(help, area);
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
