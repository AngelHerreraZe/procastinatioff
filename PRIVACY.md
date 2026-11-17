# Privacy

ProcastinaciOFF has **no servers and no accounts**. Your data lives only on your devices (and, if
you decide so, on your family's or your trusted contact's devices), and travels directly between
them, end-to-end encrypted. The project cannot see, sell or hand over anything: it has nothing.

This page lists every piece of data the apps keep. If the code ever keeps something that is not
here, that is a bug: please report it (see [SECURITY.md](SECURITY.md)).

## What is never stored or sent

- **Images, audio, video, text read from pages, transcripts.** The goal verifiers use the camera
  and microphone in memory for a few seconds; only the result is kept (done or not, a score,
  minutes). The verifier has no network permission at all.
- **Email, phone number, name, location.** There is no sign-up: each device has its own keys.
- **Browsing history and messages.** The web filter decides in memory; essential apps such as
  WhatsApp are allowed, never read.
- **Telemetry, ads, analytics or crash-reporting SDKs.** None.

## What is stored, where, and for how long

| Data | Where | Protection | Kept |
|---|---|---|---|
| Configuration: fixed goals, schedules, free days, passes, web allowlist, which unlock methods you use | Devices of your profile | Encrypted database (SQLCipher); encrypted again for each device in transit (age) | While you use the app |
| Device names you choose ("Study PC") | Devices of your profile | Encrypted | While you use the app |
| Day plan, completed goals, verification results, heartbeats | Devices of your profile | Encrypted | 90 days |
| Perceptual hash of verification photos (salted, not the photo) | Only the verifier that took it | Encrypted | 7 days |
| Argon2id hashes of your PIN, password or pattern, and of your recovery code | Devices of your profile | Encrypted (and hashed) | Until you change them |
| Failed unlock attempts counter | Only the device where they happened | Encrypted | Until the next correct unlock |
| Keys for offline approval codes | Not stored: each device derives them from the profile's key when needed | — | — |
| Trusted contact channel: the changes you ask them to approve (typed lines), their answers, alerts (type, time, goals completed out of total) | Your device and your contact's | Encrypted | 90 days |
| Network addresses of your other devices | Memory only | — | Until the app closes |
| Websites blocked by the family filter | Memory only; the next day only the **number** is recorded | Encrypted | 90 days (the number) |
| Guardian keys and roles (family mode) | Devices of the family | Encrypted | While the profile exists |
| Exported backup | Wherever you save it | Encrypted with your passphrase (age + scrypt) | Your decision |
| Technical report for support | Only if you share it yourself | You read it first | Your decision |

## Who sees what

- **Your devices** see everything in your profile.
- **A guardian** sees exactly what the minor sees in the mirror panel: the state of the day,
  tampering, emergencies, the number of blocked sites and concessions. Never messages, apps used,
  location or photos.
- **A trusted contact** sees only the changes you ask them to approve, written as typed lines
  ("Passes per day: 3 → 5"), and the alerts you chose. Never your history, goals of the day or
  any evidence.

## Deleting your data

- **"Delete all my data"** (settings, on every device): empties the database, re-encrypts it and
  replaces every key of that device, so nothing stored before can be read again, not even from
  copies the disk or flash memory keeps. It relaxes your rules, so it asks for what relaxing asks
  for: your PIN, password or pattern; with a trusted contact, first stop needing their approval
  (they approve, or it takes 7 days). On a minor's device only the guardian can remove it.
- **Android and iOS:** uninstalling the app also deletes everything on that device. The Android
  camera verifier is a separate app: uninstall it too to remove its salted photo hashes (they
  expire after 7 days anyway).
- **Windows and Linux:** uninstalling keeps the data so an accidental uninstall does not lose your
  history; use "Delete all my data" before uninstalling.
- Data on your other devices stays there until you delete it on each one. They will see this
  device stop reporting.

## Checking these claims

The code is open source (MPL-2.0) and the Linux agent is built reproducibly (see
[REPRODUCING.md](REPRODUCING.md)): anyone can check that the published binaries come from this code.
