# Security Policy

## Security model

Auryn is a local-first, read-only session browser. Its security posture is
documented in `docs/adr/0007-security-model.md`. In summary, Auryn:

* never stores, reads, or transmits credentials or API keys
* never modifies provider configuration or session files
* never transmits or proxies session data
* treats every session file as untrusted input: it bounds file sizes, validates
  parsed data, and skips malformed content rather than trusting it
* never executes commands found inside session metadata
* spawns provider commands argument by argument, never through a shell

Auryn reads provider session storage only. It does not write to any
provider-owned path.

## Reporting a vulnerability

If you discover a security issue, please report it privately rather than opening
a public issue. Use GitHub's private vulnerability reporting for this repository
(Security tab, Report a vulnerability), or contact the maintainer directly.

Please include:

* a description of the issue and its impact
* steps to reproduce, or a proof of concept
* affected version or commit

You can expect an acknowledgement within a few days. Please give a reasonable
amount of time for a fix before any public disclosure.

## Supported versions

Auryn is pre-1.0. Security fixes are applied to the latest release.
