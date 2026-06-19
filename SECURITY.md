# Security Policy

ExoQuill is local-first and stores user data on the device. We take both software
vulnerabilities and privacy regressions seriously.

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

- Preferred: use GitHub's **private vulnerability reporting** (Security → Report a vulnerability)
  on this repository.
- Alternatively, contact the maintainer privately. _(Add a security contact email here before the
  first public release.)_

Please include: affected version/commit, reproduction steps, impact, and any suggested fix.
We aim to acknowledge reports within a few days.

## Scope

In scope:

- Code execution, privilege escalation, or sandbox escape in the desktop app.
- Privacy regressions: unexpected network calls, telemetry, or raw-audio/data retention that
  contradicts the documented privacy defaults.
- Bundled-model or provider supply-chain integrity issues.

Out of scope:

- Vulnerabilities in third-party model runtimes that we only bundle — please report those
  upstream, but do tell us so we can update or mitigate.

## Supported versions

During pre-alpha (pre-v0.1), only the latest `main` is supported.
