# ProcastinaciOFF

**Blocks the mindless use of your devices until you complete the goals you set for yourself each morning.** No accounts, no servers, and nothing is ever recorded: everything happens on your devices.

> **Status: in development, not ready for daily use yet.** The core and the Android and Windows apps build and have tests, but they have not been tested on real devices yet. Sync is being moved to **local network / peer-to-peer**: the current code still includes `relay/` and `push-gateway/` from the previous architecture, which will be removed.

**Languages:** the default language is English. Spanish is included. Translations are local files bundled with the app, and anyone can add theirs: see [Translations](#translations).

---

## What it does

1. **In the morning** you open the *day plan*: your fixed goals are already there and you add today's. From then on the plan can only get stricter.
2. **Until you complete your "key" goals**, the phone and the PC only allow the essentials (calls, messages and whatever you add: authenticator, maps, a medical app…) plus a few short **passes**.
3. **You complete a goal** (read, exercise, study…) and the app checks it **on the device**. Once all key goals are done, everything is free until bedtime.
4. **At night**, sleep mode: calls only.

| State | What is allowed |
|---|---|
| Sleep | Calls only (and emergency calls, always) |
| Locked | Essentials + the day's passes |
| Pass | Everything for a few minutes (the counter is shared between devices) |
| Focus | Only the apps and sites of the current goal (e.g. Word, Coursera and the school portal for "Study") |
| Free | Everything, until bedtime |
| Emergency | Everything for 15 min, always available, with a cost and a record |

**Anti-impulse:** relaxing a rule (more passes, removing a goal, sleeping later) takes effect **tomorrow**; tightening it, immediately.

## Principles (non-negotiable)

- **Nothing is recorded.** Images, audio, OCR text and transcripts only exist in RAM for a few seconds. Results are stored ("goal done", minutes), never the evidence.
- **The component that uses the camera has no Internet access**, enforced by the operating system: a separate APK without the `INTERNET` permission on Android, an AppContainer without network on Windows.
- **No servers and no sign-up.** There is no backend: your data lives on your devices (and on your family's or your trusted contact's, if you decide so). Your identity is a key pair generated on the device.
- **Zero telemetry, ads or third-party SDKs.**
- **No surveillance:** no keylogger, screenshots, message reading, location or browsing history. Not even a guardian can see them, because they do not exist.
- **Open source** (MPL-2.0) with reproducible builds.
- **Full erasure with one button** (the keys are destroyed and everything encrypted becomes unreadable).

## Sync without servers

Devices are paired by **scanning a QR code** and comparing a 6-digit code (protects against man-in-the-middle). After that:

| Channel | When | How |
|---|---|---|
| **Local network** (default) | When they are on the same Wi-Fi, e.g. at home | They find each other via mDNS with an identifier that changes every day, and connect directly over QUIC. Only paired devices are accepted. |
| **Peer-to-peer over the Internet** (optional, off by default) | Away from home | Direct connection through the NAT; if that fails, encrypted relaying through a public or self-hosted relay that sees nothing. |
| **Offline codes** | Urgent approvals without connection | The requester reads out a 10-character code; the approver answers with an 8-digit one. |

Every change is a **signed** (Ed25519) and **encrypted** (`age`) event in an append-only log; every device computes the same state from it. The PC (with an always-on service) is the meeting point: when the phone gets home, they exchange whatever is missing. Each device enforces its own local copy, even offline. **Notifications are local** (alarms, reminders, requests received when syncing): no APNs or FCM, since both require a server. Since there is no cloud, you can **export an encrypted backup** protected by a recovery phrase.

## Who can change the rules

Changing settings, routines, schedules or lists requires **one of these 5 methods**:

| # | Method | Notes |
|---|---|---|
| 1 | **Password**: numeric PIN (≥ 6 digits) or alphanumeric (≥ 8), your choice | You can set it yourself or give it to someone else to keep. Stored as an Argon2id hash; failed attempts add growing waits. |
| 2 | **Biometrics** (fingerprint or face) | Checked by the operating system; the app never sees your fingerprint. If someone enrolls a new fingerprint, it must be enabled again. |
| 3 | **Parents or guardians** | For children's and teenagers' profiles. They approve from their phone or, in person, with their own PIN/pattern. |
| 4 | **Trusted contact** | For adults who choose it (see below). |
| 5 | **Pattern** | 3×3 grid, at least 4 points (6 recommended). |

Tightening never needs authorization, and neither does the emergency button.

**Trusted contact (adults, optional).** You choose someone (up to 3) who **has to approve** whatever relaxes your rules. They install the app in *contact mode* (nothing gets blocked for them) and you pair with a QR. They only see the change you ask for and, if you want, emergency or tampering alerts. Never your history, goals or any evidence. If you want to remove them and they do not approve, the change takes effect after 7 days: nobody gets trapped, and nobody removes them on an impulse.

**Family mode (children and teenagers).** A minor's profile **cannot exist without a guardian** (created in person, with the guardian's phone QR). Everything that relaxes rules carries the **guardian's signature**; the minor can ask and can always tighten. **Mirror panel:** the minor sees exactly what the guardian sees, and the guardian never sees messages, location, images or browsing history. The web is limited to an allowlist of study sites with layered filtering; the camera is **off** by default for minors.

## Goal verification (100 % local)

Every goal has at least one **camera-free** option: a focus session on the PC, QR anchors stuck around the house, Health Connect / HealthKit, a trusted witness, or a declaration with friction. If you choose the camera or microphone: rep counting by pose, reading presence, page numbers by OCR, a live food photo, or a summary of what you read scored by a local LLM. Models ship inside the app and are verified by hash.

## Platforms

| Platform | Blocking | Status |
|---|---|---|
| Android | Accessibility + shield; Device Owner for minors; **local** VPN for the DNS filter | In development |
| Windows | `LocalSystem` service + session agent + browser policies + local DNS | In development |
| iOS | Screen Time API (FamilyControls, ManagedSettings, DeviceActivity) | Started (needs an Apple entitlement) |
| macOS | Endpoint Security + NetworkExtension | Planned |
| Linux | systemd + fanotify + nftables | Planned |

**Honest limit:** on a device you fully administer, no software is unbeatable. The goal is to make skipping the block **slow, costly and visible**, not impossible.

## Repository layout

| Folder | Contents |
|---|---|
| `core/` | Rust core: day rules, signed events, crypto, SQLCipher storage, sync, DNS filter |
| `proto/` | Shared event and message formats |
| `verify/` | Verification logic (pose, reading, summary, food, pages). No network or disk, checked in CI |
| `netfilter/` | Local DNS server for a minor's PC (the core decides the policy) |
| `desktop/` | Windows service (`agent/`), IPC (`ipc/`), Tauri + Svelte UI (`app/`), AppContainer verifier (`sandbox/`, `verifier/`) |
| `android/` | Kotlin + Jetpack Compose app (`app/`) and the verifier without Internet permission (`verifier/`) |
| `apple/` | Core for iOS/macOS (`ffi/`) and the iOS app with its Screen Time extensions |
| `lists/`, `scripts/` | Category list sources; privacy and translation checks, build utilities |
| `relay/`, `push-gateway/` | Previous architecture, **being removed** |

## Building

Requirements: Rust 1.100 (pinned by `rust-toolchain.toml`), [`just`](https://github.com/casey/just), Python 3, Node 22 (desktop UI), JDK 17 + Android SDK 35 (Android). On Windows, SQLCipher needs [Strawberry Perl](https://strawberryperl.com/) to build OpenSSL.

```bash
just check      # format, clippy, tests, cargo-deny, cargo-audit, privacy and translation checks
just app        # desktop UI (Svelte + Tauri)
just bindings   # Kotlin bindings for the core
just fuzz <target> 60   # fuzzing (needs nightly)
```

> **Warning: do not install the Windows service on your work machine:** it closes apps. Use a virtual machine with a standard account.

---

## Contributing

Contributions are welcome. For anything big, open an issue first so we can discuss it. Security-sensitive paths (crypto, sync, authorization, the verifier, the privileged service) need two reviews (see `.github/CODEOWNERS`).

### Best practices

1. **English everywhere**: identifiers, comments, documentation, commit messages and UI *source* strings. Other languages live only in the translation files.
2. **Never hardcode user-facing text.** Every string the user can see is a translation key whose source text is in English (see [Translations](#translations)). This includes notifications, accessibility labels and error messages.
3. **The core decides, the platforms display.** Rules live in the Rust core as pure functions with the time *injected* (never call `now()` inside logic). UIs show state and ask for actions.
4. **The core never returns sentences.** It returns stable codes (`denied.no_permits_left`, `error.network`…) and each UI translates them. Never rename a code: translations depend on it.
5. **Privacy rules are not negotiable**: never write camera frames, audio, OCR text or transcripts to disk; no personal data in logs (goal titles, device names, domains, keys); no connections except to paired devices; no analytics, ads or crash-reporting SDKs. CI enforces part of this (`scripts/privacy_scan.py`).
6. **Do not invent cryptography.** Only the reviewed primitives already in use (`age`, Ed25519, SQLCipher, the platform keystores). New primitives need a written justification and two reviews.
7. **Keep it simple.** The smallest change that works; no speculative abstractions, no configuration for values that never change. Prefer the standard library and native platform features over new dependencies. Every new dependency needs a justification and a compatible license (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0; no GPL/AGPL/LGPL).
8. **Every non-trivial logic leaves a test** that would fail if it broke. `just check` must pass before you open a PR.
9. **Accessibility is part of "done"**: labels for screen readers, numbers not conveyed by color alone, touch targets ≥ 48 dp / 32 px.
10. **Commits and PRs**: small and focused, in English, imperative mood with a scope (`core: add pass cooldown`, `android: translate shield screen`). Explain *why* in the PR description and fill in the security checklist.

### Code comments

- **Write them in English.** Explain *why* (intent, invariants, trade-offs, security assumptions), not *what* the next line does.
- **Document public items**: `///` and `//!` in Rust, KDoc `/** */` in Kotlin, TSDoc `/** */` in TypeScript/Svelte, `///` in Swift.
- **Markers we use** (plain text, never emojis; no emojis anywhere in the repository):
  - `SECURITY:` security- or privacy-critical code; changes need two reviews. At the end of a sentence: `(security-critical)`.
  - `VERIFY:` something to verify against the current platform documentation (APIs that change between versions).
  - `LEGAL:` needs legal review.
  - `ponytail:` a deliberate simplification, with its limit and when to improve it (e.g. `// ponytail: O(n) per call; cache per day if it shows up in profiles`).
- **No commented-out code** and no bare `TODO`: open an issue and reference it.
- **Never put personal data, secrets or real user examples in comments** or test data.
- **Keep comments in sync**: when you change the code, update or delete the comment. A wrong comment is worse than none.
- Source files start with the MPL-2.0 header used in the rest of the repository.

---

## Translations

ProcastinaciOFF is meant to be global: **developers add translations to their own languages**. Translations are **local files bundled with each app**: they are never downloaded, and no translation service is used at runtime. Dates, times, weekdays and durations come from the operating system (`Intl`, ICU, Foundation), so they need no translation.

| Where | Source (English) | Your language |
|---|---|---|
| Desktop UI (Svelte) | `desktop/app/ui/src/locales/en.json` | `desktop/app/ui/src/locales/<lang>.json` (e.g. `de.json`, `pt-BR.json`) |
| Android app | `android/app/src/main/res/values/strings.xml` | `android/app/src/main/res/values-<lang>/strings.xml` (e.g. `values-de`, `values-pt-rBR`) |
| Android verifier | `android/verifier/src/main/res/values/strings.xml` | `android/verifier/src/main/res/values-<lang>/strings.xml` |
| iOS / macOS | `apple/ios/Shared/Localizable.xcstrings` | Same file: add your language (Xcode or JSON) |
| Windows installer | `desktop/app/src-tauri/tauri.bundle.json` | Add your NSIS language to `languages` |
| Summary scoring | `verify/src/summary.rs` | Add your language's stop words to `STOP_LISTS` |

**How to add a language**

1. Copy the English file to your language's name (or add a language to the String Catalog).
2. Translate **only the text**. Keep the keys/names and the placeholders exactly: `{n}` / `{goal}` on desktop, `%1$s` / `%1$d` on Android, `%@` / `%lld` on Apple. Do not translate entries marked `translatable="false"`.
3. Plurals: desktop uses `key.one`, `key.few`, `key.many`, `key.other`…, and Android uses `<plurals>` items. Use the [CLDR plural categories](https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html) of your language. A plural form may omit the number (`"one goal left"`).
4. Missing keys fall back to English, so partial translations are fine.
5. Run `just i18n` (or `python scripts/check_i18n.py`). It fails on stale keys, placeholder mismatches and keys used in code but missing in English.
6. Open a PR titled `i18n: add <language>`. Ideally ask a second native speaker to review it.

**Style for translators**

- Be kind and clear: the app explains and never blames ("You haven't met today's goals yet", not "You failed").
- Notification texts appear on the lock screen: keep them generic (no goal names or details).
- Do not translate the product name **ProcastinaciOFF**.
- Use these terms consistently: *pass* (a few free minutes), *focus*, *locked*, *free*, *sleep*, *goal*, *key goal* ("unlocks"), *habit*, *guardian*, *trusted contact*, *pairing*, *emergency*.

**For developers adding text:** add the key to the English file *and* to every existing translation you can write (at least Spanish). Never concatenate translated fragments: use a full sentence with placeholders, because word order changes between languages.

---

## Security

If you find a vulnerability, **do not open a public issue**: follow [SECURITY.md](SECURITY.md). CI fails if code records camera frames to disk, if the verifier links a network library, if a dependency has a disallowed license or a known vulnerability, or if the translation files are inconsistent.

## License

[Mozilla Public License 2.0](LICENSE).
