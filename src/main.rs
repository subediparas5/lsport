//! Lsport: A TUI for managing local and remote ports via SSH
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
use clap::{Parser, Subcommand};
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

/// Lsport: A TUI for managing local and remote ports via SSH
#[derive(Parser, Debug)]
#[command(name = "lsport")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Remote host to monitor (format: user@host:port or user@host or host)
    /// Only used in TUI mode (when no subcommand is provided)
    #[arg(short = 'H', long)]
    host: Option<String>,

    /// Path to SSH private key (optional, uses ssh-agent or default keys if not specified)
    /// Only used in TUI mode (when no subcommand is provided)
    #[arg(short = 'i', long)]
    identity: Option<PathBuf>,

    /// Scan interval in seconds (default: 2)
    /// Only used in TUI mode (when no subcommand is provided)
    #[arg(short = 's', long, default_value_t = DEFAULT_SCAN_INTERVAL)]
    scan_interval: u64,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Describe a port or process (by port number or PID)
    Describe {
        /// Port number or PID to describe
        #[arg(value_name = "PORT_OR_PID")]
        target: String,

        /// Remote host to query (format: user@host:port or user@host or host)
        #[arg(short = 'H', long)]
        host: Option<String>,

        /// Path to SSH private key (optional, uses ssh-agent or default keys if not specified)
        #[arg(short = 'i', long)]
        identity: Option<PathBuf>,
    },
    /// Kill a process by PID or port number
    Kill {
        /// Kill process by PID
        #[arg(long, value_name = "PID")]
        pid: Option<u32>,

        /// Kill process by port number
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,

        /// Remote host to query (format: user@host:port or user@host or host)
        #[arg(short = 'H', long)]
        host: Option<String>,

        /// Path to SSH private key (optional, uses ssh-agent or default keys if not specified)
        #[arg(short = 'i', long)]
        identity: Option<PathBuf>,

        /// Force kill (SIGKILL instead of SIGTERM)
        #[arg(short = 'f', long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Command::Describe {
            target,
            host,
            identity,
        }) => run_describe(target, host, identity),
        Some(Command::Kill {
            pid,
            port,
            host,
            identity,
            force,
        }) => run_kill(pid, port, host, identity, force),
        None => {
            // Setup terminal for TUI mode
            let terminal = setup_terminal().context("Failed to setup terminal")?;

            // Run the TUI application
            let result = run(terminal, &args);

            // Restore terminal regardless of result
            restore_terminal().context("Failed to restore terminal")?;

            // Return the result
            result
        }
    }
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

/// Run the describe command
fn run_describe(target: String, host: Option<String>, identity: Option<PathBuf>) -> Result<()> {
    let entries = scan_ports(host.as_deref(), identity.as_ref())?;

    // Try to parse as port number first, then PID
    let port: Option<u16> = target.parse().ok();
    let pid: Option<u32> = target.parse().ok();

    let matching_entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            if let Some(p) = port {
                e.port == p
            } else if let Some(p) = pid {
                e.pid == p
            } else {
                e.process_name.contains(&target)
                    || e.port.to_string() == target
                    || e.pid.to_string() == target
            }
        })
        .collect();

    if matching_entries.is_empty() {
        anyhow::bail!("No process found matching '{}'", target);
    }

    // Display detailed information
    println!("Found {} matching process(es):\n", matching_entries.len());
    for entry in matching_entries {
        println!("Port:        {}", entry.port);
        println!("Protocol:    {}", entry.protocol);
        println!("PID:         {}", entry.pid);
        println!("Process:     {}", entry.process_name);
        println!("CPU Usage:   {:.1}%", entry.cpu_usage);
        println!("Memory:      {}", entry.memory_display);
        println!(
            "Has Parent:  {}",
            if entry.has_parent { "Yes" } else { "No" }
        );
        println!(
            "Zombie:      {}",
            if entry.is_zombie { "Yes ⚠️" } else { "No" }
        );
        println!();
    }

    Ok(())
}

/// Run the kill command
fn run_kill(
    pid: Option<u32>,
    port: Option<u16>,
    host: Option<String>,
    identity: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    // Validate that exactly one of pid or port is specified
    match (pid, port) {
        (None, None) => {
            anyhow::bail!("Either --pid or --port must be specified");
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both --pid and --port. Please use only one.");
        }
        _ => {}
    }

    let entries = scan_ports(host.as_deref(), identity.as_ref())?;

    // Find matching entries
    let matching_entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            if let Some(p) = pid {
                e.pid == p
            } else if let Some(p) = port {
                e.port == p
            } else {
                false
            }
        })
        .collect();

    if matching_entries.is_empty() {
        if let Some(p) = pid {
            anyhow::bail!("No process found with PID {}", p);
        } else if let Some(p) = port {
            anyhow::bail!("No process found on port {}", p);
        }
    }

    if matching_entries.len() > 1 {
        eprintln!("Warning: Multiple processes found:");
        for entry in &matching_entries {
            eprintln!(
                "  PID {}: {} on port {} ({})",
                entry.pid, entry.process_name, entry.port, entry.protocol
            );
        }
        eprintln!("\nPlease use --pid to kill a specific process.");
        std::process::exit(1);
    }

    let entry = &matching_entries[0];
    let pid_to_kill = entry.pid;

    // Kill the process
    if let Some(host_str) = host {
        // Remote kill
        let mut config = RemoteConfig::parse(&host_str)?;
        if let Some(key_path) = identity {
            config = config.with_key(key_path);
        }
        let mut scanner = RemoteScanner::new(config);
        scanner.connect()?;

        if force {
            scanner.kill_process_force(pid_to_kill)?;
        } else {
            scanner.kill_process(pid_to_kill)?;
        }
    } else {
        // Local kill
        if force {
            kill_process_force(pid_to_kill)?;
        } else {
            scanner::kill_process(pid_to_kill)?;
        }
    }

    println!(
        "Killed process '{}' (PID: {}) on port {} ({})",
        entry.process_name, pid_to_kill, entry.port, entry.protocol
    );

    Ok(())
}

/// Scan ports (local or remote)
fn scan_ports(host: Option<&str>, identity: Option<&PathBuf>) -> Result<Vec<app::PortEntry>> {
    if let Some(host_str) = host {
        // Remote scan
        let mut config = RemoteConfig::parse(host_str)?;
        if let Some(key_path) = identity {
            config = config.with_key(key_path.clone());
        }
        let mut scanner = RemoteScanner::new(config);
        scanner.connect()?;
        Ok(scanner.scan()?)
    } else {
        // Local scan
        let mut scanner = Scanner::new();
        Ok(scanner.scan())
    }
}

/// Kill a process with SIGKILL (force)
fn kill_process_force(pid: u32) -> Result<()> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        ProcessRefreshKind::new(),
    );

    let sys_pid = Pid::from_u32(pid);

    if let Some(process) = system.process(sys_pid) {
        // Try SIGTERM first
        if process.kill() {
            // Give it a moment
            std::thread::sleep(Duration::from_millis(100));

            // Check if still alive, if so use SIGKILL
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[sys_pid]),
                ProcessRefreshKind::new(),
            );

            if system.process(sys_pid).is_some() {
                // Use kill -9 equivalent
                std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .output()
                    .context("Failed to force kill process")?;
            }
            Ok(())
        } else {
            anyhow::bail!(
                "Failed to kill process {} (PID: {}). Permission denied - try running with sudo.",
                process.name().to_string_lossy(),
                pid
            )
        }
    } else {
        anyhow::bail!(
            "Process with PID {} not found. It may have already exited.",
            pid
        )
    }
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
fn run(mut terminal: Terminal<CrosstermBackend<io::Stdout>>, args: &Args) -> Result<()> {
    use std::time::Instant;

    // Initialize application state (Model)
    let mut app = App::new();

    // Calculate scan interval from args
    let scan_interval = Duration::from_secs(args.scan_interval);

    // Initialize the scanner (local or remote)
    let mut scanner_mode = if let Some(host_str) = &args.host {
        // Remote mode
        let mut config = RemoteConfig::parse(host_str)?;
        if let Some(key_path) = &args.identity {
            config = config.with_key(key_path.clone());
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

    // Handle connect mode separately
    if app.connect_mode {
        handle_connect_input(app, code, scanner);
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
        // Connect mode (use 'c' for connect, but not Ctrl+C which is quit)
        KeyCode::Char('c') if !modifiers.contains(KeyModifiers::CONTROL) => {
            app.enter_connect_mode();
        }
        // Disconnect from remote
        KeyCode::Char('d' | 'D') if app.remote_host.is_some() => {
            handle_disconnect(app, scanner);
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

/// Handle input while in connect mode
fn handle_connect_input(app: &mut App, code: KeyCode, scanner: &mut ScannerMode) {
    match code {
        KeyCode::Enter => {
            if app.connect_key_mode {
                // Second Enter - attempt connection
                handle_connect(app, scanner);
            } else if !app.connect_input.is_empty() {
                // First Enter - ask for SSH key (optional)
                app.enter_connect_key_mode();
            }
        }
        KeyCode::Esc => {
            if app.connect_key_mode {
                // Go back to host input
                app.connect_key_mode = false;
                app.connect_key_input.clear();
            } else {
                // Cancel connect mode
                app.exit_connect_mode();
            }
        }
        KeyCode::Backspace => {
            app.connect_pop();
        }
        KeyCode::Tab if !app.connect_key_mode && !app.connect_input.is_empty() => {
            // Tab to skip SSH key and connect directly
            handle_connect(app, scanner);
        }
        KeyCode::Char(c) => {
            app.connect_push(c);
        }
        _ => {}
    }
}

/// Handle connection to remote host
fn handle_connect(app: &mut App, scanner: &mut ScannerMode) {
    let host_str = app.connect_input.trim().to_string();
    if host_str.is_empty() {
        app.set_error("Host cannot be empty");
        app.exit_connect_mode();
        return;
    }

    app.set_info(format!("Connecting to {}...", host_str));

    // Parse remote config
    let mut config = match RemoteConfig::parse(&host_str) {
        Ok(cfg) => cfg,
        Err(e) => {
            app.set_error(format!("Invalid host format: {}", e));
            app.exit_connect_mode();
            return;
        }
    };

    // Set SSH key if provided
    if !app.connect_key_input.is_empty() {
        let key_path = PathBuf::from(app.connect_key_input.trim());
        config = config.with_key(key_path);
    }

    // Attempt connection
    let mut remote_scanner = RemoteScanner::new(config.clone());
    match remote_scanner.connect() {
        Ok(()) => {
            // Success - switch to remote mode
            *scanner = ScannerMode::Remote(remote_scanner);
            app.set_remote_host(Some(config.display()));
            app.set_success(format!("Connected to {}", config.display()));
            app.exit_connect_mode();

            // Perform initial scan
            let entries = scanner.scan();
            app.update_entries(entries);
        }
        Err(e) => {
            app.set_error(format!("Connection failed: {}", e));
            // Don't exit connect mode, allow user to retry
        }
    }
}

/// Handle disconnection from remote host
fn handle_disconnect(app: &mut App, scanner: &mut ScannerMode) {
    app.disconnect();
    // Switch back to local scanner
    *scanner = ScannerMode::Local(Box::default());

    // Perform initial scan
    let entries = scanner.scan();
    app.update_entries(entries);
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

    // ==================== Connect Mode Tests ====================

    #[test]
    fn test_key_event_enter_connect_mode() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();

        handle_key_event(
            &mut app,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        assert!(app.connect_mode);
        assert!(!app.connect_key_mode);
    }

    #[test]
    fn test_key_event_c_with_ctrl_does_not_enter_connect() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();

        handle_key_event(
            &mut app,
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            &mut scanner,
        );

        assert!(!app.connect_mode);
        assert!(app.should_quit); // Ctrl+C quits
    }

    #[test]
    fn test_handle_connect_input_enter_host() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");

        handle_connect_input(&mut app, KeyCode::Enter, &mut scanner);

        assert!(app.connect_key_mode);
        assert_eq!(app.connect_input, "user@host");
    }

    #[test]
    fn test_handle_connect_input_enter_empty_host() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();

        handle_connect_input(&mut app, KeyCode::Enter, &mut scanner);

        // Should not enter key mode if host is empty
        assert!(!app.connect_key_mode);
    }

    #[test]
    fn test_handle_connect_input_esc_cancels() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");

        handle_connect_input(&mut app, KeyCode::Esc, &mut scanner);

        assert!(!app.connect_mode);
        assert!(app.connect_input.is_empty());
    }

    #[test]
    fn test_handle_connect_input_esc_in_key_mode() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");
        app.enter_connect_key_mode();
        app.connect_key_input.push_str("/key");

        handle_connect_input(&mut app, KeyCode::Esc, &mut scanner);

        assert!(!app.connect_key_mode);
        assert!(app.connect_key_input.is_empty());
        assert_eq!(app.connect_input, "user@host"); // Host should remain
    }

    #[test]
    fn test_handle_connect_input_backspace() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");

        handle_connect_input(&mut app, KeyCode::Backspace, &mut scanner);

        assert_eq!(app.connect_input, "user@hos");
    }

    #[test]
    fn test_handle_connect_input_backspace_key_mode() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");
        app.enter_connect_key_mode();
        app.connect_key_input.push_str("/path/to/key");

        handle_connect_input(&mut app, KeyCode::Backspace, &mut scanner);

        assert_eq!(app.connect_key_input, "/path/to/ke");
        assert_eq!(app.connect_input, "user@host");
    }

    #[test]
    fn test_handle_connect_input_char() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();

        handle_connect_input(&mut app, KeyCode::Char('u'), &mut scanner);
        handle_connect_input(&mut app, KeyCode::Char('s'), &mut scanner);
        handle_connect_input(&mut app, KeyCode::Char('e'), &mut scanner);
        handle_connect_input(&mut app, KeyCode::Char('r'), &mut scanner);

        assert_eq!(app.connect_input, "user");
    }

    #[test]
    fn test_handle_connect_input_tab_skips_key() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        app.connect_input.push_str("user@host");

        // Tab should attempt connection (will fail in test, but should exit connect mode)
        handle_connect_input(&mut app, KeyCode::Tab, &mut scanner);

        // Connect mode should be exited (connection will fail, but mode should exit)
        // Note: handle_connect will fail because we can't actually connect in tests
        // but the mode should be handled
    }

    #[test]
    fn test_handle_disconnect() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.set_remote_host(Some("user@host:22".to_string()));
        assert!(app.remote_host.is_some());

        handle_disconnect(&mut app, &mut scanner);

        assert!(app.remote_host.is_none());
        match &app.status_message {
            StatusMessage::Info(msg) => {
                assert!(msg.contains("Disconnected"));
            }
            _ => panic!("Expected Info message"),
        }
    }

    #[test]
    fn test_key_event_disconnect() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.set_remote_host(Some("user@host:22".to_string()));

        handle_key_event(
            &mut app,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        assert!(app.remote_host.is_none());
    }

    #[test]
    fn test_key_event_disconnect_not_connected() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        assert!(app.remote_host.is_none());

        handle_key_event(
            &mut app,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        // Should not error, just do nothing
        assert!(app.remote_host.is_none());
    }

    #[test]
    fn test_connect_mode_blocks_other_keys() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_connect_mode();
        let initial_entries = app.entries.len();

        // Navigation keys should not work in connect mode
        handle_key_event(&mut app, KeyCode::Down, KeyModifiers::NONE, &mut scanner);
        handle_key_event(
            &mut app,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        assert_eq!(app.selected_index, 0);
        assert_eq!(app.entries.len(), initial_entries);
    }

    #[test]
    fn test_connect_mode_filter_mode_exclusive() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.enter_filter_mode();
        assert!(app.filter_mode);
        assert!(!app.connect_mode);

        handle_key_event(
            &mut app,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        // Filter mode should block connect mode
        assert!(app.filter_mode);
        assert!(!app.connect_mode);
    }

    #[test]
    fn test_connect_mode_help_mode_exclusive() {
        let mut app = App::new();
        let mut scanner = create_test_scanner();
        app.show_help = true;

        handle_key_event(
            &mut app,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            &mut scanner,
        );

        // Help mode should close first
        assert!(!app.show_help);
        // Connect mode should not be entered when help closes
        assert!(!app.connect_mode);
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use std::path::PathBuf;

    // ==================== Kill Command Validation Tests ====================

    #[test]
    fn test_run_kill_neither_pid_nor_port() {
        let result = run_kill(None, None, None, None, false);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error
            .to_string()
            .contains("Either --pid or --port must be specified"));
    }

    #[test]
    fn test_run_kill_both_pid_and_port() {
        let result = run_kill(Some(123), Some(8080), None, None, false);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Cannot specify both"));
    }

    #[test]
    fn test_run_kill_pid_only() {
        // This will fail because PID likely doesn't exist, but validates the logic
        let result = run_kill(Some(999_999_999), None, None, None, false);
        // Should fail with "not found" not "must be specified"
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("not found") || error.to_string().contains("No process")
        );
    }

    #[test]
    fn test_run_kill_port_only() {
        // This will fail because port likely doesn't exist, but validates the logic
        let result = run_kill(None, Some(65535), None, None, false);
        // Should fail with "not found" not "must be specified"
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("not found") || error.to_string().contains("No process")
        );
    }

    #[test]
    fn test_run_kill_force_flag() {
        // Test that force flag is accepted (will fail on actual kill, but validates parsing)
        let result = run_kill(Some(999_999_999), None, None, None, true);
        assert!(result.is_err()); // Will fail because PID doesn't exist
    }

    #[test]
    fn test_run_kill_remote_host() {
        // Test remote host parsing (will fail on connection, but validates parsing)
        let result = run_kill(
            Some(123),
            None,
            Some("invalid-host".to_string()),
            None,
            false,
        );
        assert!(result.is_err()); // Will fail on connection
    }

    #[test]
    fn test_run_kill_remote_with_key() {
        // Test remote host with key (will fail on connection, but validates parsing)
        let key_path = PathBuf::from("/nonexistent/key");
        let result = run_kill(
            Some(123),
            None,
            Some("invalid-host".to_string()),
            Some(key_path),
            false,
        );
        assert!(result.is_err()); // Will fail on connection
    }

    // ==================== Describe Command Tests ====================

    #[test]
    fn test_run_describe_empty_target() {
        let result = run_describe(String::new(), None, None);
        // Empty string matches all processes (contains("") is always true)
        // So it will succeed and return all processes, not fail
        // This is expected behavior - empty string matches everything
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_describe_nonexistent_port() {
        let result = run_describe("99999".to_string(), None, None);
        // Will fail because port doesn't exist
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("not found") || error.to_string().contains("No process")
        );
    }

    #[test]
    fn test_run_describe_nonexistent_pid() {
        let result = run_describe("999999999".to_string(), None, None);
        // Will fail because PID doesn't exist
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("not found") || error.to_string().contains("No process")
        );
    }

    #[test]
    fn test_run_describe_remote_host() {
        // Test remote host parsing (will fail on connection, but validates parsing)
        let result = run_describe("8080".to_string(), Some("invalid-host".to_string()), None);
        assert!(result.is_err()); // Will fail on connection
    }

    #[test]
    fn test_run_describe_remote_with_key() {
        // Test remote host with key (will fail on connection, but validates parsing)
        let key_path = PathBuf::from("/nonexistent/key");
        let result = run_describe(
            "8080".to_string(),
            Some("invalid-host".to_string()),
            Some(key_path),
        );
        assert!(result.is_err()); // Will fail on connection
    }

    #[test]
    fn test_run_describe_process_name() {
        // Test with process name (will likely fail, but validates logic)
        let result = run_describe("nonexistent_process".to_string(), None, None);
        assert!(result.is_err()); // Will fail because process doesn't exist
    }

    // ==================== Scan Ports Tests ====================

    #[test]
    fn test_scan_ports_local() {
        // Test local scanning (should succeed)
        let result = scan_ports(None, None);
        assert!(result.is_ok());
        // Should return some entries (even if empty)
        let entries = result.unwrap();
        // Just verify it doesn't panic - entries can be empty or have items
        let _ = entries.len();
    }

    #[test]
    fn test_scan_ports_remote_invalid() {
        // Test remote scanning with invalid host
        let result = scan_ports(Some("invalid-host-name-that-does-not-exist"), None);
        assert!(result.is_err()); // Should fail on connection
    }

    #[test]
    fn test_scan_ports_remote_with_key() {
        // Test remote scanning with key
        let key_path = PathBuf::from("/nonexistent/key");
        let result = scan_ports(Some("invalid-host"), Some(&key_path));
        assert!(result.is_err()); // Should fail on connection
    }

    // ==================== Kill Process Force Tests ====================

    #[test]
    fn test_kill_process_force_nonexistent() {
        // Test force kill on nonexistent PID
        let result = kill_process_force(999_999_999);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("not found") || error.to_string().contains("Process"));
    }

    // ==================== Remote Config Parsing Tests ====================

    #[test]
    fn test_remote_config_parse_full() {
        let config = RemoteConfig::parse("user@example.com:2222").unwrap();
        assert_eq!(config.username, "user");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 2222);
    }

    #[test]
    fn test_remote_config_parse_no_port() {
        let config = RemoteConfig::parse("user@example.com").unwrap();
        assert_eq!(config.username, "user");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22); // Default port
    }

    #[test]
    fn test_remote_config_parse_host_only() {
        let config = RemoteConfig::parse("example.com").unwrap();
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22); // Default port
    }

    #[test]
    fn test_remote_config_parse_invalid() {
        let result = RemoteConfig::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn test_remote_config_with_key() {
        let mut config = RemoteConfig::parse("user@host").unwrap();
        let key_path = PathBuf::from("/path/to/key");
        config = config.with_key(key_path.clone());
        assert_eq!(config.key_path, Some(key_path));
    }
}
