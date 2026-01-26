//! Application state management module
//!
//! This module implements the "Model" part of the Model-View-Update pattern.
//! It holds all application state and provides methods to update it.

use regex::Regex;
use std::time::{Duration, Instant};

/// Represents a single port entry with associated process information
#[derive(Debug, Clone)]
pub struct PortEntry {
    /// The local port number
    pub port: u16,
    /// Protocol (TCP or UDP)
    pub protocol: Protocol,
    /// Process ID
    pub pid: u32,
    /// Process name
    pub process_name: String,
    /// CPU usage percentage
    pub cpu_usage: f32,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Memory usage formatted as human-readable string
    pub memory_display: String,
    /// Whether this process has a parent (used for zombie detection)
    pub has_parent: bool,
    /// Whether this entry is flagged as a "zombie" (high CPU + orphaned)
    pub is_zombie: bool,
}

/// Network protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Udp => write!(f, "UDP"),
        }
    }
}

/// Status message types for the footer
#[derive(Debug, Clone)]
pub enum StatusMessage {
    /// Normal informational message
    Info(String),
    /// Success message (e.g., process killed)
    Success(String),
    /// Error message (e.g., permission denied)
    Error(String),
}

/// Column to sort by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    #[default]
    Port,
    Protocol,
    Pid,
    ProcessName,
    CpuUsage,
    MemoryUsage,
}

impl SortColumn {
    /// Cycle to the next sort column
    pub fn next(self) -> Self {
        match self {
            SortColumn::Port => SortColumn::Protocol,
            SortColumn::Protocol => SortColumn::Pid,
            SortColumn::Pid => SortColumn::ProcessName,
            SortColumn::ProcessName => SortColumn::CpuUsage,
            SortColumn::CpuUsage => SortColumn::MemoryUsage,
            SortColumn::MemoryUsage => SortColumn::Port,
        }
    }
}

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    /// Toggle between ascending and descending
    pub fn toggle(self) -> Self {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }
}

/// Main application state
pub struct App {
    /// List of port entries currently being displayed
    pub entries: Vec<PortEntry>,
    /// Currently selected index in the table
    pub selected_index: usize,
    /// Status message to display in the footer
    pub status_message: StatusMessage,
    /// When the status message was set (for auto-clearing)
    pub status_timestamp: Instant,
    /// Duration to display status messages
    pub status_duration: Duration,
    /// Whether the application should quit
    pub should_quit: bool,
    /// Current sort column
    pub sort_column: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
    /// Filter string for process names
    pub filter: String,
    /// Whether filter input mode is active
    pub filter_mode: bool,
    /// Whether to show the help popup
    pub show_help: bool,
    /// Compiled regex for filtering (None if filter is plain text or invalid regex)
    compiled_regex: Option<Regex>,
    /// Whether the current filter is being treated as regex
    pub filter_is_regex: bool,
    /// Remote host being monitored (None for localhost)
    pub remote_host: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a new application instance with default settings
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: 0,
            status_message: StatusMessage::Info("Ready".into()),
            status_timestamp: Instant::now(),
            status_duration: Duration::from_secs(5),
            should_quit: false,
            sort_column: SortColumn::default(),
            sort_order: SortOrder::default(),
            filter: String::new(),
            filter_mode: false,
            show_help: false,
            compiled_regex: None,
            filter_is_regex: false,
            remote_host: None,
        }
    }

    /// Set the remote host being monitored
    pub fn set_remote_host(&mut self, host: Option<String>) {
        self.remote_host = host;
    }

    /// Toggle the help popup
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Update the list of port entries, applying current sort and filter
    pub fn update_entries(&mut self, mut entries: Vec<PortEntry>) {
        // Apply filter
        if !self.filter.is_empty() {
            if let Some(ref regex) = self.compiled_regex {
                // Use regex filtering
                entries.retain(|e| {
                    regex.is_match(&e.process_name)
                        || regex.is_match(&e.port.to_string())
                        || regex.is_match(&e.pid.to_string())
                });
            } else {
                // Use simple substring matching (case-insensitive)
                let filter_lower = self.filter.to_lowercase();
                entries.retain(|e| {
                    e.process_name.to_lowercase().contains(&filter_lower)
                        || e.port.to_string().contains(&filter_lower)
                        || e.pid.to_string().contains(&filter_lower)
                });
            }
        }

        // Apply sort
        self.sort_entries(&mut entries);

        self.entries = entries;
        // Ensure selected index is within bounds
        if !self.entries.is_empty() && self.selected_index >= self.entries.len() {
            self.selected_index = self.entries.len() - 1;
        }
    }

    /// Try to compile the current filter as a regex
    fn try_compile_filter_regex(&mut self) {
        if self.filter.is_empty() {
            self.compiled_regex = None;
            self.filter_is_regex = false;
            return;
        }

        // Try to compile as case-insensitive regex
        match Regex::new(&format!("(?i){}", &self.filter)) {
            Ok(regex) => {
                self.compiled_regex = Some(regex);
                self.filter_is_regex = true;
            }
            Err(_) => {
                // Invalid regex, fall back to literal matching
                self.compiled_regex = None;
                self.filter_is_regex = false;
            }
        }
    }

    /// Sort entries by current sort column and order
    fn sort_entries(&self, entries: &mut [PortEntry]) {
        entries.sort_by(|a, b| {
            let cmp = match self.sort_column {
                SortColumn::Port => a.port.cmp(&b.port),
                SortColumn::Protocol => {
                    format!("{:?}", a.protocol).cmp(&format!("{:?}", b.protocol))
                }
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::ProcessName => a
                    .process_name
                    .to_lowercase()
                    .cmp(&b.process_name.to_lowercase()),
                SortColumn::CpuUsage => a
                    .cpu_usage
                    .partial_cmp(&b.cpu_usage)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::MemoryUsage => a.memory_usage.cmp(&b.memory_usage),
            };

            match self.sort_order {
                SortOrder::Ascending => cmp,
                SortOrder::Descending => cmp.reverse(),
            }
        });
    }

    /// Cycle to the next sort column
    pub fn cycle_sort_column(&mut self) {
        self.sort_column = self.sort_column.next();
        self.set_info(format!("Sorting by: {:?}", self.sort_column));
    }

    /// Toggle sort order
    pub fn toggle_sort_order(&mut self) {
        self.sort_order = self.sort_order.toggle();
        let order_str = match self.sort_order {
            SortOrder::Ascending => "Ascending",
            SortOrder::Descending => "Descending",
        };
        self.set_info(format!("Sort order: {}", order_str));
    }

    /// Sort by specific column (k9s-style: same column toggles order)
    pub fn sort_by_column(&mut self, column: SortColumn) {
        if self.sort_column == column {
            // Same column - toggle order
            self.sort_order = self.sort_order.toggle();
        } else {
            // Different column - set to ascending
            self.sort_column = column;
            self.sort_order = SortOrder::Ascending;
        }
        let order_str = match self.sort_order {
            SortOrder::Ascending => "↑",
            SortOrder::Descending => "↓",
        };
        let col_str = match column {
            SortColumn::Port => "Port",
            SortColumn::Protocol => "Protocol",
            SortColumn::Pid => "PID",
            SortColumn::ProcessName => "Name",
            SortColumn::CpuUsage => "CPU",
            SortColumn::MemoryUsage => "Memory",
        };
        self.set_info(format!("Sort: {}{}", col_str, order_str));
    }

    /// Enter filter mode
    pub fn enter_filter_mode(&mut self) {
        self.filter_mode = true;
        self.set_info("Filter: Type to search, Enter to confirm, Esc to cancel");
    }

    /// Exit filter mode
    pub fn exit_filter_mode(&mut self) {
        self.filter_mode = false;
        self.try_compile_filter_regex();
        if self.filter.is_empty() {
            self.set_info("Filter cleared");
        } else if self.filter_is_regex {
            self.set_info(format!("Regex filter: {}", self.filter));
        } else {
            self.set_info(format!("Filter: {}", self.filter));
        }
    }

    /// Clear filter
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.compiled_regex = None;
        self.filter_is_regex = false;
        self.filter_mode = false;
        self.set_info("Filter cleared");
    }

    /// Add character to filter
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
    }

    /// Remove last character from filter
    pub fn filter_pop(&mut self) {
        self.filter.pop();
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            // Wrap around to bottom
            self.selected_index = self.entries.len() - 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected_index < self.entries.len() - 1 {
            self.selected_index += 1;
        } else {
            // Wrap around to top
            self.selected_index = 0;
        }
    }

    /// Get the currently selected entry
    pub fn selected_entry(&self) -> Option<&PortEntry> {
        self.entries.get(self.selected_index)
    }

    /// Set an info status message
    pub fn set_info(&mut self, message: impl Into<String>) {
        self.status_message = StatusMessage::Info(message.into());
        self.status_timestamp = Instant::now();
    }

    /// Set a success status message
    pub fn set_success(&mut self, message: impl Into<String>) {
        self.status_message = StatusMessage::Success(message.into());
        self.status_timestamp = Instant::now();
    }

    /// Set an error status message
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status_message = StatusMessage::Error(message.into());
        self.status_timestamp = Instant::now();
    }

    /// Check if the status message should be cleared
    pub fn maybe_clear_status(&mut self) {
        if self.status_timestamp.elapsed() > self.status_duration {
            self.status_message =
                StatusMessage::Info("Press ↑/↓ to navigate, Enter to kill, q to quit".into());
        }
    }

    /// Request application quit
    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

/// CPU threshold for zombie detection (40%)
pub const ZOMBIE_CPU_THRESHOLD: f32 = 40.0;

impl PortEntry {
    /// Check if this entry should be flagged as a zombie
    /// A zombie is defined as: CPU > 40% AND no parent process (orphaned)
    pub fn detect_zombie(&mut self) {
        self.is_zombie = self.cpu_usage > ZOMBIE_CPU_THRESHOLD && !self.has_parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ==================== Helper Functions ====================

    fn create_test_entry(port: u16, protocol: Protocol, pid: u32) -> PortEntry {
        PortEntry {
            port,
            protocol,
            pid,
            process_name: format!("process_{}", pid),
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_display: "0 B".into(),
            has_parent: true,
            is_zombie: false,
        }
    }

    fn create_entries(count: usize) -> Vec<PortEntry> {
        (0..count)
            .map(|i| create_test_entry(3000 + i as u16, Protocol::Tcp, i as u32 + 1))
            .collect()
    }

    // ==================== Protocol Tests ====================

    #[test]
    fn test_protocol_display_tcp() {
        assert_eq!(format!("{}", Protocol::Tcp), "TCP");
    }

    #[test]
    fn test_protocol_display_udp() {
        assert_eq!(format!("{}", Protocol::Udp), "UDP");
    }

    #[test]
    fn test_protocol_equality() {
        assert_eq!(Protocol::Tcp, Protocol::Tcp);
        assert_eq!(Protocol::Udp, Protocol::Udp);
        assert_ne!(Protocol::Tcp, Protocol::Udp);
    }

    #[test]
    fn test_protocol_clone() {
        let proto = Protocol::Tcp;
        let cloned = proto;
        assert_eq!(proto, cloned);
    }

    #[test]
    fn test_protocol_debug() {
        assert_eq!(format!("{:?}", Protocol::Tcp), "Tcp");
        assert_eq!(format!("{:?}", Protocol::Udp), "Udp");
    }

    // ==================== PortEntry Tests ====================

    #[test]
    fn test_port_entry_clone() {
        let entry = create_test_entry(8080, Protocol::Tcp, 1234);
        let cloned = entry.clone();
        assert_eq!(entry.port, cloned.port);
        assert_eq!(entry.pid, cloned.pid);
        assert_eq!(entry.process_name, cloned.process_name);
    }

    #[test]
    fn test_port_entry_debug() {
        let entry = create_test_entry(8080, Protocol::Tcp, 1234);
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("8080"));
        assert!(debug_str.contains("1234"));
        assert!(debug_str.contains("Tcp"));
    }

    // ==================== Zombie Detection Tests ====================

    #[test]
    fn test_zombie_detection_high_cpu_no_parent() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: 50.0,
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(entry.is_zombie, "High CPU + no parent should be zombie");
    }

    #[test]
    fn test_zombie_detection_high_cpu_has_parent() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: 50.0,
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: true,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(
            !entry.is_zombie,
            "High CPU but has parent should NOT be zombie"
        );
    }

    #[test]
    fn test_zombie_detection_low_cpu_no_parent() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: 20.0,
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(
            !entry.is_zombie,
            "Low CPU even without parent should NOT be zombie"
        );
    }

    #[test]
    fn test_zombie_detection_exactly_at_threshold() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: ZOMBIE_CPU_THRESHOLD, // Exactly 40%
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(
            !entry.is_zombie,
            "Exactly at threshold (40%) should NOT be zombie"
        );
    }

    #[test]
    fn test_zombie_detection_just_above_threshold() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: ZOMBIE_CPU_THRESHOLD + 0.1, // 40.1%
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(entry.is_zombie, "Just above threshold should be zombie");
    }

    #[test]
    fn test_zombie_detection_max_cpu() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: 100.0,
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(entry.is_zombie, "100% CPU + no parent should be zombie");
    }

    #[test]
    fn test_zombie_detection_zero_cpu() {
        let mut entry = PortEntry {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: "test".into(),
            cpu_usage: 0.0,
            memory_usage: 1024,
            memory_display: "1 KB".into(),
            has_parent: false,
            is_zombie: false,
        };

        entry.detect_zombie();
        assert!(!entry.is_zombie, "0% CPU should NOT be zombie");
    }

    #[test]
    fn test_zombie_threshold_constant() {
        assert_eq!(ZOMBIE_CPU_THRESHOLD, 40.0);
    }

    // ==================== App Creation Tests ====================

    #[test]
    fn test_app_new() {
        let app = App::new();
        assert!(app.entries.is_empty());
        assert_eq!(app.selected_index, 0);
        assert!(!app.should_quit);
        assert_eq!(app.status_duration, Duration::from_secs(5));
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(app.entries.is_empty());
        assert_eq!(app.selected_index, 0);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_app_default_equals_new() {
        let app1 = App::new();
        let app2 = App::default();
        assert_eq!(app1.entries.len(), app2.entries.len());
        assert_eq!(app1.selected_index, app2.selected_index);
        assert_eq!(app1.should_quit, app2.should_quit);
    }

    // ==================== Navigation Tests ====================

    #[test]
    fn test_navigation_empty_list() {
        let mut app = App::new();
        // Should not panic on empty list
        app.select_next();
        assert_eq!(app.selected_index, 0);
        app.select_previous();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_navigation_single_item() {
        let mut app = App::new();
        app.entries = create_entries(1);

        app.select_next();
        assert_eq!(app.selected_index, 0, "Single item: next should wrap to 0");

        app.select_previous();
        assert_eq!(
            app.selected_index, 0,
            "Single item: previous should wrap to 0"
        );
    }

    #[test]
    fn test_navigation_two_items() {
        let mut app = App::new();
        app.entries = create_entries(2);

        assert_eq!(app.selected_index, 0);
        app.select_next();
        assert_eq!(app.selected_index, 1);
        app.select_next();
        assert_eq!(app.selected_index, 0, "Should wrap to beginning");
        app.select_previous();
        assert_eq!(app.selected_index, 1, "Should wrap to end");
    }

    #[test]
    fn test_navigation_wrap_around_forward() {
        let mut app = App::new();
        app.entries = create_entries(5);
        app.selected_index = 4; // Last item

        app.select_next();
        assert_eq!(app.selected_index, 0, "Should wrap to first");
    }

    #[test]
    fn test_navigation_wrap_around_backward() {
        let mut app = App::new();
        app.entries = create_entries(5);
        app.selected_index = 0;

        app.select_previous();
        assert_eq!(app.selected_index, 4, "Should wrap to last");
    }

    #[test]
    fn test_navigation_many_items() {
        let mut app = App::new();
        app.entries = create_entries(100);

        // Navigate through all items forward
        for i in 0..100 {
            assert_eq!(app.selected_index, i);
            app.select_next();
        }
        assert_eq!(app.selected_index, 0, "Should wrap after 100");

        // Navigate through all items backward
        for i in (0..100).rev() {
            app.select_previous();
            assert_eq!(app.selected_index, i);
        }
    }

    // ==================== Update Entries Tests ====================

    #[test]
    fn test_update_entries_empty_to_filled() {
        let mut app = App::new();
        assert!(app.entries.is_empty());

        app.update_entries(create_entries(5));
        assert_eq!(app.entries.len(), 5);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_update_entries_filled_to_empty() {
        let mut app = App::new();
        app.entries = create_entries(5);
        app.selected_index = 3;

        app.update_entries(Vec::new());
        assert!(app.entries.is_empty());
        assert_eq!(
            app.selected_index, 3,
            "Index preserved when empty (no entries to clamp)"
        );
    }

    #[test]
    fn test_update_entries_index_adjustment() {
        let mut app = App::new();
        app.entries = create_entries(10);
        app.selected_index = 9; // Last item

        // Shrink list - index should be adjusted
        app.update_entries(create_entries(5));
        assert_eq!(
            app.selected_index, 4,
            "Index should be clamped to new last item"
        );
    }

    #[test]
    fn test_update_entries_index_preserved() {
        let mut app = App::new();
        app.entries = create_entries(10);
        app.selected_index = 3;

        // New list is same size or larger - index should be preserved
        app.update_entries(create_entries(10));
        assert_eq!(app.selected_index, 3);

        app.update_entries(create_entries(20));
        assert_eq!(app.selected_index, 3);
    }

    #[test]
    fn test_update_entries_boundary_index() {
        let mut app = App::new();
        app.entries = create_entries(5);
        app.selected_index = 4;

        // Shrink to exactly the selected index + 1
        app.update_entries(create_entries(5));
        assert_eq!(app.selected_index, 4, "At boundary, should stay");

        // Shrink to exactly the selected index
        app.update_entries(create_entries(4));
        assert_eq!(app.selected_index, 3, "Past boundary, should clamp");
    }

    // ==================== Selected Entry Tests ====================

    #[test]
    fn test_selected_entry_empty_list() {
        let app = App::new();
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_selected_entry_valid() {
        let mut app = App::new();
        app.entries = create_entries(3);
        app.selected_index = 1;

        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.port, 3001);
    }

    #[test]
    fn test_selected_entry_first() {
        let mut app = App::new();
        app.entries = create_entries(3);
        app.selected_index = 0;

        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.port, 3000);
    }

    #[test]
    fn test_selected_entry_last() {
        let mut app = App::new();
        app.entries = create_entries(3);
        app.selected_index = 2;

        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.port, 3002);
    }

    // ==================== Status Message Tests ====================

    #[test]
    fn test_status_message_info() {
        let mut app = App::new();
        app.set_info("Test info message");

        match &app.status_message {
            StatusMessage::Info(msg) => assert_eq!(msg, "Test info message"),
            _ => panic!("Expected Info variant"),
        }
    }

    #[test]
    fn test_status_message_success() {
        let mut app = App::new();
        app.set_success("Process killed");

        match &app.status_message {
            StatusMessage::Success(msg) => assert_eq!(msg, "Process killed"),
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_status_message_error() {
        let mut app = App::new();
        app.set_error("Permission denied");

        match &app.status_message {
            StatusMessage::Error(msg) => assert_eq!(msg, "Permission denied"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_status_message_timestamp_updated() {
        let mut app = App::new();
        let before = Instant::now();

        thread::sleep(Duration::from_millis(10));
        app.set_info("New message");

        assert!(app.status_timestamp > before);
    }

    #[test]
    fn test_status_message_variants_debug() {
        let info = StatusMessage::Info("test".into());
        let success = StatusMessage::Success("test".into());
        let error = StatusMessage::Error("test".into());

        assert!(format!("{:?}", info).contains("Info"));
        assert!(format!("{:?}", success).contains("Success"));
        assert!(format!("{:?}", error).contains("Error"));
    }

    #[test]
    fn test_status_message_clone() {
        let original = StatusMessage::Success("test".into());
        let cloned = original.clone();

        match cloned {
            StatusMessage::Success(msg) => assert_eq!(msg, "test"),
            _ => panic!("Clone should preserve variant"),
        }
    }

    #[test]
    fn test_maybe_clear_status_not_expired() {
        let mut app = App::new();
        app.set_error("Error message");

        // Immediately check - should not clear
        app.maybe_clear_status();

        match &app.status_message {
            StatusMessage::Error(msg) => assert_eq!(msg, "Error message"),
            _ => panic!("Should still be error"),
        }
    }

    // ==================== Quit Tests ====================

    #[test]
    fn test_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);

        app.quit();
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_idempotent() {
        let mut app = App::new();
        app.quit();
        app.quit();
        app.quit();
        assert!(app.should_quit);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_workflow() {
        let mut app = App::new();

        // Start empty
        assert!(app.entries.is_empty());
        assert!(app.selected_entry().is_none());

        // Add entries
        app.update_entries(create_entries(5));
        assert_eq!(app.entries.len(), 5);

        // Navigate
        app.select_next();
        app.select_next();
        assert_eq!(app.selected_index, 2);

        // Check selected
        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.port, 3002);

        // Update with fewer entries
        app.update_entries(create_entries(2));
        assert_eq!(app.selected_index, 1); // Clamped

        // Status messages
        app.set_success("Done!");
        app.set_error("Oops!");

        // Quit
        app.quit();
        assert!(app.should_quit);
    }

    #[test]
    fn test_rapid_navigation() {
        let mut app = App::new();
        app.entries = create_entries(10);

        // Rapid forward navigation
        for _ in 0..1000 {
            app.select_next();
        }
        assert!(app.selected_index < 10);

        // Rapid backward navigation
        for _ in 0..1000 {
            app.select_previous();
        }
        assert!(app.selected_index < 10);
    }

    // ==================== Sort Column Tests ====================

    #[test]
    fn test_sort_column_default() {
        assert_eq!(SortColumn::default(), SortColumn::Port);
    }

    #[test]
    fn test_sort_column_cycle() {
        assert_eq!(SortColumn::Port.next(), SortColumn::Protocol);
        assert_eq!(SortColumn::Protocol.next(), SortColumn::Pid);
        assert_eq!(SortColumn::Pid.next(), SortColumn::ProcessName);
        assert_eq!(SortColumn::ProcessName.next(), SortColumn::CpuUsage);
        assert_eq!(SortColumn::CpuUsage.next(), SortColumn::MemoryUsage);
        assert_eq!(SortColumn::MemoryUsage.next(), SortColumn::Port); // Wraps
    }

    #[test]
    fn test_sort_column_full_cycle() {
        let mut col = SortColumn::Port;
        for _ in 0..6 {
            col = col.next();
        }
        assert_eq!(col, SortColumn::Port); // Back to start
    }

    // ==================== Sort Order Tests ====================

    #[test]
    fn test_sort_order_default() {
        assert_eq!(SortOrder::default(), SortOrder::Ascending);
    }

    #[test]
    fn test_sort_order_toggle() {
        assert_eq!(SortOrder::Ascending.toggle(), SortOrder::Descending);
        assert_eq!(SortOrder::Descending.toggle(), SortOrder::Ascending);
    }

    #[test]
    fn test_sort_order_double_toggle() {
        let order = SortOrder::Ascending;
        assert_eq!(order.toggle().toggle(), SortOrder::Ascending);
    }

    // ==================== App Sorting Tests ====================

    #[test]
    fn test_app_cycle_sort_column() {
        let mut app = App::new();
        assert_eq!(app.sort_column, SortColumn::Port);

        app.cycle_sort_column();
        assert_eq!(app.sort_column, SortColumn::Protocol);

        app.cycle_sort_column();
        assert_eq!(app.sort_column, SortColumn::Pid);
    }

    #[test]
    fn test_app_toggle_sort_order() {
        let mut app = App::new();
        assert_eq!(app.sort_order, SortOrder::Ascending);

        app.toggle_sort_order();
        assert_eq!(app.sort_order, SortOrder::Descending);

        app.toggle_sort_order();
        assert_eq!(app.sort_order, SortOrder::Ascending);
    }

    #[test]
    fn test_sorting_by_port_ascending() {
        let mut app = App::new();
        app.sort_column = SortColumn::Port;
        app.sort_order = SortOrder::Ascending;

        let entries = vec![
            create_test_entry(8080, Protocol::Tcp, 1),
            create_test_entry(3000, Protocol::Tcp, 2),
            create_test_entry(5000, Protocol::Tcp, 3),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries[0].port, 3000);
        assert_eq!(app.entries[1].port, 5000);
        assert_eq!(app.entries[2].port, 8080);
    }

    #[test]
    fn test_sorting_by_port_descending() {
        let mut app = App::new();
        app.sort_column = SortColumn::Port;
        app.sort_order = SortOrder::Descending;

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(8080, Protocol::Tcp, 2),
            create_test_entry(5000, Protocol::Tcp, 3),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries[0].port, 8080);
        assert_eq!(app.entries[1].port, 5000);
        assert_eq!(app.entries[2].port, 3000);
    }

    #[test]
    fn test_sorting_by_pid() {
        let mut app = App::new();
        app.sort_column = SortColumn::Pid;
        app.sort_order = SortOrder::Ascending;

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 300),
            create_test_entry(3001, Protocol::Tcp, 100),
            create_test_entry(3002, Protocol::Tcp, 200),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries[0].pid, 100);
        assert_eq!(app.entries[1].pid, 200);
        assert_eq!(app.entries[2].pid, 300);
    }

    #[test]
    fn test_sorting_by_memory() {
        let mut app = App::new();
        app.sort_column = SortColumn::MemoryUsage;
        app.sort_order = SortOrder::Descending;

        let mut entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
            create_test_entry(3002, Protocol::Tcp, 3),
        ];
        entries[0].memory_usage = 1000;
        entries[1].memory_usage = 3000;
        entries[2].memory_usage = 2000;

        app.update_entries(entries);

        assert_eq!(app.entries[0].memory_usage, 3000);
        assert_eq!(app.entries[1].memory_usage, 2000);
        assert_eq!(app.entries[2].memory_usage, 1000);
    }

    #[test]
    fn test_sorting_by_cpu() {
        let mut app = App::new();
        app.sort_column = SortColumn::CpuUsage;
        app.sort_order = SortOrder::Descending;

        let mut entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
            create_test_entry(3002, Protocol::Tcp, 3),
        ];
        entries[0].cpu_usage = 10.0;
        entries[1].cpu_usage = 50.0;
        entries[2].cpu_usage = 25.0;

        app.update_entries(entries);

        assert_eq!(app.entries[0].cpu_usage, 50.0);
        assert_eq!(app.entries[1].cpu_usage, 25.0);
        assert_eq!(app.entries[2].cpu_usage, 10.0);
    }

    // ==================== Filter Tests ====================

    #[test]
    fn test_filter_mode_default() {
        let app = App::new();
        assert!(!app.filter_mode);
        assert!(app.filter.is_empty());
    }

    #[test]
    fn test_enter_filter_mode() {
        let mut app = App::new();
        app.enter_filter_mode();
        assert!(app.filter_mode);
    }

    #[test]
    fn test_exit_filter_mode() {
        let mut app = App::new();
        app.enter_filter_mode();
        app.filter_push('t');
        app.exit_filter_mode();
        assert!(!app.filter_mode);
        assert_eq!(app.filter, "t");
    }

    #[test]
    fn test_clear_filter() {
        let mut app = App::new();
        app.filter = "test".into();
        app.filter_mode = true;

        app.clear_filter();

        assert!(app.filter.is_empty());
        assert!(!app.filter_mode);
    }

    #[test]
    fn test_filter_push_pop() {
        let mut app = App::new();

        app.filter_push('h');
        app.filter_push('e');
        app.filter_push('l');
        app.filter_push('l');
        app.filter_push('o');
        assert_eq!(app.filter, "hello");

        app.filter_pop();
        assert_eq!(app.filter, "hell");

        app.filter_pop();
        app.filter_pop();
        assert_eq!(app.filter, "he");
    }

    #[test]
    fn test_filter_pop_empty() {
        let mut app = App::new();
        app.filter_pop(); // Should not panic
        assert!(app.filter.is_empty());
    }

    #[test]
    fn test_filter_by_process_name() {
        let mut app = App::new();
        app.filter = "node".into();

        let mut entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
            create_test_entry(3002, Protocol::Tcp, 3),
        ];
        entries[0].process_name = "node".into();
        entries[1].process_name = "python".into();
        entries[2].process_name = "nodejs".into();

        app.update_entries(entries);

        assert_eq!(app.entries.len(), 2); // "node" and "nodejs"
        assert!(app.entries.iter().all(|e| e.process_name.contains("node")));
    }

    #[test]
    fn test_filter_by_port() {
        let mut app = App::new();
        app.filter = "3001".into();

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
            create_test_entry(3002, Protocol::Tcp, 3),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0].port, 3001);
    }

    #[test]
    fn test_filter_by_pid() {
        let mut app = App::new();
        app.filter = "123".into();

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 123),
            create_test_entry(3001, Protocol::Tcp, 456),
            create_test_entry(3002, Protocol::Tcp, 1234),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries.len(), 2); // 123 and 1234
    }

    #[test]
    fn test_filter_case_insensitive() {
        let mut app = App::new();
        app.filter = "NODE".into();

        let mut entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
        ];
        entries[0].process_name = "node".into();
        entries[1].process_name = "python".into();

        app.update_entries(entries);

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0].process_name, "node");
    }

    #[test]
    fn test_filter_no_matches() {
        let mut app = App::new();
        app.filter = "nonexistent".into();

        let entries = create_entries(5);
        app.update_entries(entries);

        assert!(app.entries.is_empty());
    }

    #[test]
    fn test_filter_empty_string() {
        let mut app = App::new();
        app.filter = String::new();

        let entries = create_entries(5);
        app.update_entries(entries);

        assert_eq!(app.entries.len(), 5); // No filtering applied
    }

    #[test]
    fn test_sort_and_filter_combined() {
        let mut app = App::new();
        app.filter = "process".into();
        app.sort_column = SortColumn::Port;
        app.sort_order = SortOrder::Descending;

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(5000, Protocol::Tcp, 2),
            create_test_entry(4000, Protocol::Tcp, 3),
        ];

        app.update_entries(entries);

        // All entries match "process_X" pattern, sorted descending by port
        assert_eq!(app.entries.len(), 3);
        assert_eq!(app.entries[0].port, 5000);
        assert_eq!(app.entries[1].port, 4000);
        assert_eq!(app.entries[2].port, 3000);
    }

    #[test]
    fn test_regex_filter_basic() {
        let mut app = App::new();
        app.filter = "process_[12]".into();
        app.try_compile_filter_regex();

        assert!(app.filter_is_regex);
        assert!(app.compiled_regex.is_some());

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1), // process_1
            create_test_entry(3001, Protocol::Tcp, 2), // process_2
            create_test_entry(3002, Protocol::Tcp, 3), // process_3
        ];

        app.update_entries(entries);

        // Only process_1 and process_2 should match
        assert_eq!(app.entries.len(), 2);
    }

    #[test]
    fn test_regex_filter_case_insensitive() {
        let mut app = App::new();
        app.filter = "PROCESS".into();
        app.try_compile_filter_regex();

        assert!(app.filter_is_regex);

        let entries = create_entries(3);
        app.update_entries(entries);

        // All entries should match (case-insensitive)
        assert_eq!(app.entries.len(), 3);
    }

    #[test]
    fn test_regex_filter_port_pattern() {
        let mut app = App::new();
        app.filter = "^300[01]$".into(); // Match ports 3000 or 3001
        app.try_compile_filter_regex();

        assert!(app.filter_is_regex);

        let entries = vec![
            create_test_entry(3000, Protocol::Tcp, 1),
            create_test_entry(3001, Protocol::Tcp, 2),
            create_test_entry(3002, Protocol::Tcp, 3),
        ];

        app.update_entries(entries);

        assert_eq!(app.entries.len(), 2);
    }

    #[test]
    fn test_regex_filter_invalid_falls_back() {
        let mut app = App::new();
        app.filter = "[invalid".into(); // Invalid regex (unclosed bracket)
        app.try_compile_filter_regex();

        // Should fall back to literal matching
        assert!(!app.filter_is_regex);
        assert!(app.compiled_regex.is_none());
    }

    #[test]
    fn test_regex_filter_cleared() {
        let mut app = App::new();
        app.filter = "test".into();
        app.try_compile_filter_regex();
        assert!(app.filter_is_regex);

        app.clear_filter();

        assert!(app.filter.is_empty());
        assert!(!app.filter_is_regex);
        assert!(app.compiled_regex.is_none());
    }
}
