
/// Fetch and verify a pinned Eclipse Adoptium JDK release.
///
/// Adoptium's API reports the download link and its SHA-256 together, and the
/// GA list carries the patch history, so `release` pins the exact version while
/// the checksum still comes from upstream. A release Adoptium no longer
/// publishes fails closed rather than moving to a different version.
fn adoptium_jdk(release: &str, dest: &Path) -> Result<(), String> {
    let (os, arch) = adoptium_platform().ok_or_else(|| {
        "no managed JDK is published for this platform; install Java manually".to_string()
    })?;
    let feature = release
        .strip_prefix("jdk-")
        .and_then(|rest| rest.split('.').next())
        .ok_or_else(|| format!("misconfigured Adoptium release {release}"))?;
    let api = format!(
        "https://api.adoptium.net/v3/assets/feature_releases/{feature}/ga?os={os}&architecture={arch}&image_type=jdk&page_size=50&sort_order=DESC"
    );
    let body = curl_text(&api).map_err(|err| format!("could not query Adoptium: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("bad Adoptium response: {err}"))?;
    let entry = value
        .as_array()
        .and_then(|releases| {
            releases.iter().find(|entry| {
                entry.get("release_name").and_then(|name| name.as_str()) == Some(release)
            })
        })
        .ok_or_else(|| format!("Adoptium has no {release} release for {os}/{arch}"))?;
    let package = entry
        .pointer("/binaries/0/package")
        .ok_or_else(|| format!("Adoptium {release} had no JDK package for {os}/{arch}"))?;
    let link = package
        .get("link")
        .and_then(|link| link.as_str())
        .ok_or_else(|| "Adoptium package had no link".to_string())?;
    // Fail closed: an unverified JDK would be launched as `JAVA_HOME`, so never
    // fall back to "no checksum required".
    let checksum = package
        .get("checksum")
        .and_then(|sum| sum.as_str())
        .ok_or_else(|| {
            "Adoptium package had no checksum; refusing an unverified JDK".to_string()
        })?;
    download(link, dest, Some(checksum))
}

/// The Adoptium OS/architecture names for this platform.
fn adoptium_platform() -> Option<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some((os, arch))
}

/// The Runtime Identifier (RID) of the .NET SDK archive for this platform.
fn dotnet_platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "osx-x64",
        ("macos", "aarch64") => "osx-arm64",
        _ => return None,
    })
}

/// Download and verify the pinned .NET SDK that OmniSharp runs on.
///
/// Microsoft publishes each channel's `releases.json` with a SHA-512 for every
/// SDK archive, so Kemori downloads the versioned archive directly instead of
/// fetching and running the moving, unverified `dotnet-install.sh` script. The
/// pinned SDK version must be present in the channel or the install fails
/// closed rather than silently moving to another version.
fn dotnet_sdk(dest: &Path) -> Result<(), String> {
    let rid = dotnet_platform().ok_or_else(|| {
        "no .NET SDK is published for this platform; install the .NET SDK manually".to_string()
    })?;
    let body = curl_text(&format!(
        "https://builds.dotnet.microsoft.com/dotnet/release-metadata/{DOTNET_CHANNEL}/releases.json"
    ))
    .map_err(|err| format!("could not query the .NET release metadata: {err}"))?;
    let releases: serde_json::Value = serde_json::from_str(&body)
        .map_err(|err| format!("could not parse the .NET release metadata: {err}"))?;
    let sdk = releases
        .get("releases")
        .and_then(|value| value.as_array())
        .and_then(|releases| {
            releases.iter().find(|release| {
                release
                    .pointer("/sdk/version")
                    .and_then(|version| version.as_str())
                    == Some(DOTNET_SDK_VERSION)
            })
        })
        .and_then(|release| release.get("sdk"))
        .ok_or_else(|| {
            format!(".NET SDK {DOTNET_SDK_VERSION} is not in the {DOTNET_CHANNEL} channel")
        })?;
    let file = sdk
        .get("files")
        .and_then(|value| value.as_array())
        .and_then(|files| {
            files.iter().find(|file| {
                file.get("rid").and_then(|rid| rid.as_str()) == Some(rid)
                    && file
                        .get("name")
                        .and_then(|name| name.as_str())
                        .is_some_and(|name| name.ends_with(".tar.gz"))
            })
        })
        .ok_or_else(|| format!("the .NET SDK has no {rid} archive"))?;
    let url = file
        .get("url")
        .and_then(|url| url.as_str())
        .ok_or_else(|| "the .NET SDK archive had no URL".to_string())?;
    // Fail closed: a .NET SDK runs OmniSharp, so never install it unverified.
    let hash = file
        .get("hash")
        .and_then(|hash| hash.as_str())
        .filter(|hash| is_sha512_hex(hash))
        .ok_or_else(|| {
            "the .NET SDK archive had no SHA-512; refusing an unverified SDK".to_string()
        })?;
    download_sha512(url, dest, hash)
}

/// The Node.js distribution archive for this platform, or `None` when the
/// project publishes none.
fn node_asset() -> Option<String> {
    let (triple, extension) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-x64", "tar.xz"),
        ("linux", "aarch64") => ("linux-arm64", "tar.xz"),
        ("macos", "x86_64") => ("darwin-x64", "tar.gz"),
        ("macos", "aarch64") => ("darwin-arm64", "tar.gz"),
        ("windows", "x86_64") => ("win-x64", "zip"),
        _ => return None,
    };
    Some(format!("node-v{NODE_VERSION}-{triple}.{extension}"))
}

/// Whether Kemori can provision a managed Node.js runtime here: a published
/// archive for the platform, plus `curl` and the matching archive tool.
fn node_provisionable() -> bool {
    let Some(asset) = node_asset() else {
        return false;
    };
    let archive_tool = if asset.ends_with(".zip") {
        zip_extractor().is_some()
    } else {
        locate("tar").is_some()
    };
    locate("curl").is_some() && archive_tool
}

/// Download and verify the Node.js runtime archive.
///
/// Node.js publishes one `SHASUMS256.txt` per release, so Kemori reads the
/// checksum for *this* platform's asset and verifies it before extraction. The
/// archive is not unpacked here; an [`InstallStep::Extract`] follows.
fn node_runtime(dest: &Path) -> Result<(), String> {
    let asset = node_asset().ok_or_else(|| {
        "no managed Node.js is published for this platform; install Node.js manually".to_string()
    })?;
    let sums_url = format!("https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt");
    let output = install_command("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-sS",
            "-L",
            "--fail",
            "--connect-timeout",
            "30",
            "--max-time",
            "60",
        ])
        .arg(&sums_url)
        .output()
        .map_err(|err| format!("could not query the Node.js checksums: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not query the Node.js checksums: {}",
            first_stderr_line(&output.stderr)
        ));
    }
    // Fail closed: an unverified runtime would be executed to install and run
    // language servers, so never fall back to "no checksum required".
    let checksum = checksum_for(&String::from_utf8_lossy(&output.stdout), &asset)
        .ok_or_else(|| format!("Node.js published no checksum for {asset}"))?;
    let url = format!("https://nodejs.org/dist/v{NODE_VERSION}/{asset}");
    download(&url, dest, Some(&checksum))
}

/// Find the published SHA-256 for `asset` in a `SHASUMS256.txt` body.
///
/// The format is `<sha256>  <name>`, one entry per line. A malformed or
/// truncated line is ignored rather than trusted, and a leading `*` (binary
/// mode) is tolerated.
fn checksum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_string())
    })
}

/// The Dart SDK archive for this platform, or `None` when Google publishes none.
fn dart_sdk_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "dartsdk-linux-x64-release.zip",
        ("linux", "aarch64") => "dartsdk-linux-arm64-release.zip",
        ("macos", "x86_64") => "dartsdk-macos-x64-release.zip",
        ("macos", "aarch64") => "dartsdk-macos-arm64-release.zip",
        ("windows", "x86_64") => "dartsdk-windows-x64-release.zip",
        _ => return None,
    })
}

/// Download and verify a Kemori-managed Dart SDK.
///
/// Google publishes a sibling `.sha256sum` for every SDK archive, so Kemori reads
/// the checksum for *this* platform's asset and verifies it before extraction.
/// Only the Dart SDK is downloaded; Flutter is not, because the analysis server
/// is part of Dart and a Dart-only project does not need it.
fn dart_sdk(dest: &Path) -> Result<(), String> {
    let asset = dart_sdk_asset().ok_or_else(|| {
        "no managed Dart SDK is published for this platform; install the Dart SDK manually"
            .to_string()
    })?;
    let base = format!(
        "https://storage.googleapis.com/dart-archive/channels/stable/release/{DART_SDK_VERSION}/sdk/{asset}"
    );
    let sums_url = format!("{base}.sha256sum");
    let output = install_command("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-sS",
            "-L",
            "--fail",
            "--connect-timeout",
            "30",
            "--max-time",
            "60",
        ])
        .arg(&sums_url)
        .output()
        .map_err(|err| format!("could not query the Dart SDK checksum: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not query the Dart SDK checksum: {}",
            first_stderr_line(&output.stderr)
        ));
    }
    // Fail closed: the SDK runs as the language server, so never install it
    // without a verified checksum.
    let checksum = checksum_for(&String::from_utf8_lossy(&output.stdout), asset)
        .ok_or_else(|| format!("Dart published no checksum for {asset}"))?;
    download(&base, dest, Some(&checksum))
}
