# Release signing and platform trust

StructTrace does not treat an unsigned archive as a trusted desktop release. The release workflow
builds Linux, Intel macOS, Apple Silicon macOS, and Windows archives from one tag, generates an SPDX
SBOM, attests provenance, extracts each final archive, and publishes a machine-readable evidence
receipt. Signing is a separate platform identity boundary.

## macOS

The release operator must provide an Apple Developer ID Application certificate through repository
secrets, sign the final executable with a hardened runtime and timestamp, submit the archive to the
Apple notary service, wait for acceptance, staple the ticket, and require both of these checks to
pass on each architecture:

```bash
codesign --verify --deep --strict structtrace
spctl --assess --type execute --verbose=2 structtrace
```

Required secret material is the Developer ID certificate and password plus App Store Connect API
issuer, key ID, and private key. These credentials must live only in the release environment. A
workflow run without them may build test artifacts but must not publish a stable macOS release.

## Windows

The release operator must sign `structtrace.exe` with an organization-validated Authenticode
certificate, use an RFC 3161 timestamp server, package only the signed executable, and verify
`Get-AuthenticodeSignature` reports `Valid` after extracting the final ZIP. The certificate and
password belong in the protected release environment. A missing or invalid signature blocks a
stable Windows release.

Windows local evidence also receives an explicit protected ACL: inherited access is removed and
the current account receives full control. The Windows test matrix asserts ACL inheritance is
disabled and process descendants remain in a kill-on-close Job Object.

## SmartScreen and Gatekeeper

Signing establishes publisher identity; it does not manufacture reputation. SmartScreen reputation
must accumulate through consistently signed releases from the same certificate. Gatekeeper trust
requires notarization for every distributed macOS archive. Documentation must never instruct users
to disable either security system as the normal installation path.

External credentials and platform reputation cannot be simulated in source control. Until signed,
notarized archives have passed the post-extraction checks, source builds and prerelease artifacts
must be labelled accordingly.
