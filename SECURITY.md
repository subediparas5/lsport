# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in Port-Patrol, please report it responsibly:

1. **Do NOT** open a public issue
2. Email the maintainers directly or use GitHub's private vulnerability reporting
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Security Considerations

### SSH Connections

- Port-Patrol uses the `ssh2` crate for remote connections
- Credentials are never stored - authentication uses SSH agent or key files
- Host key verification follows system SSH configuration

### Process Termination

- Killing processes requires appropriate system permissions
- The tool does not elevate privileges automatically
- Users should run with `sudo` only when necessary

### Local Scanning

- Port scanning only reads system information
- No network packets are sent for local scanning
- Process information is read via the `sysinfo` crate

## Best Practices

1. Only connect to trusted remote hosts
2. Use SSH keys instead of passwords
3. Run without `sudo` when possible
4. Keep the tool updated to receive security fixes

