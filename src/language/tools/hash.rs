
/// SHA-256 of a file, using whichever tool the platform provides.
fn file_sha256(path: &Path) -> Result<String, String> {
    // `sha256sum` (coreutils, busybox) and `shasum` (macOS) print the hex first.
    for (program, args) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        if let Ok(output) = install_command(program).args(args).arg(path).output()
            && output.status.success()
            && let Some(hash) = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
            && is_sha256_hex(hash)
        {
            return Ok(hash.to_string());
        }
    }
    // `openssl dgst -sha256` prints `SHA256(name)= <hex>`.
    if let Ok(output) = install_command("openssl")
        .args(["dgst", "-sha256"])
        .arg(path)
        .output()
        && output.status.success()
        && let Some(hash) = String::from_utf8_lossy(&output.stdout)
            .split('=')
            .nth(1)
            .map(str::trim)
        && is_sha256_hex(hash)
    {
        return Ok(hash.to_string());
    }
    // A Python interpreter, present on many minimal systems without coreutils.
    if let Ok(output) = install_command("python3")
        .args([
            "-c",
            "import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())",
        ])
        .arg(path)
        .output()
        && output.status.success()
        && let Some(hash) = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
        && is_sha256_hex(hash)
    {
        return Ok(hash.to_string());
    }
    Err("no SHA-256 tool found (looked for sha256sum, shasum, openssl and python3)".to_string())
}

/// Whether `value` is a 64-character lowercase-or-uppercase hex SHA-256.
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether `value` is a 128-character lowercase-or-uppercase hex SHA-512.
fn is_sha512_hex(value: &str) -> bool {
    value.len() == 128 && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// Compute a file's SHA-512 with whichever hashing tool the system provides.
///
/// Same fail-closed fallback chain as [`file_sha256`].
fn file_sha512(path: &Path) -> Result<String, String> {
    for (program, args) in [("sha512sum", &[][..]), ("shasum", &["-a", "512"][..])] {
        if let Ok(output) = install_command(program).args(args).arg(path).output()
            && output.status.success()
            && let Some(hash) = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
            && is_sha512_hex(hash)
        {
            return Ok(hash.to_string());
        }
    }
    if let Ok(output) = install_command("openssl")
        .args(["dgst", "-sha512"])
        .arg(path)
        .output()
        && output.status.success()
        && let Some(hash) = String::from_utf8_lossy(&output.stdout)
            .split('=')
            .nth(1)
            .map(str::trim)
        && is_sha512_hex(hash)
    {
        return Ok(hash.to_string());
    }
    if let Ok(output) = install_command("python3")
        .args([
            "-c",
            "import hashlib,sys;print(hashlib.sha512(open(sys.argv[1],'rb').read()).hexdigest())",
        ])
        .arg(path)
        .output()
        && output.status.success()
        && let Some(hash) = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
        && is_sha512_hex(hash)
    {
        return Ok(hash.to_string());
    }
    Err("no SHA-512 tool found (looked for sha512sum, shasum, openssl and python3)".to_string())
}
