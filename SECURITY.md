# Security policy

## Supported versions

Only the latest release receives security fixes. Wisp updates itself, so please update before reporting.

## Reporting a vulnerability

Please **do not** open a public issue. Use GitHub's private reporting instead:
https://github.com/Wisp-Browser/Wisp/security/advisories/new

Include what you found, how to reproduce it, and the Wisp version (Settings → About).
You can expect a first reply within a few days.

## Things worth knowing

- API keys for the AI sidebar are stored in the Windows Credential Manager, never in Wisp's files.
- Web pages cannot talk to Wisp's internals: the bridge only accepts a small set of messages from pages,
  and AI requests from the right-click menu are protected by a per-session secret.
- Updates are downloaded over HTTPS and checked against a SHA-256 from the release before they are applied.
  If you add code signing (see README), builds are also Authenticode-signed.
- Private windows keep their data in a temporary folder that is deleted when the window closes.
