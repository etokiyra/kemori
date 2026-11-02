# AUR packaging for Kemori

This directory contains two ready-to-push AUR packages:

| Directory | AUR package | Source |
| --- | --- | --- |
| `kemori/` | stable release | GitHub tag `v2.1.0` |
| `kemori-git/` | VCS package | `master` from GitHub |

Both install `/usr/bin/kemori`, so a normal Arch/Artix `sudo kemori` resolves the
binary through the default `secure_path`. Both also install the licence under
`/usr/share/licenses/` and the documentation under `/usr/share/doc/`.

## Validation already performed

On the packaging host (Artix, Arch-compatible):

```bash
makepkg -f --nodeps --noconfirm
namcap PKGBUILD
namcap <built-package>.pkg.tar.zst
```

- `kemori 0.1.2-1` built successfully; its package contains `/usr/bin/kemori`.
- `kemori-git` built successfully; its `pkgver()` derives the version from
  `git describe`, and the package provides and conflicts with `kemori`.
- `namcap` reported no findings for either PKGBUILD or package.
- Runtime dependencies are `glibc` and `libgcc`; language runtimes and servers
  are `optdepends`, because Kemori itself provisions or discovers them.

## Push the stable package

Replace `~/Projects/kemori` with this checkout if it lives elsewhere.

```bash
git clone ssh://aur@aur.archlinux.org/kemori.git /tmp/aur-kemori
cp ~/Projects/kemori/packaging/aur/kemori/PKGBUILD /tmp/aur-kemori/
cp ~/Projects/kemori/packaging/aur/kemori/.SRCINFO /tmp/aur-kemori/
cd /tmp/aur-kemori
git add PKGBUILD .SRCINFO
git commit -m "Initial import: kemori 0.1.2"
git push
```

## Push the VCS package

```bash
git clone ssh://aur@aur.archlinux.org/kemori-git.git /tmp/aur-kemori-git
cp ~/Projects/kemori/packaging/aur/kemori-git/PKGBUILD /tmp/aur-kemori-git/
cp ~/Projects/kemori/packaging/aur/kemori-git/.SRCINFO /tmp/aur-kemori-git/
cd /tmp/aur-kemori-git
git add PKGBUILD .SRCINFO
git commit -m "Initial import: kemori-git"
git push
```

Only the first push creates the AUR repository. If the names have been taken in
the meantime, rename the `pkgname`/directory consistently before pushing.

## Updating the stable package for a later release

1. Change `pkgver` in `packaging/aur/kemori/PKGBUILD`.
2. Refresh the checksum:

   ```bash
   cd packaging/aur/kemori
   updpkgsums          # or: makepkg -g >> PKGBUILD after deleting the old sum
   makepkg --printsrcinfo > .SRCINFO
   ```

3. Rebuild and re-run `namcap`, then commit and push the AUR repository.

`kemori-git` needs no version edit: `pkgver()` derives the version from
`git describe`. Its `.SRCINFO` is regenerated whenever the PKGBUILD changes.
