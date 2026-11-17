# Releasing

1. `CHANGELOG.md` has a section `## [X.Y.Z]` with **Privacy and security** (even if "no changes").
   If what is stored or sent changed, `PRIVACY.md` changed in the same pull request.
2. CI is green on `main` (privacy, secrets, SAST, supply chain, desktop, Android).
3. Tag `vX.Y.Z`. The `release` workflow builds the artifacts unsigned, checks that the Linux agent
   is reproducible, attaches the SBOM and `SHA256SUMS`, and creates a **draft** release.
4. **Signing ceremony, two people.** One signs with the hardware keys (Windows code signing,
   Android, Linux repository); the other recomputes the hashes of the draft artifacts, checks
   them against `SHA256SUMS`, and checks the signatures. No signing key is ever in CI.
   Android: the app carries the camera verifier inside (`assets/verifier.apk`) and installs it, so
   sign the verifier **first**, put it in the app, then sign the app (same key for both):
   ```sh
   apksigner sign --ks <key> --out verifier.apk procastinatioff-verifier-unsigned.apk
   mkdir -p assets && cp verifier.apk assets/verifier.apk
   cp procastinatioff-unsigned.apk app.apk && zip -0 app.apk assets/verifier.apk   # replaces the entry
   zipalign -p -f 4 app.apk app-aligned.apk
   apksigner sign --ks <key> --out procastinatioff.apk app-aligned.apk
   ```
5. Publish the draft. Staged rollout: Google Play 5 % → 20 % → 100 % in 72 h; App Store phased
   release; desktop beta channel 48 h before stable.
6. Keep the previous version ready to publish again (rollback plan).
