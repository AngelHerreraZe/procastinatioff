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
- Renamed to ProcastinatiOFF, internal ids included (`dev.procastinatioff.*`, signing domains,
  sync ALPN, key aliases). Not compatible with earlier builds: install again and pair again.
- Android: the camera verifier (still a separate app without Internet access) now comes inside the
  app, which installs it as the first setup step, before blocking can get in the installer's way;
  the screen explains why there are two installs (new permission `REQUEST_INSTALL_PACKAGES`, used
  only for that). Windows: the installer now includes the verifier.
- Goal check suggestions run on the device: the phone's own model when it has one (Gemini Nano
  through AICore on Android, Apple's on-device model on iOS 26) or the core's keyword catalog. The
  goal's name never leaves the device, and the answer can only be one of the offered checks.
- New Android permissions: `health.READ_EXERCISE` (only today's total workout minutes, for workout
  goals) and AICore's `BIND_SERVICE` (added by ML Kit GenAI). iOS: HealthKit, read only.
- Place goals keep only the hash of their QR, so no device can show it again; a new QR counts as
  relaxing (next wake-up), and a day plan cannot swap it.

### Added
- Linux agent (systemd service), packaging for Debian/Ubuntu.
- Trusted contact mode, encrypted backup, local network discovery (mDNS with names that change
  every day).
- Goals for anyone, not only reading or eating well: place goals (scan a QR stuck at the gym, the
  library or the office), workout goals (Health Connect / Apple Health minutes) and goals the
  guardian confirms from their phone. How a goal is checked is suggested from its name.

### Changed
- Android build: AGP 8.9.3, Gradle 8.14, Kotlin 2.3.21, compileSdk 36 (targetSdk stays 35), needed
  by Health Connect 1.1 and ML Kit GenAI.

### Removed
- `relay/`, `push-gateway/`, Firebase and the iOS notification service extension.
