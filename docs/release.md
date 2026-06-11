# Release and packaging

Auryn is distributed as prebuilt binaries on GitHub Releases, with Homebrew as a
secondary channel for macOS and Linux. Scoop is deferred. See
`docs/adr/0006-release-distribution.md`.

## Target platforms

Each release publishes archives for:

* macOS ARM64 (`aarch64-apple-darwin`)
* macOS x86_64 (`x86_64-apple-darwin`)
* Linux x86_64 (`x86_64-unknown-linux-gnu`)
* Linux ARM64 (`aarch64-unknown-linux-gnu`)
* Windows x86_64 (`x86_64-pc-windows-msvc`)

Unix archives are `tar.gz`, Windows archives are `zip`. Each archive has a
matching `.sha256` checksum file.

## Cutting a release

The tag is the trigger, and `Cargo.toml` is the source of truth for the version.
The two must agree: the release workflow's `verify` job fails the release if the
tag does not match the `Cargo.toml` version, so a binary whose `--version`
disagrees with the release tag can never ship.

1. Bump the version. The simplest one-command path is
   [`cargo-release`](https://github.com/crate-ci/cargo-release), which updates
   `Cargo.toml`, commits, tags, and pushes in step:

   ```bash
   cargo release patch --execute   # or minor / major / 1.2.3
   ```

   Or do it by hand: edit the `Cargo.toml` version, move the relevant
   `CHANGELOG.md` entries into a new section, commit, then:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which verifies the
version, builds every target, packages the archives with checksums, attaches
them to a GitHub Release, and publishes the Homebrew formula (see below). The
Formula version and download URLs derive from the tag automatically; no manual
editing of the formula is required.

## Continuous integration

`.github/workflows/ci.yml` runs on pushes and pull requests: formatting check,
clippy, and the test suite on Linux, macOS, and Windows.

## Source of truth

The hand-maintained `.github/workflows/release.yml` is the single source of truth
for releases. It verifies the version, builds every target, publishes the GitHub
Release, and publishes the Homebrew formula, without any extra tooling.

cargo-dist is an alternative that generates an equivalent workflow and publishes
a Homebrew formula from `Cargo.toml` metadata. It is not configured in this
repository. To adopt it instead, install cargo-dist, run `dist init` (which adds
its config and generates the workflow), and remove the hand-maintained workflow
and the formula template so there is one source of truth.

## Homebrew

The Homebrew formula installs the macOS and Linux archives from the GitHub
Release, from a tap repository named `kevinquillen/homebrew-tap`. One tap repo
holds the formulas for any number of projects, in `Formula/`.

The formula is published automatically by the `publish-homebrew` job in
`release.yml`. On each release it reads the checksums the build jobs produced,
fills the version and the four SHA-256 values into `packaging/homebrew/auryn.rb`,
and commits the result to the tap. Two one-time prerequisites:

* Create the public repo `kevinquillen/homebrew-tap`.
* Add a secret named `HOMEBREW_TAP_TOKEN` to this repository: a fine-grained
  Personal Access Token scoped to only `homebrew-tap` with `Contents: write`.
  The default `GITHUB_TOKEN` cannot push to a separate repository, so this
  cross-repo token is required.

Prerelease tags (any tag containing a hyphen, such as `v0.0.0-test1`) skip the
formula publish, so they can be used to dry-run the build without touching the
tap.

```bash
brew install kevinquillen/tap/auryn
```

## Windows

Windows users download the `.zip` from the GitHub Release and put `auryn.exe` on
their `PATH`. When the cargo-dist flow is used, the PowerShell installer offers a
one-line install.

## Scoop (deferred)

Scoop support is deferred. It would require a separately maintained bucket
repository (for example `kevinquillen/scoop-bucket`), and cargo-dist does not
publish Scoop manifests, so the manifest would be updated per release. A future
template is kept in `packaging/scoop/auryn.json`. If added later, install is:

```powershell
scoop bucket add kevinquillen https://github.com/kevinquillen/scoop-bucket
scoop install auryn
```

WinGet is an alternative worth considering instead of Scoop, since it ships with
Windows and reaches all users.
