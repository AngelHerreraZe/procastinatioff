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
the code in `relay/` and `push-gateway/`, which is being removed and is not part of any release.

ProcastinaciOFF has no servers: the project holds no user data.
