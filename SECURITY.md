# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in keel, please report it responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Email: **security@keel.engineer** (or open a private security advisory on GitHub)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### What to Expect

- **Acknowledgment** within 48 hours
- **Assessment** within 7 days
- **Fix timeline** communicated after assessment
- **Credit** in the release notes (unless you prefer anonymity)

### Scope

The following are in scope:

- Code execution vulnerabilities in keel CLI
- Path traversal in file walker or graph storage
- SQL injection in SQLite graph store
- Denial of service in MCP/HTTP server
- Information disclosure through error messages

The following are out of scope:

- Vulnerabilities in upstream dependencies (report to the upstream project)
- Issues requiring physical access to the machine
- Social engineering attacks

## Security Design

keel is designed with security in mind:

- **No network access** by default — only `keel serve` opens ports
- **Read-only by default** — keel reads your code but only writes to `.keel/`
- **No code execution** — keel parses code structurally, never evaluates it
- **Sandboxed subprocess calls** — `ty` subprocess runs with minimal permissions
