
/// Fetch a small text/JSON document over HTTPS with `curl`.
///
/// Used to read checksums and API responses; it never writes to disk, and the
/// body is bounded. No `Accept` header is sent, so it works with any HTTPS
/// JSON/txt endpoint; use [`curl_text_github`] for the GitHub API.
fn curl_text(url: &str) -> Result<String, String> {
    curl_text_with_accept(url, None)
}

/// Fetch a GitHub API document with the vendor media type GitHub expects.
fn curl_text_github(url: &str) -> Result<String, String> {
    curl_text_with_accept(url, Some("application/vnd.github+json"))
}

/// Fetch a small document with `curl`, optionally sending an `Accept` header.
///
/// A successful fetch refreshes a URL-keyed cache of the body; if the network is
/// unavailable the cached body is used instead. The digest inside cached
/// metadata is still applied to the artifact, so a cached checksum can only make
/// an offline install possible, never skip verification.
fn curl_text_with_accept(url: &str, accept: Option<&str>) -> Result<String, String> {
    match curl_text_now(url, accept) {
        Ok(body) => {
            let _ = remember_metadata(url, &body);
            Ok(body)
        }
        Err(error) => match cached_metadata(url) {
            Some(body) => Ok(body),
            None => Err(error),
        },
    }
}

/// Fetch a small document with `curl` and no fallback.
fn curl_text_now(url: &str, accept: Option<&str>) -> Result<String, String> {
    let mut command = install_command("curl");
    command.args([
        "--proto",
        "=https",
        "--tlsv1.2",
        "-sS",
        "-L",
        "--fail",
        "--compressed",
        "--connect-timeout",
        "30",
        "--max-time",
        "120",
    ]);
    if let Some(accept) = accept {
        command.arg("-H").arg(format!("Accept: {accept}"));
    }
    // The .NET release metadata is ~1 MB and grows with each patch, so leave
    // headroom while keeping the body bounded.
    command.arg("--max-filesize").arg("4194304").arg(url);
    let output = command
        .output()
        .map_err(|err| format!("could not run curl: {err}"))?;
    if !output.status.success() {
        return Err(first_stderr_line(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The cache path for a metadata document, keyed by its URL.
fn metadata_cache_path(url: &str) -> Option<PathBuf> {
    Some(
        cache_dir()?
            .join("metadata")
            .join(format!("{:016x}", fnv1a(url))),
    )
}

/// Remember a fetched metadata body for offline fallback, atomically.
fn remember_metadata(url: &str, body: &str) -> Result<(), String> {
    let Some(path) = metadata_cache_path(url) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    let tmp = temp_sibling(&path)?;
    std::fs::write(&tmp, body).map_err(|err| format!("could not cache metadata: {err}"))?;
    atomic_replace(&tmp, &path)
}

/// A previously fetched metadata body for `url`, if one was cached.
fn cached_metadata(url: &str) -> Option<String> {
    let path = metadata_cache_path(url)?;
    std::fs::read_to_string(path).ok()
}

/// A small, stable, dependency-free hash used to name metadata cache entries.
fn fnv1a(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Download a file and verify it against a detached GPG signature.
///
/// The keys are imported into a private keyring under Kemori's tools directory, so
/// the user's own GPG configuration is never touched. Verification is
/// fail-closed: a missing `gpg`, a missing signature, or a signature that does
/// not match leaves no downloaded file behind.
fn download_gpg(url: &str, dest: &Path, signature_url: &str, keys_url: &str) -> Result<(), String> {
    if locate("gpg").is_none() {
        return Err(
            "gpg is required to verify this download — install gnupg and try again".to_string(),
        );
    }
    let Some(home) = gnupg_home() else {
        return Err("could not locate Kemori's data directory for a GPG keyring".to_string());
    };
    let Some(downloads) = downloads_dir() else {
        return Err("could not locate Kemori's download directory".to_string());
    };
    let keys = downloads.join("release-keys.asc");
    let signature = append_extension(dest, "sig");
    std::fs::create_dir_all(&downloads)
        .map_err(|err| format!("could not create {}: {err}", downloads.display()))?;

    // Offline reuse: if the archive, its signature and the keys are all already
    // present, verify them before making any network request. The signature is
    // kept after a successful install precisely so this path works offline.
    if dest.is_file()
        && signature.is_file()
        && keys.is_file()
        && verify_signature(&home, &keys, &signature, dest).is_ok()
    {
        return Ok(());
    }

    // Fetch the keys and the signature first, then the archive, so a failure
    // never leaves a large unverified file on disk.
    download(keys_url, &keys, None)?;
    download(signature_url, &signature, None)?;
    // Reuse an archive that already verifies, so re-running an install (or
    // recovering from a network failure) does not re-download a large toolchain.
    if dest.is_file() && verify_signature(&home, &keys, &signature, dest).is_ok() {
        return Ok(());
    }
    // A stale or partial file cannot be trusted; start the download fresh.
    let _ = std::fs::remove_file(dest);
    download(url, dest, None)?;

    let result = verify_signature(&home, &keys, &signature, dest);
    if result.is_err() {
        // Never keep an archive whose provenance could not be proven.
        let _ = std::fs::remove_file(dest);
    }
    result
}

/// Import `keys` into the isolated keyring at `home` and check `signature`
/// against `file`.
fn verify_signature(home: &Path, keys: &Path, signature: &Path, file: &Path) -> Result<(), String> {
    std::fs::create_dir_all(home)
        .map_err(|err| format!("could not create {}: {err}", home.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700));
    }

    let imported = install_command("gpg")
        .env("GNUPGHOME", home)
        .args(["--batch", "--quiet", "--import"])
        .arg(keys)
        .output()
        .map_err(|err| format!("could not run gpg: {err}"))?;
    if !imported.status.success() {
        return Err(format!(
            "could not import the release key: {}",
            first_stderr_line(&imported.stderr)
        ));
    }

    let verified = install_command("gpg")
        .env("GNUPGHOME", home)
        .args(["--batch", "--status-fd", "1", "--verify"])
        .arg(signature)
        .arg(file)
        .output()
        .map_err(|err| format!("could not run gpg: {err}"))?;
    let status = String::from_utf8_lossy(&verified.stdout);
    let trusted = status.contains("VALIDSIG") || status.contains("GOODSIG");
    if !verified.status.success() || !trusted {
        return Err(format!(
            "signature verification failed for {}: {}",
            file.display(),
            first_stderr_line(&verified.stderr)
        ));
    }
    Ok(())
}

/// The SHA-256 GitHub's API reports for a release asset, if present.
///
/// GitHub added a `digest` field (`sha256:<hex>`) to release assets, which lets
/// Kemori verify an asset without a separately published checksum file.
fn github_asset_sha256(body: &str, asset: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let assets = value.get("assets")?.as_array()?;
    for entry in assets {
        if entry.get("name").and_then(|name| name.as_str()) != Some(asset) {
            continue;
        }
        let digest = entry.get("digest").and_then(|digest| digest.as_str())?;
        let hex = digest.strip_prefix("sha256:")?;
        if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hex.to_string());
        }
    }
    None
}

/// Download a pinned GitHub release asset, verifying the digest the API reports.
fn github_release(repo: &str, tag: &str, asset: &str, dest: &Path) -> Result<(), String> {
    let api = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let body = curl_text_github(&api)
        .map_err(|err| format!("could not query the {repo} release metadata: {err}"))?;
    // Fail closed: an unverified asset is never run.
    let checksum = github_asset_sha256(&body, asset)
        .ok_or_else(|| format!("GitHub published no checksum digest for {asset}"))?;
    let url = format!("https://github.com/{repo}/releases/download/{tag}/{asset}");
    download(&url, dest, Some(&checksum))
}
