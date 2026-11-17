# Changelog

Every release has a **Privacy and security** section, even when it says "no changes".
Versions follow SemVer; MAJOR goes up when the event format changes incompatibly. The sync
protocol has its own version (`PROTOCOL_VERSION`); apps accept the current one and the previous
one for 6 months.

## [Unreleased]

### Privacy and security
- No servers anymore: the relay and the push gateway are gone. Devices sync directly on the local
  network (QUIC, accepted only from the keys listed in the profile) and show local notifications.
- New ways to authorize changes: PIN, password or pattern (Argon2id, with a growing wait after 5
  wrong attempts), biometrics bound to a hardware key, and a trusted contact who must approve
  changes that relax your rules. A recovery code resets forgotten methods after 72 hours.
- Offline approval codes for small concessions (an extra pass, a rest day, a later bedtime
  tonight), at most 3 a day and always visible to the guardian or the trusted contact.
- Fixed: a modified app could grant itself passes or rest days in a personal profile.
- Linux agent: blocks apps with fanotify before they start; secrets encrypted with a
  systemd-creds key; it never dumps core (a crash cannot freeze the computer).
- Technical report for support that contains only versions, states and counts, and is only sent
  if you share it (Android, Windows and Linux).
- "Delete all my data" on Android, Windows and Linux: empties the database, re-encrypts it and
  replaces every key of the device (crypto-shredding). It needs the same authorization as
  relaxing a rule, and an interrupted deletion is finished the next time the app starts.
- Android: if another fingerprint is enrolled, fingerprint unlock is turned off and says so; it
  can be turned on again only after unlocking with the PIN, password or pattern.
- Sync protocol version 2.

### Added
- Linux agent (systemd service), packaging for Debian/Ubuntu.
- Trusted contact mode, encrypted backup, local network discovery (mDNS with names that change
  every day).

### Removed
- `relay/`, `push-gateway/`, Firebase and the iOS notification service extension.
