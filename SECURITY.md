# Security policy

If you find a vulnerability, **do not open a public issue**. Report it privately with
**"Report a vulnerability"** in this repository's *Security* tab (GitHub Private Vulnerability
Reporting).

- We acknowledge receipt within 72 hours.
- We keep you informed at least every 7 days.
- We publish the fix and credit you (if you want) within 90 days at most.

**In scope:** the apps (Android, iOS, desktop), the privileged service and its IPC, the offline
verifiers, sync between devices (QR pairing, peer-to-peer protocol, local network discovery),
the event format and cryptography, the methods that authorize changes (password, pattern,
biometrics, guardian and trusted-contact signatures, offline codes) and the DNS filter.

**Out of scope:** attacks that require physically unlocking the device with the user's own code;
social engineering; attacks on devices or accounts that are not yours.

## Severity

| Severity | Examples |
|---|---|
| Critical | Reading another user's events; getting camera images; running code on another device through peer sync; relaxing the rules without a guardian's or trusted contact's signature |
| High | Forging a guardian approval; disabling blocking without leaving a trace; freezing a computer through the desktop service |
| Medium | Getting around the web filter with an undocumented technique |
| Low | Undocumented metadata leaks |

Rewards: a paid bounty program is planned. Until it opens, every valid report is credited in the
release notes (if you want).

ProcastinatiOFF has no servers: the project holds no user data.
