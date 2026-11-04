
/// Refuse an archive whose entries could escape the extraction directory.
///
/// GNU `tar` and Info-ZIP `unzip` sanitize `..` and absolute paths, but a
/// minimal system's tools may not, and a downloaded archive is untrusted input.
/// The listing is bounded by the archive's own entry count and any failure to
/// list is treated as unsafe, so this fails closed.
fn ensure_archive_paths_safe(archive: &Path, zip: bool) -> Result<(), String> {
    let listing = archive_listing(archive, zip)?;
    for line in listing.lines() {
        let entry = line.trim_end_matches('/');
        if entry.starts_with('/') || entry.starts_with('\\') {
            return Err(format!(
                "{} contains an absolute path: {entry}",
                archive.display()
            ));
        }
        if entry.split(['/', '\\']).any(|part| part == "..") {
            return Err(format!(
                "{} contains a parent path: {entry}",
                archive.display()
            ));
        }
    }
    Ok(())
}

/// List an archive's entry names using whichever tool can read it.
///
/// Zip archives are listed with the same extractor [`zip_extractor`] selects, so
/// a system without `unzip` is not refused by the safety check alone. A listing
/// that cannot be produced fails closed.
fn archive_listing(archive: &Path, zip: bool) -> Result<String, String> {
    if !zip {
        return listing_from(
            install_command("tar").arg("-tf").arg(archive).output(),
            archive,
        );
    }
    let extractor = zip_extractor().ok_or_else(|| {
        format!(
            "no zip extractor found (looked for unzip, bsdtar and python3) to inspect {}",
            archive.display()
        )
    })?;
    zip_listing(extractor, archive)
}

/// List a zip archive's entries with a specific extractor.
fn zip_listing(extractor: ZipExtractor, archive: &Path) -> Result<String, String> {
    let output = match extractor {
        ZipExtractor::Unzip => install_command("unzip").arg("-Z1").arg(archive).output(),
        ZipExtractor::BsdTar => install_command("bsdtar").arg("-tf").arg(archive).output(),
        ZipExtractor::Python => install_command("python3")
            .args([
                "-c",
                "import sys,zipfile;print('\\n'.join(zipfile.ZipFile(sys.argv[1]).namelist()))",
            ])
            .arg(archive)
            .output(),
    };
    listing_from(output, archive)
}

/// Return a listing command's stdout, or a failure that names the archive.
fn listing_from(
    output: std::io::Result<std::process::Output>,
    archive: &Path,
) -> Result<String, String> {
    let output = output.map_err(|err| format!("could not list {}: {err}", archive.display()))?;
    if !output.status.success() {
        return Err(format!(
            "could not list {}: {}",
            archive.display(),
            first_stderr_line(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Extract a `.tar.gz`/`.tar.xz`/`.zip` archive into `dest`.
///
/// `tar` supports `--strip-components`; `unzip` does not, so a stripped zip is
/// unpacked into a scratch directory and the leading path components are moved
/// up by hand.
fn extract(archive: &Path, dest: &Path, strip: usize) -> Result<(), String> {
    // Unpack into a staging directory beside the destination, then promote it in
    // one same-filesystem rename. A failed extraction leaves the previous
    // installation untouched, and a failed promotion restores it.
    let staging = temp_sibling(dest)?;
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|err| format!("could not create {}: {err}", staging.display()))?;

    let result = (|| {
        // The archive's own size is a real lower bound for the unpacked tree;
        // there is no separate extracted-size estimate, so do not invent one.
        if let Ok(metadata) = std::fs::metadata(archive) {
            ensure_disk_space(&staging, metadata.len())?;
        }
        let zip = archive.extension().and_then(|ext| ext.to_str()) == Some("zip");
        // Refuse an archive that could write outside the staging directory.
        ensure_archive_paths_safe(archive, zip)?;

        if zip {
            return extract_zip(archive, &staging, strip);
        }
        let mut command = install_command("tar");
        command.arg("-xf").arg(archive).arg("-C").arg(&staging);
        if strip > 0 {
            command.arg(format!("--strip-components={strip}"));
        }
        match command.output() {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(format!(
                "could not extract {}: {}",
                archive.display(),
                first_stderr_line(&output.stderr)
            )),
            Err(err) => Err(format!("could not run the archive tool: {err}")),
        }
    })();

    if let Err(err) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(err);
    }
    promote_directory(&staging, dest)
}

/// Move a staged directory into its final location without destroying the
/// previous installation if the move fails.
///
/// The existing directory is renamed aside first, then restored if the promotion
/// fails. On success the backup is removed.
fn promote_directory(staging: &Path, dest: &Path) -> Result<(), String> {
    let backup = temp_sibling(dest)?;
    let _ = std::fs::remove_dir_all(&backup);
    let had_old = dest.exists();
    if had_old {
        std::fs::rename(dest, &backup).map_err(|err| {
            let _ = std::fs::remove_dir_all(staging);
            format!("could not set aside the existing {}: {err}", dest.display())
        })?;
    }
    match std::fs::rename(staging, dest) {
        Ok(()) => {
            if had_old {
                let _ = std::fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(err) => {
            if had_old {
                // Put the known-good installation back.
                let _ = std::fs::rename(&backup, dest);
            }
            let _ = std::fs::remove_dir_all(staging);
            Err(format!("could not install into {}: {err}", dest.display()))
        }
    }
}

/// Which tool Kemori uses to unpack a zip archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZipExtractor {
    Unzip,
    BsdTar,
    Python,
}

/// The first available zip extractor: Info-ZIP `unzip`, then `bsdtar` (libarchive,
/// the macOS default), then a Python interpreter. This keeps zip-based bundles
/// installable on minimal systems without `unzip`.
fn zip_extractor() -> Option<ZipExtractor> {
    if locate("unzip").is_some() {
        Some(ZipExtractor::Unzip)
    } else if locate("bsdtar").is_some() {
        Some(ZipExtractor::BsdTar)
    } else if locate("python3").is_some() {
        Some(ZipExtractor::Python)
    } else {
        None
    }
}

/// Extract a zip archive, using whichever extractor is available.
fn extract_zip(archive: &Path, dest: &Path, strip: usize) -> Result<(), String> {
    let extractor = zip_extractor().ok_or_else(|| {
        format!(
            "no zip extractor found (looked for unzip, bsdtar and python3) to unpack {}",
            archive.display()
        )
    })?;
    extract_zip_with(extractor, archive, dest, strip)
}

/// Extract a zip archive with a specific extractor.
fn extract_zip_with(
    extractor: ZipExtractor,
    archive: &Path,
    dest: &Path,
    strip: usize,
) -> Result<(), String> {
    std::fs::create_dir_all(dest)
        .map_err(|err| format!("could not create {}: {err}", dest.display()))?;
    if extractor == ZipExtractor::BsdTar {
        // libarchive understands `--strip-components` for zip archives.
        let mut command = install_command("bsdtar");
        command.arg("-xf").arg(archive).arg("-C").arg(dest);
        if strip > 0 {
            command.arg(format!("--strip-components={strip}"));
        }
        return match command.output() {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(format!(
                "could not extract {}: {}",
                archive.display(),
                first_stderr_line(&output.stderr)
            )),
            Err(err) => Err(format!("could not run bsdtar: {err}")),
        };
    }

    // `unzip` and Python have no `--strip-components`, so unpack into a scratch
    // directory and move the wanted level up by hand.
    let scratch = dest.with_extension("kemori-extract");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)
        .map_err(|err| format!("could not create {}: {err}", scratch.display()))?;
    let result = (|| {
        let output = match extractor {
            ZipExtractor::Unzip => install_command("unzip")
                .args(["-q", "-o"])
                .arg(archive)
                .arg("-d")
                .arg(&scratch)
                .output()
                .map_err(|err| format!("could not run unzip: {err}"))?,
            ZipExtractor::Python => install_command("python3")
                .args(["-m", "zipfile", "-e"])
                .arg(archive)
                .arg(&scratch)
                .output()
                .map_err(|err| format!("could not run python3: {err}"))?,
            ZipExtractor::BsdTar => unreachable!(),
        };
        if !output.status.success() {
            return Err(format!(
                "could not extract {}: {}",
                archive.display(),
                first_stderr_line(&output.stderr)
            ));
        }
        let mut root = scratch.clone();
        for _ in 0..strip {
            root = single_child_directory(&root).ok_or_else(|| {
                format!(
                    "archive {} does not have {strip} leading directory level(s)",
                    archive.display()
                )
            })?;
        }
        move_directory_contents(&root, dest)
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

/// The only child of `dir` when it is a directory, else `None`.
fn single_child_directory(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir).ok()?.flatten();
    let first = entries.next()?;
    if entries.next().is_some() {
        return None;
    }
    let path = first.path();
    path.is_dir().then_some(path)
}

/// Move every entry of `from` into `to`, replacing same-named entries.
fn move_directory_contents(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|err| format!("could not create {}: {err}", to.display()))?;
    let entries = std::fs::read_dir(from)
        .map_err(|err| format!("could not read {}: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("could not read {}: {err}", from.display()))?;
        let target = to.join(entry.file_name());
        if target.exists() {
            if target.is_dir() {
                std::fs::remove_dir_all(&target)
            } else {
                std::fs::remove_file(&target)
            }
            .map_err(|err| format!("could not replace {}: {err}", target.display()))?;
        }
        std::fs::rename(entry.path(), &target).map_err(|err| {
            format!(
                "could not move {} into {}: {err}",
                entry.path().display(),
                to.display()
            )
        })?;
    }
    Ok(())
}
