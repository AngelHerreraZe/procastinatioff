## What changes and why

## Related issue
<!-- e.g. #42 -->

## Security checklist (required)
- [ ] Does not store or send images, audio, OCR text or transcripts.
- [ ] Adds no personal data to logs.
- [ ] Opens no connections to servers (only to paired devices).
- [ ] If it stores or sends new data: explained here (what data, where it lives, for how long).
- [ ] If it adds a dependency: justification here + allowed license + `cargo vet` / review.
- [ ] User-facing text uses translation keys (English source + existing translations); no hardcoded strings.
- [ ] Includes tests that would fail if the logic broke.
- [ ] `just check` passes locally.
