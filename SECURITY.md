# Security Policy

## Supported Versions

Riply is alpha software (`0.x.0-alpha.N`). Security fixes are applied to the latest release.

## Reporting a Vulnerability

We take security seriously. If you discover a security vulnerability, please report it responsibly:

1. **Do NOT open a public issue.**
2. Email **Skovens** at the email associated with the GitHub account `Skovens`.
3. Include a description of the vulnerability and steps to reproduce it.
4. Allow time for a fix before any public disclosure.

We aim to respond within 48 hours and provide regular updates on the fix progress.

## Security Measures

- `cargo-deny` scans for known vulnerabilities and license compliance on every CI run
- All dependencies are tracked via lockfiles (`Cargo.lock`, `package-lock.json`)
- Secrets are excluded from git via `.gitignore`
- Database files are treated as ephemeral and never committed

## What to Expect

- **Acknowledgment:** Within 48 hours
- **Fix timeline:** Varies by severity (critical fixes within 1 week)
- **Disclosure:** Coordinated with reporter after fix is deployed
