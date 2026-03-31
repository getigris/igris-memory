# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in Igris Memory, **please do not open a public issue**.

Instead, report it privately:

1. **GitHub Security Advisories** (preferred): Go to [Security > Advisories > New draft advisory](https://github.com/getigris/igris-memory/security/advisories/new) and submit a private report.
2. **Email**: Send details to the repository owner via their GitHub profile.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if you have one)

### What to expect

- **Acknowledgment** within 48 hours
- **Assessment** within 7 days
- **Fix or mitigation** as soon as feasible, prioritized by severity
- Credit in the release notes (unless you prefer to remain anonymous)

## Scope

The following are in scope for security reports:

- **SQLCipher encryption** — key handling, storage, or bypass
- **Privacy redaction** — `<private>` tag stripping failures that leak secrets
- **MCP transport** — unauthorized access or injection via stdio/HTTP
- **SQL injection** — through any user-facing input (search queries, titles, content)
- **Path traversal** — in data-dir, project-scoped paths, or sync export/import
- **Denial of service** — via crafted input that crashes or hangs the server

## Out of Scope

- Vulnerabilities in upstream dependencies (report those to the respective projects)
- Issues requiring physical access to the machine where igmem runs
- Social engineering
