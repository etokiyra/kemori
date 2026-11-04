
/// Download `url` to `dest`, verifying a SHA-256 when one is known.
///
/// A verified artifact is reused from Kemori's cache when present, so a reinstall
/// or repair needs no network. The destination is replaced atomically only after
/// verification, so a failed download never clobbers a known-good file.
fn download(url: &str, dest: &Path, sha256: Option<&str>) -> Result<(), String> {
    match sha256 {
        Some(expected) => download_verified(
            url,
            dest,
            expected,
            HashKind::Sha256,
            cache_dir().as_deref(),
        ),
        // Unverified material (a signing key or a detached signature) is still
        // written atomically so a failed fetch cannot corrupt an existing copy.
        None => {
            let tmp = temp_sibling(dest)?;
            curl_download(url, &tmp)?;
            atomic_replace(&tmp, dest)
        }
    }
}

/// Download `url` to `dest`, verifying a SHA-512.
fn download_sha512(url: &str, dest: &Path, sha512: &str) -> Result<(), String> {
    download_verified(url, dest, sha512, HashKind::Sha512, cache_dir().as_deref())
}

/// Download and verify, reusing a verified cache entry when one exists.
///
/// Cache identity is the expected digest itself, never the filename, and a
/// cached file is verified again before it is used — a corrupt entry is removed
/// and the download retried. A freshly downloaded file is cached only after it
/// has passed verification.
fn download_verified(
    url: &str,
    dest: &Path,
    expected: &str,
    kind: HashKind,
    cache: Option<&Path>,
) -> Result<(), String> {
    if let Some(dir) = cache
        && let Some(cached) = cache_lookup(dir, expected, kind)?
    {
        return link_or_copy(&cached, dest);
    }

    let tmp = temp_sibling(dest)?;
    let result = (|| {
        curl_download(url, &tmp)?;
        verify_hash(&tmp, expected, kind)
    })();
    if let Err(err) = result {
        let _ = std::fs::remove_file(&tmp);
        // Make an offline/air-gapped failure actionable: say whether a cached
        // artifact exists that could have satisfied the request.
        let cached = cache
            .map(|dir| dir.join(cache_key(expected, kind)).is_file())
            .unwrap_or(false);
        if !cached {
            return Err(format!(
                "{err} (no cached artifact available; check the network)"
            ));
        }
        return Err(err);
    }
    // Only a verified artifact enters the cache, and a cache failure never
    // fails the install it is accelerating.
    if let Some(dir) = cache {
        let _ = cache_store(dir, expected, &tmp, kind);
    }
    atomic_replace(&tmp, dest)
}

/// Which digest algorithm a verification uses.
#[derive(Clone, Copy)]
enum HashKind {
    Sha256,
    Sha512,
}

impl HashKind {
    fn label(self) -> &'static str {
        match self {
            HashKind::Sha256 => "sha256",
            HashKind::Sha512 => "sha512",
        }
    }
}

/// Fetch `url` to `dest` with the shared, bounded curl invocation.
///
/// No verification happens here; callers must run [`verify_hash`] on the result.
fn curl_download(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    // Tests exercise the real install path (a `Download` step) without the
    // network by copying a local payload, so verification is still the same
    // code the production path runs. Never available in a production build.
    #[cfg(test)]
    if let Some(source) = url.strip_prefix("file://") {
        std::fs::copy(source, dest).map_err(|err| format!("could not copy {source}: {err}"))?;
        return Ok(());
    }
    let mut command = install_command("curl");
    command.args([
        "--proto",
        "=https",
        "--tlsv1.2",
        "-L",
        "--fail",
        "-sS",
        // Decode a gzip transfer encoding. Some CDNs compress text responses
        // (for example a PGP key block) regardless of the file extension; without
        // this the downloaded key file is gzip bytes and the signature check fails.
        "--compressed",
        "--connect-timeout",
        "30",
        // Retry transient failures (a reset connection, a 5xx) a few times
        // before giving up, so a large managed download survives a blip.
        "--retry",
        "4",
        "--retry-delay",
        "2",
        "--retry-max-time",
        "180",
        "--max-time",
        "3600",
        // Abort a genuinely stalled transfer (under 1 KiB/s for a minute) rather
        // than letting it sit until the hour-long cap, while still allowing a
        // large managed toolchain to finish on a slow-but-steady link.
        "--speed-limit",
        "1024",
        "--speed-time",
        "60",
    ]);
    apply_proxy(&mut command);
    command
        .arg("--max-filesize")
        .arg(MAX_DOWNLOAD_BYTES.to_string())
        .arg("-o")
        .arg(dest)
        .arg(url);
    let output = command
        .output()
        .map_err(|err| format!("could not run curl: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "download failed: {}{}",
            first_stderr_line(&output.stderr),
            download_failure_hint(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    Ok(())
}

/// A short, actionable hint appended to a failed download, based on curl output.
///
/// The raw cause is kept for the curious; this adds what the user should try.
fn download_failure_hint(stderr: &str) -> &'static str {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("could not resolve host") || lower.contains("name or service not known") {
        " — no network access to the download host; check your connection or DNS"
    } else if lower.contains("proxy")
        || lower.contains("could not connect")
        || lower.contains("connection refused")
    {
        " — the host or proxy refused the connection; check HTTPS_PROXY"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        " — the connection timed out; retry, or check HTTPS_PROXY"
    } else if lower.contains("ssl") || lower.contains("certificate") || lower.contains("tls") {
        " — the TLS connection failed; a proxy or firewall may be intercepting it"
    } else {
        " — retry the install; if it keeps failing, check your network and HTTPS_PROXY"
    }
}

/// Verify `dest` against `expected`, removing the file on any mismatch.
///
/// Fails closed: a wrong or truncated download is deleted before the caller can
/// use it, so a corrupt archive can never become an installed tool.
fn verify_hash(dest: &Path, expected: &str, kind: HashKind) -> Result<(), String> {
    let actual = match kind {
        HashKind::Sha256 => file_sha256(dest)?,
        HashKind::Sha512 => file_sha512(dest)?,
    };
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_file(dest);
        return Err(format!(
            "checksum mismatch for {} (expected {expected}, got {actual}); the download was discarded and not installed",
            dest.display()
        ));
    }
    Ok(())
}

/// Kemori's cache of verified provisioning artifacts.
///
/// A sibling of the managed tools directory, keyed by the artifact's own digest
/// so two different artifacts can never collide and a cached entry is always
/// re-verified before use.
fn cache_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("cache"))
}

/// The cache file name for an expected digest: `<algorithm>-<hex>`.
fn cache_key(expected: &str, kind: HashKind) -> String {
    format!("{}-{}", kind.label(), expected.to_ascii_lowercase())
}

/// Return a verified cache entry for `expected`, removing a corrupt one.
///
/// "Exists in the cache" is never trusted: the digest is recomputed and
/// compared before the path is returned.
fn cache_lookup(dir: &Path, expected: &str, kind: HashKind) -> Result<Option<PathBuf>, String> {
    let path = dir.join(cache_key(expected, kind));
    if !path.is_file() {
        return Ok(None);
    }
    match verify_hash(&path, expected, kind) {
        Ok(()) => Ok(Some(path)),
        Err(_) => {
            // A wrong or truncated entry is discarded, never used.
            let _ = std::fs::remove_file(&path);
            Ok(None)
        }
    }
}

/// Store a verified artifact under its digest, atomically and best-effort.
fn cache_store(dir: &Path, expected: &str, source: &Path, kind: HashKind) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
    let dest = dir.join(cache_key(expected, kind));
    if dest.is_file() {
        return Ok(());
    }
    let tmp = temp_sibling(&dest)?;
    if std::fs::hard_link(source, &tmp).is_err() {
        std::fs::copy(source, &tmp)
            .map_err(|err| format!("could not cache the download: {err}"))?;
    }
    let result = atomic_replace(&tmp, &dest);
    // Best-effort: keep the verified cache from growing without bound. Eviction
    // never fails the install it is accelerating.
    let _ = evict_cache_if_needed(dir);
    result
}

/// Best-effort cache eviction: when the verified cache exceeds a soft limit,
/// remove the oldest entries until it is back under a lower target. Only files
/// directly in `dir` are considered; the metadata cache lives beside them and is
/// evicted together.
fn evict_cache_if_needed(dir: &Path) -> std::io::Result<()> {
    const SOFT_LIMIT: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
    const TARGET: u64 = 3 * 1024 * 1024 * 1024 / 2; // 1.5 GiB
    let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    let mut total = 0u64;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        total += metadata.len();
        entries.push((
            path,
            metadata.len(),
            metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
        ));
    }
    if total <= SOFT_LIMIT {
        return Ok(());
    }
    entries.sort_by_key(|entry| entry.2);
    for (path, size, _) in entries {
        if total <= TARGET {
            break;
        }
        let _ = std::fs::remove_file(&path);
        total = total.saturating_sub(size);
    }
    Ok(())
}

/// Place `src` at `dest` without destroying `dest` if the move fails.
///
/// Prefers a hard link (the cache and the managed tools share a filesystem), so
/// a cached artifact costs no extra disk; falls back to a copy.
fn link_or_copy(src: &Path, dest: &Path) -> Result<(), String> {
    let tmp = temp_sibling(dest)?;
    if std::fs::hard_link(src, &tmp).is_err() {
        std::fs::copy(src, &tmp)
            .map_err(|err| format!("could not use the cached artifact: {err}"))?;
    }
    atomic_replace(&tmp, dest)
}

/// A unique sibling path for a temporary file or directory beside `path`.
fn temp_sibling(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("no parent directory for {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    Ok(parent.join(format!(".{name}.kemori-{}-{unique}", std::process::id())))
}

/// Move `src` over `dest` in one step, creating `dest`'s parent if needed.
fn atomic_replace(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    std::fs::rename(src, dest).map_err(|err| {
        format!(
            "could not move {} into place at {}: {err}",
            src.display(),
            dest.display()
        )
    })
}

/// The proxy to pass to `curl`, chosen from the standard environment variables.
///
/// `HTTPS_PROXY`/`https_proxy` win for our all-HTTPS downloads, then
/// `ALL_PROXY`, then `HTTP_PROXY` (some proxies serve HTTPS tunnelling through
/// the plain proxy variable). An empty value is ignored.
fn proxy_from_env(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Some(value) = env(key)
            && !value.is_empty()
        {
            return Some(value);
        }
    }
    None
}

/// Forward the selected proxy environment variable to `curl`.
///
/// `curl` honours these variables itself, but passing the selected one
/// explicitly keeps Kemori's intent visible and testable. This forwards the
/// variable; it does not establish or validate a live proxy.
fn apply_proxy(command: &mut Command) {
    if let Some(proxy) = proxy_from_env(|key| std::env::var(key).ok()) {
        command.arg("--proxy").arg(proxy);
    }
}

/// Free space in bytes on the filesystem containing `path`, or `None` when it
/// cannot be determined.
///
/// Uses `df -Pk` (POSIX output, 1024-byte blocks), which is available on Linux
/// and macOS. A not-yet-created path is resolved to its nearest existing
/// ancestor so a first install still gets a check.
fn free_disk_bytes(path: &Path) -> Option<u64> {
    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let output = install_command("df").arg("-Pk").arg(probe).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // The POSIX format keeps one filesystem on one line; the last line is the
    // data row and its fourth column is the available block count.
    let line = text.lines().last()?;
    let available = line.split_whitespace().nth(3)?.parse::<u64>().ok()?;
    available.checked_mul(1024)
}

/// Refuse an install when the target filesystem cannot hold it.
///
/// A clear error beats a half-written toolchain. When free space cannot be
/// measured (`df` missing or unparsable) the check is skipped rather than
/// blocking a valid install, and a size Kemori does not know is not guessed.
fn ensure_disk_space(dest: &Path, needed: u64) -> Result<(), String> {
    let Some(free) = free_disk_bytes(dest) else {
        return Ok(());
    };
    if free < needed {
        return Err(format!(
            "not enough free disk space under {}: need about {}, only {} is available",
            dest.display(),
            human_bytes(needed),
            human_bytes(free)
        ));
    }
    Ok(())
}
