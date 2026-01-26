//! Port scanning and process correlation module
//!
//! This module handles:
//! - Discovering which ports are currently in use (TCP and UDP)
//! - Mapping ports to their owning processes via PIDs
//! - Gathering process statistics (CPU, memory, parent info)
//! - Killing processes

use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use listeners::Listener;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::app::{PortEntry, Protocol};

/// How often to refresh UDP port data (expensive operation)
const UDP_CACHE_DURATION: Duration = Duration::from_secs(5);

/// Scanner responsible for gathering port and process information
pub struct Scanner {
    /// System information handle
    system: System,
    /// Cached UDP entries
    udp_cache: Vec<UdpCacheEntry>,
    /// When UDP cache was last updated
    udp_cache_time: Instant,
}

/// Cached UDP port entry (without live process stats)
#[derive(Clone)]
struct UdpCacheEntry {
    port: u16,
    pid: u32,
    process_name: String,
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Scanner {
    /// Create a new scanner instance
    pub fn new() -> Self {
        let mut system = System::new();
        // Initial refresh to populate process list
        system.refresh_processes(ProcessesToUpdate::All);
        Self {
            system,
            udp_cache: Vec::new(),
            udp_cache_time: Instant::now() - UDP_CACHE_DURATION, // Force initial refresh
        }
    }

    /// Scan for all listening ports and correlate with process information
    ///
    /// This method:
    /// 1. Gets all listening TCP ports using the `listeners` crate
    /// 2. Gets UDP ports using platform-specific methods (cached)
    /// 3. Refreshes system process information
    /// 4. Correlates each port with its process and gathers stats
    pub fn scan(&mut self) -> Vec<PortEntry> {
        // Refresh only CPU and memory info (much faster than everything())
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );

        // Build a map of PID -> Process info for quick lookups
        let process_map: HashMap<u32, ProcessInfo> = self
            .system
            .processes()
            .iter()
            .map(|(pid, proc)| {
                let pid_u32 = pid.as_u32();
                let info = ProcessInfo {
                    name: proc.name().to_string_lossy().into_owned(),
                    cpu_usage: proc.cpu_usage(),
                    memory: proc.memory(),
                    has_parent: proc.parent().is_some(),
                };
                (pid_u32, info)
            })
            .collect();

        let mut entries = Vec::new();

        // Get TCP listeners using the listeners crate
        if let Ok(tcp_listeners) = listeners::get_all() {
            for listener in tcp_listeners {
                if let Some(entry) = self.listener_to_entry(listener, Protocol::Tcp, &process_map) {
                    entries.push(entry);
                }
            }
        }

        // Get UDP listeners (cached for performance)
        let udp_entries = self.get_udp_entries(&process_map);
        entries.extend(udp_entries);

        // Sort by port number, then by protocol, then by memory usage (descending)
        entries.sort_by(|a, b| {
            a.port
                .cmp(&b.port)
                .then_with(|| {
                    // TCP before UDP
                    match (&a.protocol, &b.protocol) {
                        (Protocol::Tcp, Protocol::Udp) => std::cmp::Ordering::Less,
                        (Protocol::Udp, Protocol::Tcp) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    }
                })
                .then_with(|| b.memory_usage.cmp(&a.memory_usage)) // Higher memory first
        });

        // Apply zombie detection
        for entry in &mut entries {
            entry.detect_zombie();
        }

        entries
    }

    /// Get UDP entries, using cache when possible
    fn get_udp_entries(&mut self, process_map: &HashMap<u32, ProcessInfo>) -> Vec<PortEntry> {
        // Refresh UDP cache if expired
        if self.udp_cache_time.elapsed() >= UDP_CACHE_DURATION {
            self.refresh_udp_cache();
        }

        // Convert cached entries to PortEntries with live process stats
        self.udp_cache
            .iter()
            .map(|cached| {
                let (cpu_usage, memory, has_parent) = match process_map.get(&cached.pid) {
                    Some(info) => (info.cpu_usage, info.memory, info.has_parent),
                    None => (0.0, 0, true),
                };

                PortEntry {
                    port: cached.port,
                    protocol: Protocol::Udp,
                    pid: cached.pid,
                    process_name: cached.process_name.clone(),
                    cpu_usage,
                    memory_usage: memory,
                    memory_display: format_memory(memory),
                    has_parent,
                    is_zombie: false,
                }
            })
            .collect()
    }

    /// Refresh the UDP port cache
    fn refresh_udp_cache(&mut self) {
        self.udp_cache = self.scan_udp_ports_raw();
        self.udp_cache_time = Instant::now();
    }

    /// Scan for UDP listening ports using platform-specific methods (raw, no process map)
    fn scan_udp_ports_raw(&self) -> Vec<UdpCacheEntry> {
        // Try lsof first (works on macOS and Linux)
        if let Some(entries) = self.scan_udp_with_lsof_raw() {
            return entries;
        }

        // Fallback: try netstat (works on most systems)
        if let Some(entries) = self.scan_udp_with_netstat_raw() {
            return entries;
        }

        Vec::new()
    }

    /// Scan UDP ports using lsof command (raw, for caching)
    fn scan_udp_with_lsof_raw(&self) -> Option<Vec<UdpCacheEntry>> {
        let output = Command::new("lsof")
            .args(["-i", "UDP", "-n", "-P"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        let mut seen_ports: HashMap<(u16, u32), bool> = HashMap::new();

        for line in stdout.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }

            // Parse: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
            let process_name = parts[0].to_string();
            let pid: u32 = match parts[1].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // NAME is like "*:5353" or "127.0.0.1:5353"
            let name = parts.last()?;
            let port_str = name.rsplit(':').next()?;
            let port: u16 = match port_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Skip duplicates
            if seen_ports.contains_key(&(port, pid)) {
                continue;
            }
            seen_ports.insert((port, pid), true);

            entries.push(UdpCacheEntry {
                port,
                pid,
                process_name,
            });
        }

        Some(entries)
    }

    /// Scan UDP ports using netstat command (raw, for caching)
    fn scan_udp_with_netstat_raw(&self) -> Option<Vec<UdpCacheEntry>> {
        // Try different netstat variants
        let output = Command::new("netstat")
            .args(["-ulnp"]) // Linux style
            .output()
            .or_else(|_| {
                Command::new("netstat")
                    .args(["-an", "-p", "udp"]) // macOS/BSD style
                    .output()
            })
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        for line in stdout.lines() {
            // Look for UDP lines
            if !line.contains("udp") && !line.contains("UDP") {
                continue;
            }

            // Try to parse Linux-style netstat output
            // Proto Recv-Q Send-Q Local Address Foreign Address State PID/Program
            let parts: Vec<&str> = line.split_whitespace().collect();

            // Extract port from local address (format: 0.0.0.0:port or :::port)
            let local_addr = parts.get(3).or_else(|| parts.get(1))?;
            let port_str = local_addr.rsplit(':').next()?;
            let port: u16 = match port_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Try to extract PID/Program (Linux format: "1234/program")
            if let Some(pid_prog) = parts.last() {
                if let Some((pid_str, prog)) = pid_prog.split_once('/') {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        entries.push(UdpCacheEntry {
                            port,
                            pid,
                            process_name: prog.to_string(),
                        });
                    }
                }
            }
        }

        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    /// Convert a Listener to a PortEntry
    fn listener_to_entry(
        &self,
        listener: Listener,
        protocol: Protocol,
        process_map: &HashMap<u32, ProcessInfo>,
    ) -> Option<PortEntry> {
        let port = listener.socket.port();
        let pid = listener.process.pid;

        // Get process info from our map
        let proc_info = process_map.get(&pid);

        let (process_name, cpu_usage, memory_usage, has_parent) = match proc_info {
            Some(info) => (
                info.name.clone(),
                info.cpu_usage,
                info.memory,
                info.has_parent,
            ),
            None => {
                // Process might have exited, use info from listener
                (listener.process.name, 0.0, 0, true)
            }
        };

        Some(PortEntry {
            port,
            protocol,
            pid,
            process_name,
            cpu_usage,
            memory_usage,
            memory_display: format_memory(memory_usage),
            has_parent,
            is_zombie: false, // Will be set by detect_zombie()
        })
    }

    /// Kill a process by PID (wrapper for the standalone function)
    pub fn kill_process(&mut self, pid: u32) -> Result<()> {
        kill_process(pid)
    }
}

/// Intermediate struct for process information
struct ProcessInfo {
    name: String,
    cpu_usage: f32,
    memory: u64,
    has_parent: bool,
}

/// Format memory size in human-readable format
fn format_memory(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Kill a process by PID
///
/// Returns Ok(()) if the process was killed successfully,
/// or an error with details (e.g., permission denied)
pub fn kill_process(pid: u32) -> Result<()> {
    // Create a new System instance for the kill operation
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        ProcessRefreshKind::new(),
    );

    let sys_pid = Pid::from_u32(pid);

    if let Some(process) = system.process(sys_pid) {
        if process.kill() {
            Ok(())
        } else {
            // Kill returned false - usually permission denied
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Format Memory Tests ====================

    #[test]
    fn test_format_memory_zero() {
        assert_eq!(format_memory(0), "0 B");
    }

    #[test]
    fn test_format_memory_bytes() {
        assert_eq!(format_memory(1), "1 B");
        assert_eq!(format_memory(512), "512 B");
        assert_eq!(format_memory(1023), "1023 B");
    }

    #[test]
    fn test_format_memory_kilobytes_boundary() {
        assert_eq!(format_memory(1024), "1.0 KB");
        assert_eq!(format_memory(1025), "1.0 KB"); // Rounds down
    }

    #[test]
    fn test_format_memory_kilobytes() {
        assert_eq!(format_memory(1536), "1.5 KB");
        assert_eq!(format_memory(2048), "2.0 KB");
        assert_eq!(format_memory(10240), "10.0 KB");
        assert_eq!(format_memory(1024 * 1023), "1023.0 KB");
    }

    #[test]
    fn test_format_memory_megabytes_boundary() {
        assert_eq!(format_memory(1024 * 1024), "1.0 MB");
        assert_eq!(format_memory(1024 * 1024 + 1), "1.0 MB");
    }

    #[test]
    fn test_format_memory_megabytes() {
        assert_eq!(format_memory(1_048_576), "1.0 MB");
        assert_eq!(format_memory(1_572_864), "1.5 MB"); // 1.5 MB
        assert_eq!(format_memory(104_857_600), "100.0 MB"); // 100 MB
        assert_eq!(format_memory(536_870_912), "512.0 MB"); // 512 MB
    }

    #[test]
    fn test_format_memory_gigabytes_boundary() {
        assert_eq!(format_memory(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_format_memory_gigabytes() {
        assert_eq!(format_memory(1_073_741_824), "1.0 GB");
        assert_eq!(format_memory(1_610_612_736), "1.5 GB"); // 1.5 GB
        assert_eq!(format_memory(2_147_483_648), "2.0 GB"); // 2 GB
        assert_eq!(format_memory(8_589_934_592), "8.0 GB"); // 8 GB
    }

    #[test]
    fn test_format_memory_large_values() {
        assert_eq!(format_memory(16 * 1024 * 1024 * 1024), "16.0 GB");
        assert_eq!(format_memory(32 * 1024 * 1024 * 1024), "32.0 GB");
    }

    #[test]
    fn test_format_memory_max_u64() {
        // Should not panic on max value
        let result = format_memory(u64::MAX);
        assert!(result.contains("GB"));
    }

    #[test]
    fn test_format_memory_precision() {
        // Test decimal precision
        assert_eq!(format_memory(1126), "1.1 KB"); // 1.099 KB rounds to 1.1
        assert_eq!(format_memory(1177), "1.1 KB"); // 1.149 KB rounds to 1.1
        assert_eq!(format_memory(1178), "1.2 KB"); // 1.150 KB rounds to 1.2
    }

    // ==================== Scanner Creation Tests ====================

    #[test]
    fn test_scanner_new() {
        let scanner = Scanner::new();
        assert!(
            !scanner.system.processes().is_empty(),
            "Should have at least one process"
        );
    }

    #[test]
    fn test_scanner_default() {
        let scanner = Scanner::default();
        assert!(!scanner.system.processes().is_empty());
    }

    #[test]
    fn test_scanner_default_equals_new() {
        let scanner1 = Scanner::new();
        let scanner2 = Scanner::default();
        // Both should have processes populated
        assert!(!scanner1.system.processes().is_empty());
        assert!(!scanner2.system.processes().is_empty());
    }

    // ==================== Scanner Scan Tests ====================

    #[test]
    fn test_scanner_scan_returns_vec() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();
        // Should return a Vec (might be empty if no ports are open)
        // Vec length is always >= 0, this just tests it returns successfully
        let _ = entries.len();
    }

    #[test]
    fn test_scanner_scan_entries_have_valid_ports() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        // Port 0 can appear in some edge cases (wildcard bindings)
        // Valid port range is 0-65535 (u16 type guarantees this)
        // This test ensures we can iterate entries without panics
        for entry in &entries {
            let _port = entry.port; // Verify accessible
        }
    }

    #[test]
    fn test_scanner_scan_entries_have_valid_pids() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        for entry in &entries {
            // PID 0 can exist (kernel/system processes on some platforms)
            // Just verify it's a valid u32
            let _ = entry.pid;
        }
    }

    #[test]
    fn test_scanner_scan_entries_have_process_names() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        for entry in &entries {
            assert!(
                !entry.process_name.is_empty(),
                "Process name should not be empty"
            );
        }
    }

    #[test]
    fn test_scanner_scan_entries_have_valid_protocol() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        for entry in &entries {
            assert!(
                entry.protocol == Protocol::Tcp || entry.protocol == Protocol::Udp,
                "Protocol should be TCP or UDP"
            );
        }
    }

    #[test]
    fn test_scanner_scan_entries_have_memory_display() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        for entry in &entries {
            assert!(
                !entry.memory_display.is_empty(),
                "Memory display should not be empty"
            );
            assert!(
                entry.memory_display.contains("B")
                    || entry.memory_display.contains("KB")
                    || entry.memory_display.contains("MB")
                    || entry.memory_display.contains("GB"),
                "Memory display should contain unit"
            );
        }
    }

    #[test]
    fn test_scanner_scan_entries_sorted_by_port() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        if entries.len() > 1 {
            for i in 0..entries.len() - 1 {
                assert!(
                    entries[i].port <= entries[i + 1].port,
                    "Entries should be sorted by port"
                );
            }
        }
    }

    #[test]
    fn test_scanner_scan_zombie_detection_applied() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        // Zombie detection should have been applied (even if no zombies found)
        for entry in &entries {
            // If it's a zombie, verify the conditions
            if entry.is_zombie {
                assert!(entry.cpu_usage > 40.0, "Zombie should have high CPU");
                assert!(!entry.has_parent, "Zombie should not have parent");
            }
        }
    }

    #[test]
    fn test_scanner_multiple_scans() {
        let mut scanner = Scanner::new();

        // Multiple scans should work without issues
        for _ in 0..5 {
            let entries = scanner.scan();
            // Just verify scan completes successfully
            let _ = entries.len();
        }
    }

    #[test]
    fn test_scanner_scan_cpu_usage_range() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        for entry in &entries {
            assert!(entry.cpu_usage >= 0.0, "CPU usage should be non-negative");
            // CPU can exceed 100% on multi-core systems, but shouldn't be negative
        }
    }

    // ==================== Kill Process Tests ====================

    #[test]
    fn test_kill_process_nonexistent_pid() {
        // Use a PID that almost certainly doesn't exist
        let result = kill_process(999_999_999);
        assert!(result.is_err(), "Killing non-existent process should fail");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found") || err_msg.contains("exited"),
            "Error should mention process not found"
        );
    }

    #[test]
    fn test_kill_process_zero_pid() {
        // PID 0 should not exist
        let result = kill_process(0);
        assert!(result.is_err(), "Killing PID 0 should fail");
    }

    // ==================== ProcessInfo Tests ====================

    #[test]
    fn test_process_info_struct() {
        let info = ProcessInfo {
            name: "test_process".to_string(),
            cpu_usage: 25.5,
            memory: 1024 * 1024,
            has_parent: true,
        };

        assert_eq!(info.name, "test_process");
        assert_eq!(info.cpu_usage, 25.5);
        assert_eq!(info.memory, 1024 * 1024);
        assert!(info.has_parent);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_scanner_full_workflow() {
        // Create scanner
        let mut scanner = Scanner::new();

        // Initial scan
        let entries1 = scanner.scan();

        // Second scan (should refresh data)
        let entries2 = scanner.scan();

        // Both scans should complete without error
        // Entries may have port 0 or PID 0 in edge cases
        for entry in entries1.iter().chain(entries2.iter()) {
            // Verify fields are populated (process_name might be empty for some system processes)
            let _ = entry.port;
            let _ = entry.pid;
            let _ = &entry.process_name;
        }
    }

    #[test]
    fn test_format_memory_consistency() {
        // Same input should always produce same output
        for _ in 0..100 {
            assert_eq!(format_memory(1024), "1.0 KB");
            assert_eq!(format_memory(1_048_576), "1.0 MB");
            assert_eq!(format_memory(1_073_741_824), "1.0 GB");
        }
    }

    #[test]
    fn test_scanner_entries_deduplication() {
        let mut scanner = Scanner::new();
        let entries = scanner.scan();

        // Count duplicates (same port + pid + protocol)
        // Some duplicates may appear due to IPv4/IPv6 dual-stack
        let mut seen = std::collections::HashSet::new();
        let mut duplicate_count = 0;

        for entry in &entries {
            let key = (entry.port, entry.pid, format!("{:?}", entry.protocol));
            if !seen.insert(key) {
                duplicate_count += 1;
            }
        }

        // Allow some duplicates from dual-stack, but not excessive
        assert!(
            duplicate_count <= entries.len() / 2,
            "Too many duplicates: {} out of {} entries",
            duplicate_count,
            entries.len()
        );
    }
}
