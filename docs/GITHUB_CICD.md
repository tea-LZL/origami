## GitHub Actions CI

The [workflow](.github/workflows/ci.yml) runs Rust and UI checks, then builds
packages. Pull requests, `master` pushes, and manual dispatch runs produce CI
results and downloadable job artifacts.

Linux jobs run in an Arch Linux container. They verify the workspace, then
`package_arch` builds an Arch package with `makepkg`. `package_windows` builds
an unsigned NSIS installer (`*-setup.exe`) on GitHub-hosted Windows runners.

To reproduce the Arch package locally:

```sh
cd packaging/arch
makepkg --syncdeps --cleanbuild --force
```

The package recipe runs `npm ci`, builds `ui/`, builds the locked Rust release
binary, and installs the binary, desktop entry, and hicolor icons into the
package archive.

Version tags publish four assets as a GitHub Release: the Arch package and its
checksum, and the Windows NSIS installer and its checksum.

```sh
git push origin master
git tag -a v0.1.0 -m "Origami 0.1.0"
git push origin v0.1.0
```

Tag version must match `Cargo.toml`,
`crates/origami-app/tauri.conf.json`, `ui/package.json`, and
`packaging/arch/PKGBUILD`. Update all four before creating a release tag.

To run CI manually, open **Actions > CI > Run workflow** in GitHub and select
branch or tag. Branch runs do not publish packages; only `vX.Y.Z` tags run the
publish job.

## Windows installer

On `master` and `vX.Y.Z` tags, `package_windows` builds an unsigned NSIS
installer (`*-setup.exe`) on GitHub-hosted Windows runners:

```sh
npm ci --prefix ui
npm exec --prefix ui -- tauri build --ci \
  --config crates/origami-app/tauri.conf.json \
  --bundles nsis -- --locked --features custom-protocol
```

That artifact is **not a supported Windows release** — unsigned, so Windows
SmartScreen warns on first run. The job allows failure on branch pushes
(experimental) but must succeed for a version tag before the release publishes.
Windows tests run in the Linux `rust` job; the Windows job only packages.
