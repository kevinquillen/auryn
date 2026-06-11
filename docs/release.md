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

1. Update the version in `Cargo.toml`.
2. Move the relevant `CHANGELOG.md` entries from Unreleased into a new version
   section.
3. Commit the changes.
4. Tag and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds every
target, packages the archives with checksums, and attaches them to a GitHub
Release for the tag.

## Continuous integration

`.github/workflows/ci.yml` runs on pushes and pull requests: formatting check,
clippy, and the test suite on Linux, macOS, and Windows.

## cargo-dist

The repository carries cargo-dist configuration in `Cargo.toml` under
`[workspace.metadata.dist]`, declaring the targets and installers (shell,
PowerShell, and Homebrew). The committed `release.yml` performs the equivalent
build-and-publish without requiring cargo-dist to be installed.

To adopt the fully cargo-dist-managed flow instead, install cargo-dist and run:

```bash
dist init
dist generate
```

This pins `cargo-dist-version`, regenerates the release workflow from the
committed config, and can produce the Homebrew formula. Choose either the
hand-maintained workflow or the cargo-dist-generated one as the single source of
truth, rather than maintaining both.

## Homebrew

The Homebrew formula installs the macOS and Linux archives from the GitHub
Release. It lives in a tap repository (for example
`kevinquillen/homebrew-tap`). A template is in `packaging/homebrew/auryn.rb`;
the URLs and SHA-256 values are updated for each release (cargo-dist can
automate this).

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
