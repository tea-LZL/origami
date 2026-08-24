## GitLab CI/CD

The [GitLab pipeline](.gitlab-ci.yml) runs Rust and UI checks, then builds an
Arch package with `makepkg` in an Arch Linux container. Merge requests and
branch pushes produce CI results and downloadable job artifacts.

To reproduce the package build locally on Arch:

```sh
cd packaging/arch
makepkg --syncdeps --cleanbuild --force
```

The package recipe runs `npm ci`, builds `ui/`, builds the locked Rust release
binary, and installs the binary, desktop entry, and hicolor icons into the
package archive.

Version tags publish the package and checksum to GitLab's Package Registry:

```sh
git push origin master
git tag -a v0.1.0 -m "Origami 0.1.0"
git push origin v0.1.0
```

Tag version must match `Cargo.toml`,
`crates/origami-app/tauri.conf.json`, `ui/package.json`, and
`packaging/arch/PKGBUILD`. Update all four before creating a release tag.

To run CI manually, open **Build > Pipelines > Run pipeline** in GitLab and
select branch or tag. Branch runs do not publish packages; only `vX.Y.Z` tags
run the publish stage.
