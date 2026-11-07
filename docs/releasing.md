# Releasing

A release is a version tag. CI builds the binaries and publishes the release;
the AUR packages are then updated by hand.

## 1. Bump the version

The version lives in five places, and they move together:

- `version` in `Cargo.toml`, then run `cargo build` so `Cargo.lock` follows.
- A new `<release version="…" date="…"/>` at the top of `<releases>` in
  `packaging/dev.douglasmoura.winkel.metainfo.xml`.
- A dated section in `CHANGELOG.md`.
- `pkgver` in `packaging/aur/winkel/PKGBUILD` and
  `packaging/aur/winkel-bin/PKGBUILD`, with `pkgrel=1`.

## 2. Check

```sh
make test
appstreamcli validate --no-net packaging/dev.douglasmoura.winkel.metainfo.xml
desktop-file-validate packaging/dev.douglasmoura.winkel.desktop
```

If the view changed, retake the README's screenshots on a Wayland session
with grim, ImageMagick and Hyprland:

```sh
cargo build --release
tools/screenshots.sh                     # over docs/screenshots
tools/screenshots.sh /tmp/shots          # or somewhere else, to compare first
```

It shoots the three views in each of the README's two themes under a HOME of
its own per theme. It reads each theme's `colors.toml` from
`~/.local/share/omarchy/themes` and the `shell.toml` omarchy generated for
the theme in use, so the two themes it shows must be installed.

## 3. Tag

```sh
git commit -am "release: vX.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

The `Release` workflow builds `winkel-X.Y.Z-x86_64-linux.tar.gz`
and `…-aarch64-linux.tar.gz` with their `.sha256` files and publishes them as
the GitHub release, with notes generated from the commits.

## 4. The AUR

The release workflow publishes `winkel`, `winkel-bin` and `winkel-git` to
the AUR once the release exists, through `.github/workflows/aur.yml`. It sets
each recipe's version and checksums with `packaging/aur/set-version.sh`,
regenerates `.SRCINFO`, and pushes with `packaging/aur/publish.sh`, committing
as `github-actions[bot]`. It authenticates with the `AUR_KEY` repository
secret, an SSH private key registered with the AUR account.

To publish a release again, or to try it without pushing:

```sh
gh workflow run aur.yml -f version=X.Y.Z -f dry_run=true    # prepare, push nothing
gh workflow run aur.yml -f version=X.Y.Z -f dry_run=false   # publish
```

To keep the recipes in this repository in step with the release, run
`packaging/aur/set-version.sh X.Y.Z` and commit the result.

Publishing from this machine still works, with an AUR key in `~/.ssh/config`:

```sh
packaging/aur/publish.sh --dry-run
packaging/aur/publish.sh
```

## 5. The Omarchy package repository

Omarchy users can already install from the AUR with
`omarchy pkg aur add winkel`. Shipping in Omarchy's own
repository, so `omarchy pkg add winkel` works with no AUR helper,
is up to Omarchy's maintainers.

`packaging/omarchy/winkel/` holds the two files a submission to
[omacom/omarchy-pkgs](https://github.com/omacom/omarchy-pkgs) needs:

- `PKGBUILD`, the source build, with Omarchy's dependency list style.
- `.omarchy/package.json`, which watches this repository's GitHub releases
  for `v*` tags, so Omarchy's pipeline picks up each one after a 24-hour
  quarantine, updating the version and checksums itself.

To submit, once a release exists:

```sh
gh repo fork omacom/omarchy-pkgs --clone
cd omarchy-pkgs
git switch -c add-winkel
cp -r ../winkel/packaging/omarchy/winkel pkgbuilds/
(cd pkgbuilds/winkel && updpkgsums)
git add pkgbuilds/winkel
git commit -m "Add winkel"
gh pr create --repo omacom/omarchy-pkgs --title "Add winkel"
```

Describe the app, link the source and its MIT licence, and say you are its
author and maintainer, as other package requests there do. After a release,
Omarchy's watch updates the version and checksums itself; the copy here only
matters for the first submission.
