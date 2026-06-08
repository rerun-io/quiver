# Release Checklist

Use `cargo semver-checks` to check if a new release can be a patch release.

* [ ] Update `CHANGELOG.md` using `./scripts/generate_changelog.py --version 0.NEW.VERSION`
* [ ] Improve the changelog and make sure each new feature and breaking change has its own line
* [ ] Bump version numbers in `Cargo.toml` and run `cargo check`.
* [ ] `git commit -m 'Release 0.x.0 - summary'`
* [ ] `cargo publish --quiet -p quiver_types`
* [ ] `cargo publish --quiet -p quiver_derive`
* [ ] `cargo publish --quiet -p quiver`
* [ ] `git tag -a 0.x.0 -m 'Release 0.x.0 - summary'`
* [ ] `git pull --tags ; git tag -d latest && git tag -a latest -m 'Latest release' && git push --tags origin latest --force ; git push --tags`
* [ ] Do a GitHub release: https://github.com/rerun-io/quiver/releases/new
