//! Port-Patrol: A TUI task manager for localhost ports
//!
//! This is the main entry point that sets up the terminal,
//! runs the event loop, and coordinates the Model-View-Update cycle.

mod app;
mod remote;
mod scanner;
mod ui;

use std::{
    io::{self, stdout},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use app::SortColumn;
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use remote::{RemoteConfig, RemoteScanner};
use scanner::Scanner;

/// Poll rate for responsive input (50ms)
const POLL_RATE: Duration = Duration::from_millis(50);

/// Default scan interval for refreshing port data (2 seconds)
const DEFAULT_SCAN_INTERVAL: u64 = 2;

/// Port-Patrol: A TUI task manager for localhost ports
#[derive(Parser, Debug)]
#[command(name = "port-patrol")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Remote host to monitor (format: user@host:port or user@host or host)
    #[arg(short = 'H', long)]
    host: Option<String>,

    /// Path to SSH private key (optional, uses ssh-agent or default keys if not specified)
    #[arg(short = 'i', long)]
    identity: Option<PathBuf>,

    /// Scan interval in seconds (default: 2)
    #[arg(short = 's', long, default_value_t = DEFAULT_SCAN_INTERVAL)]
    scan_interval: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup terminal
    let terminal = setup_terminal().context("Failed to setup terminal")?;

    // Run the application
    let result = run(terminal, args);

    // Restore terminal regardless of result
    restore_terminal().context("Failed to restore terminal")?;

    // Return the result
    result
}

/// Setup the terminal for TUI rendering
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).context("Failed to create terminal")?;
    Ok(terminal)
}

/// Restore the terminal to its original state
fn restore_terminal() -> Result<()> {
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(stdout(), LeaveAlternateScreen).context("Failed to leave alternate screen")?;
    Ok(())
}

/// Scanner mode - either local or remote
enum ScannerMode {
    Local(Box<Scanner>),
    Remote(RemoteScanner),
}

impl ScannerMode {
    fn scan(&mut self) -> Vec<app::PortEntry> {
        match self {
            ScannerMode::Local(scanner) => scanner.scan(),
            ScannerMode::Remote(scanner) => scanner.scan().unwrap_or_default(),
        }
    }

    fn kill_process(&mut self, pid: u32) -> Result<()> {
        match self {
            ScannerMode::Local(scanner) => scanner.kill_process(pid),
            ScannerMode::Remote(scanner) => scanner.kill_process(pid),
        }
    }
}

/// Main application loop implementing Model-View-Update pattern
fn run(mut terminal: Terminal<CrosstermBackend<io::Stdout>>, args: Args) -> Result<()> {
    use std::time::Instant;

    // Initialize application state (Model)
    let mut app = App::new();

    // Calculate scan interval from args
    let scan_interval = Duration::from_secs(args.scan_interval);

    // Initialize the scanner (local or remote)
    let mut scanner_mode = if let Some(host_str) = &args.host {
        // Remote mode
        let mut config = RemoteConfig::parse(host_str)?;
        if let Some(key_path) = args.identity {
            config = config.with_key(key_path);
        }

        app.set_remote_host(Some(config.display()));
        app.set_info(format!("Connecting to {}...", config.display()));

        // Draw connecting message
        terminal.draw(|frame| ui::render(frame, &app))?;

        let mut remote_scanner = RemoteScanner::new(config.clone());
        match remote_scanner.connect() {
            Ok(()) => {
                app.set_success(format!("Connected to {}", config.display()));
            }
            Err(e) => {
                app.set_error(format!("Connection failed: {}", e));
                // Still allow viewing the error
            }
        }

        ScannerMode::Remote(remote_scanner)
    } else {
        // Local mode
        ScannerMode::Local(Box::default())
    };

    // Perform initial scan
    let entries = scanner_mode.scan();
    app.update_entries(entries);

    // Track last scan time for throttling
    let mut last_scan = Instant::now();

    // Main event loop
    loop {
        // VIEW: Render the current state
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Check if we should quit
        if app.should_quit {
            break;
        }

        // UPDATE: Handle events with short poll for responsive input
        if event::poll(POLL_RATE)? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events (not release)
                if key.kind == KeyEventKind::Press {
                    handle_key_event(&mut app, key.code, key.modifiers, &mut scanner_mode);
                }
            }
        }

        // TICK: Update data only at scan interval (not every poll)
        if last_scan.elapsed() >= scan_interval {
            let entries = scanner_mode.scan();
            app.update_entries(entries);
            last_scan = Instant::now();
        }

        // Maybe clear old status messages
        app.maybe_clear_status();
    }

    Ok(())
}

/// Handle keyboard input events
fn handle_key_event(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    scanner: &mut ScannerMode,
) {
    // If help is shown, close it on any key
    if app.show_help {
        app.show_help = false;
        return;
    }

    // Handle filter mode separately
    if app.filter_mode {
        handle_filter_input(app, code);
        return;
    }

    match code {
        // Quit commands
        KeyCode::Char('q' | 'Q') => {
            app.quit();
        }
        // Ctrl+C to quit
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit();
        }
        // Help toggle
        KeyCode::Char('?') => {
            app.toggle_help();
        }
        // Navigation
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next();
        }
        // Page navigation
        KeyCode::PageUp => {
            for _ in 0..10 {
                app.select_previous();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                app.select_next();
            }
        }
        // Home/End
        KeyCode::Home => {
            app.selected_index = 0;
        }
        KeyCode::End => {
            if !app.entries.is_empty() {
                app.selected_index = app.entries.len() - 1;
            }
        }
        // Kill selected process
        KeyCode::Enter => {
            handle_kill(app, scanner);
        }
        // Alternative kill with 'k' + Ctrl
        KeyCode::Char('K') if modifiers.contains(KeyModifiers::CONTROL) => {
            handle_kill(app, scanner);
        }
        // Sort: cycle through columns (legacy)
        KeyCode::Char('s') => {
            app.cycle_sort_column();
        }
        // Reverse sort order (legacy)
        KeyCode::Char('r') => {
            app.toggle_sort_order();
        }
        // K9s-style sorting: Shift + letter or number keys
        // Press same key again to toggle ascending/descending
        KeyCode::Char('P') => app.sort_by_column(SortColumn::Port), // Shift+P = Port
        KeyCode::Char('O') => app.sort_by_column(SortColumn::Protocol), // Shift+O = prOtocol
        KeyCode::Char('I') => app.sort_by_column(SortColumn::Pid),  // Shift+I = pId
        KeyCode::Char('N') => app.sort_by_column(SortColumn::ProcessName), // Shift+N = Name
        KeyCode::Char('C') => app.sort_by_column(SortColumn::CpuUsage), // Shift+C = Cpu
        KeyCode::Char('M') => app.sort_by_column(SortColumn::MemoryUsage), // Shift+M = Memory
        // Number keys for quick sort
        KeyCode::Char('1') => app.sort_by_column(SortColumn::Port),
        KeyCode::Char('2') => app.sort_by_column(SortColumn::Protocol),
        KeyCode::Char('3') => app.sort_by_column(SortColumn::Pid),
        KeyCode::Char('4') => app.sort_by_column(SortColumn::ProcessName),
        KeyCode::Char('5') => app.sort_by_column(SortColumn::CpuUsage),
        KeyCode::Char('6') => app.sort_by_column(SortColumn::MemoryUsage),
        // Filter mode
        KeyCode::Char('/') => {
            app.enter_filter_mode();
        }
        // Clear filter or close help
        KeyCode::Esc => {
            if !app.filter.is_empty() {
                app.clear_filter();
            }
        }
        _ => {}
    }
}

/// Handle input while in filter mode
fn handle_filter_input(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Enter => {
            app.exit_filter_mode();
        }
        KeyCode::Esc => {
            app.filter.clear();
            app.exit_filter_mode();
        }
        KeyCode::Backspace => {
            app.filter_pop();
        }
        KeyCode::Char(c) => {
            app.filter_push(c);
        }
        _ => {}
    }
}

/// Handle the kill command for the selected process
fn handle_kill(app: &mut App, scanner: &mut ScannerMode) {
    if let Some(entry) = app.selected_entry() {
        let pid = entry.pid;
        let process_name = entry.process_name.clone();
        let port = entry.port;

        // Attempt to kill the process
        match scanner.kill_process(pid) {
            Ok(()) => {
                app.set_success(format!(
                    "Killed '{}' (PID: {}) on port {}",
                    process_name, pid, port
                ));
            }
            Err(e) => {
                // Handle permission errors gracefully
                app.set_error(format!("{}", e));
            }
        }
    } else {
        app.set_info("No process selected");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PortEntry, Protocol, StatusMessage};

    // ==================== Helper Functions ====================

    fn create_test_entry(port: u16, protocol: Protocol, pid: u32) -> PortEntry {
        PortEntry {
            port,
            protocol,
            pid,
            process_name: format!("process_{}", pid),
            cpu_usage: 0.0,
            memory_usage: 1024 * pid as u64,
            memory_display: format!("{} KB", pid),
            has_parent: true,
            is_zombie: false,
        }
    }

    fn create_entries(count: usize) -> Vec<PortEntry> {
        (0..count)
            .map(|i| create_test_entry(3000 + i as u16, Protocol::Tcp, i as u32 + 1))
            .collect()
    }

    fn create_app_with_entries(count: usize) -> App {
        let mut app = App::new();
        app.entries = create_entries(count);
        app
    }

    fn create_test_scanner() -> ScannerMode {
        ScannerMode::Local(Box::default())
    }

    /// Helper to call handle_key_event without scanner (for tests that don't need kill)
    fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        let mut scanner = create_test_scanner();
        handle_key_event(app, code, modifiers, &mut scanner);
    }

    // ==================== App Initialization Tests ====================

    #[test]
    fn test_app_initialization() {
        let app = App::new();
        assert!(!app.should_quit);
        assert_eq!(app.selected_index, 0);
        assert!(app.entries.is_empty());
    }

    #[test]
    fn test_app_initial_status_message() {
        let app = App::new();
        match &app.status_message {
            StatusMessage::Info(msg) => {
                assert!(msg.contains("Ready") || msg.contains("navigate") || msg.contains("↑/↓"));
            }
            _ => panic!("Initial message should be Info"),
        }
    }

    // ==================== Quit Key Tests ====================

    #[test]
    fn test_key_event_quit_lowercase_q() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn test_key_event_quit_uppercase_q() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('Q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn test_key_event_quit_ctrl_c() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn test_key_event_c_without_ctrl_does_not_quit() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(!app.should_quit);
    }

    // ==================== Navigation Arrow Keys Tests ====================

    #[test]
    fn test_key_event_down_arrow() {
        let mut app = create_app_with_entries(5);
        assert_eq!(app.selected_index, 0);

        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_key_event_up_arrow() {
        let mut app = create_app_with_entries(5);
        app.selected_index = 2;

        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_key_event_down_wrap_around() {
        let mut app = create_app_with_entries(3);
        app.selected_index = 2; // Last item

        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0); // Wrap to first
    }

    #[test]
    fn test_key_event_up_wrap_around() {
        let mut app = create_app_with_entries(3);
        app.selected_index = 0; // First item

        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 2); // Wrap to last
    }

    // ==================== Vim-style Navigation Tests ====================

    #[test]
    fn test_key_event_j_moves_down() {
        let mut app = create_app_with_entries(5);
        assert_eq!(app.selected_index, 0);

        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_key_event_k_moves_up() {
        let mut app = create_app_with_entries(5);
        app.selected_index = 2;

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_vim_navigation_wrap_around() {
        let mut app = create_app_with_entries(3);

        // j wraps at end
        app.selected_index = 2;
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);

        // k wraps at beginning
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 2);
    }

    // ==================== Page Navigation Tests ====================

    #[test]
    fn test_key_event_page_down() {
        let mut app = create_app_with_entries(25);
        app.selected_index = 0;

        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 10); // Moves 10 items
    }

    #[test]
    fn test_key_event_page_up() {
        let mut app = create_app_with_entries(25);
        app.selected_index = 15;

        handle_key(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 5); // Moves 10 items
    }

    #[test]
    fn test_key_event_page_down_near_end() {
        let mut app = create_app_with_entries(15);
        app.selected_index = 10;

        // PageDown moves 10 times, wrapping around
        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        // After 10 next() calls from index 10 in 15 items:
        // 10->11->12->13->14->0->1->2->3->4->5
        assert_eq!(app.selected_index, 5);
    }

    #[test]
    fn test_key_event_page_up_near_start() {
        let mut app = create_app_with_entries(15);
        app.selected_index = 3;

        // PageUp moves 10 times, wrapping around
        handle_key(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
        // After 10 previous() calls from index 3 in 15 items:
        // 3->2->1->0->14->13->12->11->10->9->8
        assert_eq!(app.selected_index, 8);
    }

    // ==================== Home/End Tests ====================

    #[test]
    fn test_key_event_home() {
        let mut app = create_app_with_entries(10);
        app.selected_index = 7;

        handle_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_key_event_home_already_at_start() {
        let mut app = create_app_with_entries(10);
        app.selected_index = 0;

        handle_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_key_event_end() {
        let mut app = create_app_with_entries(10);
        app.selected_index = 3;

        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 9);
    }

    #[test]
    fn test_key_event_end_already_at_end() {
        let mut app = create_app_with_entries(10);
        app.selected_index = 9;

        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 9);
    }

    #[test]
    fn test_key_event_end_empty_list() {
        let mut app = App::new();
        app.selected_index = 0;

        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0); // No change when empty
    }

    // ==================== Navigation on Empty List Tests ====================

    #[test]
    fn test_navigation_empty_list_down() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_navigation_empty_list_up() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_navigation_empty_list_page_down() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_navigation_empty_list_page_up() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    // ==================== Kill Tests ====================

    #[test]
    fn test_handle_kill_no_selection() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        handle_kill(&mut app, &mut scanner);

        match &app.status_message {
            StatusMessage::Info(msg) => assert!(msg.contains("No process")),
            _ => panic!("Expected Info message for no selection"),
        }
    }

    #[test]
    fn test_handle_kill_nonexistent_process() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.entries = vec![PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 999_999_999, // Unlikely to exist
            process_name: "fake".into(),
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_display: "0 B".into(),
            has_parent: true,
            is_zombie: false,
        }];

        handle_kill(&mut app, &mut scanner);

        // Should get an error message
        match &app.status_message {
            StatusMessage::Error(msg) => {
                assert!(msg.contains("not found") || msg.contains("Permission"));
            }
            _ => panic!("Expected Error message for failed kill"),
        }
    }

    #[test]
    fn test_key_event_enter_calls_kill() {
        let mut app = App::new();
        // Empty list, Enter should result in "No process selected"
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        match &app.status_message {
            StatusMessage::Info(msg) => assert!(msg.contains("No process")),
            _ => panic!("Expected Info message"),
        }
    }

    #[test]
    fn test_key_event_ctrl_k_calls_kill() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('K'), KeyModifiers::CONTROL);

        match &app.status_message {
            StatusMessage::Info(msg) => assert!(msg.contains("No process")),
            _ => panic!("Expected Info message"),
        }
    }

    // ==================== Unknown Key Tests ====================

    #[test]
    fn test_unknown_key_does_nothing() {
        let mut app = create_app_with_entries(5);
        app.selected_index = 2;

        // Press an unhandled key
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);

        // State should be unchanged
        assert_eq!(app.selected_index, 2);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_function_keys_do_nothing() {
        let mut app = create_app_with_entries(5);
        app.selected_index = 2;

        handle_key(&mut app, KeyCode::F(1), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::F(12), KeyModifiers::NONE);

        assert_eq!(app.selected_index, 2);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_escape_does_nothing() {
        let mut app = create_app_with_entries(5);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        assert!(!app.should_quit);
    }

    // ==================== Modifier Key Tests ====================

    #[test]
    fn test_shift_modifier_ignored_for_navigation() {
        let mut app = create_app_with_entries(5);

        // Shift+Down should still navigate
        handle_key(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_alt_modifier_ignored_for_navigation() {
        let mut app = create_app_with_entries(5);

        handle_key(&mut app, KeyCode::Down, KeyModifiers::ALT);
        assert_eq!(app.selected_index, 1);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_navigation_workflow() {
        let mut app = create_app_with_entries(20);

        // Navigate down with various methods
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);

        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 2);

        // Page down
        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 12);

        // Navigate up
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 11);

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 10);

        // Jump to end
        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 19);

        // Jump to start
        handle_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);

        // Quit
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn test_rapid_key_events() {
        let mut app = create_app_with_entries(100);

        // Rapid navigation should work
        for _ in 0..500 {
            handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        }

        assert!(app.selected_index < 100);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_mixed_protocol_entries() {
        let mut app = App::new();
        app.entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3000, Protocol::Udp, 2),
            create_test_entry(3001, Protocol::Tcp, 3),
            create_test_entry(3001, Protocol::Udp, 4),
        ];

        // Navigate through all
        for i in 0..4 {
            assert_eq!(app.selected_index, i);
            handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        }
        assert_eq!(app.selected_index, 0); // Wrapped
    }

    #[test]
    fn test_timing_constants() {
        assert_eq!(POLL_RATE, Duration::from_millis(50));
        assert_eq!(DEFAULT_SCAN_INTERVAL, 2);
    }
}
