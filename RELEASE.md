# Releasing Focal

Releases are prepared on the Mac that holds the Developer ID certificate and the Sparkle key, and published to this repository's GitHub Releases by hand. Nothing in CI signs or publishes.

## Prerequisites

- The pinned Rust toolchain (`mise install`) and Xcode or the Command Line Tools (`codesign`, `notarytool`, `stapler`).
- The certificate "Developer ID Application: Niklas Heer (WL5ASJ82YX)" in the login Keychain.
- The `notarytool` Keychain profile `keywink-notary`, shared with Keywink and Spokn (`xcrun notarytool store-credentials keywink-notary` creates it).
- The Sparkle EdDSA key in the login Keychain under the account `ed25519`, shared with Keywink and Spokn. Its public key, `SL638YLPjJQuUkrCa0p26d1nfMoa69+ymt0mzNQDscA=`, is built into every release. Keep an exported copy (`generate_keys -x <file>`) in a password manager, never in a repository: without the key, installed copies cannot accept future updates.

`scripts/sparkle` downloads Sparkle 2.10.0 once into `build/sparkle`, checked against a pinned checksum; its `bin/` holds `generate_appcast` and `sign_update`.

## Steps

1. Start from a clean checkout of `main`. Set the version in `crates/focal/Cargo.toml` if it changes, run `mise run check`, and commit. Pick a build number higher than the last release's (the second part of the cask's `version`).

2. Prepare a signed dry run, which never leaves the Mac:

   ```sh
   FOCAL_BUILD=2 scripts/release
   ```

3. Prepare the real artifacts. This submits the app to Apple's notary service, staples the ticket and writes the release files:

   ```sh
   rm -rf build/release
   FOCAL_BUILD=2 scripts/release --notarize
   ```

   ```text
   build/release/Focal-<version>-<build>.zip
   build/release/Focal-<version>-<build>.zip.sha256
   build/release/appcast.xml
   ```

   The appcast lists this release only, signed with the Sparkle key. Installed copies read `https://github.com/niklas-heer/focal/releases/latest/download/appcast.xml`, which GitHub redirects to the newest release's asset, so every older version updates straight to the newest.

4. Tag, push and publish with all three files. Without `appcast.xml`, installed copies stop seeing updates:

   ```sh
   git tag -a v0.1.0 -m "Focal 0.1.0"
   git push origin v0.1.0
   gh release create v0.1.0 build/release/Focal-0.1.0-2.zip build/release/Focal-0.1.0-2.zip.sha256 build/release/appcast.xml --title "Focal 0.1.0" --notes-file <notes>
   ```

5. Update the Homebrew cask: copy `packaging/focal.rb` to `Casks/focal.rb` in `niklas-heer/homebrew-tap` with the new `version` and `sha256`, and open a pull request (the tap's `main` accepts no direct pushes).

6. Verify: download the ZIP, unzip it, run `spctl --assess --type execute Focal.app`, and from an older installed build choose **Focal › Check for Updates…**.

## Local builds

`mise run bundle` and `mise run install-app` build an ad hoc signed `Focal.app` without an update feed; its menu shows **Focal Releases…** instead of **Check for Updates…**. `FOCAL_LINK_DIR=/tmp/bin` makes **Install Command Line Tool…** link there instead of `/usr/local/bin`.
