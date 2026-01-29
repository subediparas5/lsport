//! Remote SSH scanning module
//!
//! This module provides functionality to scan ports on remote machines via SSH.

use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use ssh2::Session;

use crate::app::{PortEntry, Protocol};

/// Remote host connection configuration
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// Username for SSH connection
    pub username: String,
    /// Hostname or IP address
    pub host: String,
    /// SSH port (default 22)
    pub port: u16,
    /// Path to private key (optional, uses ssh-agent if not provided)
    pub key_path: Option<PathBuf>,
}

impl RemoteConfig {
    /// Parse a host string like "user@host:port" or "user@host"
    pub fn parse(host_str: &str) -> Result<Self> {
        if host_str.trim().is_empty() {
            return Err(anyhow!("Host cannot be empty"));
        }

        let (user_host, port) = if host_str.contains(':') {
            let parts: Vec<&str> = host_str.rsplitn(2, ':').collect();
            let port: u16 = parts[0].parse().context("Invalid port number")?;
            (parts[1], port)
        } else {
            (host_str, 22)
        };

        let (username, host) = if user_host.contains('@') {
            let parts: Vec<&str> = user_host.splitn(2, '@').collect();
            let host = parts[1].to_string();
            if host.is_empty() {
                return Err(anyhow!("Host cannot be empty"));
            }
            (parts[0].to_string(), host)
        } else {
            // Use current user
            if user_host.is_empty() {
                return Err(anyhow!("Host cannot be empty"));
            }
            let username = std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "root".to_string());
            (username, user_host.to_string())
        };

        Ok(Self {
            username,
            host,
            port,
            key_path: None,
        })
    }

    /// Set the private key path
    pub fn with_key(mut self, key_path: PathBuf) -> Self {
        self.key_path = Some(key_path);
        self
    }

    /// Get display string for UI
    pub fn display(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }
}

/// Remote scanner that connects via SSH
pub struct RemoteScanner {
    config: RemoteConfig,
    session: Option<Session>,
}

impl RemoteScanner {
    /// Create a new remote scanner
    pub fn new(config: RemoteConfig) -> Self {
        Self {
            config,
            session: None,
        }
    }

    /// Connect to the remote host
    pub fn connect(&mut self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let tcp = TcpStream::connect_timeout(
            &addr.parse().context("Invalid address")?,
            Duration::from_secs(10),
        )
        .context(format!("Failed to connect to {}", addr))?;

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        // Try authentication methods
        if let Some(ref key_path) = self.config.key_path {
            // Use specified private key
            session
                .userauth_pubkey_file(&self.config.username, None, key_path, None)
                .context("Public key authentication failed")?;
        } else {
            // Try ssh-agent first
            if session.userauth_agent(&self.config.username).is_err() {
                // Fall back to default key locations
                let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
                let default_keys = [
                    home.join(".ssh/id_ed25519"),
                    home.join(".ssh/id_rsa"),
                    home.join(".ssh/id_ecdsa"),
                ];

                let mut authenticated = false;
                for key_path in &default_keys {
                    if key_path.exists()
                        && session
                            .userauth_pubkey_file(&self.config.username, None, key_path, None)
                            .is_ok()
                    {
                        authenticated = true;
                        break;
                    }
                }

                if !authenticated {
                    return Err(anyhow!(
                        "Authentication failed. Tried ssh-agent and default keys."
                    ));
                }
            }
        }

        if !session.authenticated() {
            return Err(anyhow!("SSH authentication failed"));
        }

        self.session = Some(session);
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Execute a command on the remote host
    fn exec(&self, command: &str) -> Result<String> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let mut channel = session
            .channel_session()
            .context("Failed to open channel")?;
        channel.exec(command).context("Failed to execute command")?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .context("Failed to read output")?;

        channel.wait_close().ok();

        Ok(output)
    }

    /// Scan ports on the remote host
    pub fn scan(&self) -> Result<Vec<PortEntry>> {
        if !self.is_connected() {
            return Err(anyhow!("Not connected to remote host"));
        }

        // Detect OS and use appropriate command
        let os_output = self.exec("uname -s")?;
        let os = os_output.trim();

        let entries = match os {
            "Linux" => self.scan_linux()?,
            "Darwin" => self.scan_macos()?,
            _ => self.scan_generic()?,
        };

        Ok(entries)
    }

    /// Scan on Linux using ss command
    fn scan_linux(&self) -> Result<Vec<PortEntry>> {
        // ss -tlnp for TCP, ss -ulnp for UDP
        let tcp_output = self.exec("ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null")?;
        let udp_output = self.exec("ss -ulnp 2>/dev/null || netstat -ulnp 2>/dev/null")?;

        let mut entries = Vec::new();

        // Parse TCP
        for line in tcp_output.lines().skip(1) {
            if let Some(entry) = self.parse_ss_line(line, Protocol::Tcp) {
                entries.push(entry);
            }
        }

        // Parse UDP
        for line in udp_output.lines().skip(1) {
            if let Some(entry) = self.parse_ss_line(line, Protocol::Udp) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Parse a line from ss output
    fn parse_ss_line(&self, line: &str, protocol: Protocol) -> Option<PortEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }

        // Extract local address (format: *:port or 0.0.0.0:port or [::]:port)
        let local_addr = parts.get(4)?;
        let port = self.extract_port(local_addr)?;

        // Extract PID and process name from the last column
        // Format: users:(("process",pid=1234,fd=5))
        let (pid, process_name) = if let Some(users) = parts.last() {
            self.parse_ss_users(users)
        } else {
            (0, "unknown".to_string())
        };

        Some(PortEntry {
            port,
            protocol,
            pid,
            process_name,
            cpu_usage: 0.0, // Can't get CPU remotely easily
            memory_usage: 0,
            memory_display: "-".to_string(),
            has_parent: true,
            is_zombie: false,
        })
    }

    /// Parse the users field from ss output
    fn parse_ss_users(&self, users: &str) -> (u32, String) {
        // Format: users:(("process",pid=1234,fd=5))
        if let Some(start) = users.find("((\"") {
            let rest = &users[start + 3..];
            if let Some(end) = rest.find('"') {
                let process_name = rest[..end].to_string();

                // Extract PID
                if let Some(pid_start) = rest.find("pid=") {
                    let pid_rest = &rest[pid_start + 4..];
                    if let Some(pid_end) = pid_rest.find(|c: char| !c.is_ascii_digit()) {
                        if let Ok(pid) = pid_rest[..pid_end].parse() {
                            return (pid, process_name);
                        }
                    }
                }

                return (0, process_name);
            }
        }

        (0, "unknown".to_string())
    }

    /// Scan on macOS using netstat and lsof
    fn scan_macos(&self) -> Result<Vec<PortEntry>> {
        let output = self.exec("lsof -iTCP -sTCP:LISTEN -P -n 2>/dev/null")?;
        let mut entries = Vec::new();

        for line in output.lines().skip(1) {
            if let Some(entry) = self.parse_lsof_line(line, Protocol::Tcp) {
                entries.push(entry);
            }
        }

        // UDP
        let udp_output = self.exec("lsof -iUDP -P -n 2>/dev/null")?;
        for line in udp_output.lines().skip(1) {
            if let Some(entry) = self.parse_lsof_line(line, Protocol::Udp) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Parse a line from lsof output
    fn parse_lsof_line(&self, line: &str, protocol: Protocol) -> Option<PortEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            return None;
        }

        let process_name = parts[0].to_string();
        let pid: u32 = parts[1].parse().ok()?;
        let addr = parts[8];

        let port = self.extract_port(addr)?;

        Some(PortEntry {
            port,
            protocol,
            pid,
            process_name,
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_display: "-".to_string(),
            has_parent: true,
            is_zombie: false,
        })
    }

    /// Generic scan using netstat
    fn scan_generic(&self) -> Result<Vec<PortEntry>> {
        let output = self.exec("netstat -tlnp 2>/dev/null || netstat -an 2>/dev/null")?;
        let mut entries = Vec::new();

        for line in output.lines() {
            if line.contains("LISTEN") || line.contains("tcp") {
                if let Some(entry) = self.parse_netstat_line(line) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Parse a generic netstat line
    fn parse_netstat_line(&self, line: &str) -> Option<PortEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        // Try to find local address column (usually 4th)
        for part in &parts {
            if let Some(port) = self.extract_port(part) {
                return Some(PortEntry {
                    port,
                    protocol: Protocol::Tcp,
                    pid: 0,
                    process_name: "unknown".to_string(),
                    cpu_usage: 0.0,
                    memory_usage: 0,
                    memory_display: "-".to_string(),
                    has_parent: true,
                    is_zombie: false,
                });
            }
        }

        None
    }

    /// Extract port number from address string like "*:8080" or "0.0.0.0:8080"
    fn extract_port(&self, addr: &str) -> Option<u16> {
        // Handle IPv6 format [::]:port
        if addr.contains("]:") {
            let parts: Vec<&str> = addr.rsplitn(2, "]:").collect();
            return parts.first()?.parse().ok();
        }

        // Handle IPv4 format or *:port
        if let Some(pos) = addr.rfind(':') {
            return addr[pos + 1..].parse().ok();
        }

        None
    }

    /// Kill a process on the remote host
    pub fn kill_process(&self, pid: u32) -> Result<()> {
        if !self.is_connected() {
            return Err(anyhow!("Not connected to remote host"));
        }

        // Try SIGTERM first, then SIGKILL
        let result = self.exec(&format!("kill {} 2>&1 || kill -9 {} 2>&1", pid, pid))?;

        if result.contains("No such process") {
            return Err(anyhow!("Process {} not found", pid));
        }

        if result.contains("Operation not permitted") || result.contains("Permission denied") {
            return Err(anyhow!(
                "Permission denied. Try running with sudo on remote host."
            ));
        }

        Ok(())
    }

    /// Force kill a process on the remote host (SIGKILL)
    pub fn kill_process_force(&self, pid: u32) -> Result<()> {
        if !self.is_connected() {
            return Err(anyhow!("Not connected to remote host"));
        }

        // Use SIGKILL directly
        let result = self.exec(&format!("kill -9 {} 2>&1", pid))?;

        if result.contains("No such process") {
            return Err(anyhow!("Process {} not found", pid));
        }

        if result.contains("Operation not permitted") || result.contains("Permission denied") {
            return Err(anyhow!(
                "Permission denied. Try running with sudo on remote host."
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(config.port, 22);
    }

    #[test]
    fn test_remote_config_parse_host_only() {
        let config = RemoteConfig::parse("example.com").unwrap();
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22);
    }

    #[test]
    fn test_remote_config_display() {
        let config = RemoteConfig::parse("user@example.com:2222").unwrap();
        assert_eq!(config.display(), "user@example.com:2222");
    }

    #[test]
    fn test_extract_port_ipv4() {
        let scanner = RemoteScanner::new(RemoteConfig::parse("test@localhost").unwrap());
        assert_eq!(scanner.extract_port("0.0.0.0:8080"), Some(8080));
        assert_eq!(scanner.extract_port("127.0.0.1:3000"), Some(3000));
        assert_eq!(scanner.extract_port("*:22"), Some(22));
    }

    #[test]
    fn test_extract_port_ipv6() {
        let scanner = RemoteScanner::new(RemoteConfig::parse("test@localhost").unwrap());
        assert_eq!(scanner.extract_port("[::]:8080"), Some(8080));
        assert_eq!(scanner.extract_port("[::1]:3000"), Some(3000));
    }

    #[test]
    fn test_parse_ss_users() {
        let scanner = RemoteScanner::new(RemoteConfig::parse("test@localhost").unwrap());
        let (pid, name) = scanner.parse_ss_users("users:((\"node\",pid=1234,fd=5))");
        assert_eq!(pid, 1234);
        assert_eq!(name, "node");
    }
}
