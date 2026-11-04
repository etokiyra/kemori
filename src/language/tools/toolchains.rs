/// Create Kemori's Swift compatibility directory.
///
/// The official toolchain's only mismatches with a non-native distribution are
/// the ncurses soname (Kemori's build expects `libncurses.so.6`, the system ships
/// `libncursesw.so.6`, the same library) and `libxml2.so.2`, which some rolling
/// distributions provide through a compatibility package. Nothing in the system
/// is copied or modified: the directory holds links that a scoped
/// `LD_LIBRARY_PATH` uses to launch `sourcekit-lsp`.
fn swift_compat(dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest)
        .map_err(|err| format!("could not create {}: {err}", dest.display()))?;
    // Alias the narrow ncurses-family sonames to the wide libraries the system
    // actually ships.
    for narrow in ["libncurses.so.6", "libpanel.so.6", "libform.so.6"] {
        if system_library(&[narrow]).is_some() {
            continue;
        }
        let wide = narrow.replace(".so.6", "w.so.6");
        let Some(target) = system_library(&[&wide]) else {
            return Err(format!(
                "{narrow} is required by the Swift toolchain but neither it nor {wide} exists"
            ));
        };
        link_into(dest, narrow, &target)?;
    }
    let Some(xml) = system_library(&["libxml2.so.2"]) else {
        return Err(
            "libxml2.so.2 is required by the Swift toolchain; install your distribution's \
             libxml2 compatibility package (on Arch: libxml2-legacy)"
                .to_string(),
        );
    };
    link_into(dest, "libxml2.so.2", &xml)?;
    Ok(())
}

/// Create (or refresh) `dir/name` as a symlink to `target`.
#[cfg(unix)]
fn link_into(dir: &Path, name: &str, target: &Path) -> Result<(), String> {
    let link = dir.join(name);
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(target, &link).map_err(|err| {
        format!(
            "could not link {} → {}: {err}",
            link.display(),
            target.display()
        )
    })
}

#[cfg(not(unix))]
fn link_into(_dir: &Path, _name: &str, _target: &Path) -> Result<(), String> {
    Err("the Swift compatibility layer is only used on Linux".to_string())
}

/// Mark a downloaded single-file program as executable.
fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path)
            .map_err(|err| format!("could not stat {}: {err}", path.display()))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)
            .map_err(|err| format!("could not make {} executable: {err}", path.display()))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Write a small executable shell launcher for a runtime-run script.
fn write_launcher(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    crate::filesystem::write_atomic(path, contents)
        .map_err(|err| format!("could not write {}: {err}", path.display()))?;
    make_executable(path)
}

/// The `go.dev` target names for this host, if Go publishes a build for it.
fn go_platform() -> Option<(&'static str, &'static str)> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux", "amd64"),
        ("linux", "aarch64") => ("linux", "arm64"),
        ("macos", "x86_64") => ("darwin", "amd64"),
        ("macos", "aarch64") => ("darwin", "arm64"),
        ("windows", "x86_64") => ("windows", "amd64"),
        ("windows", "aarch64") => ("windows", "arm64"),
        _ => return None,
    })
}

/// Download the pinned official Go toolchain.
///
/// `go.dev/dl/?mode=json&include=all` reports every release and the SHA-256 of
/// each archive, so Kemori selects `GO_VERSION` and verifies against upstream
/// metadata instead of tracking the latest stable. The archive root is `go/`, so
/// a later [`InstallStep::Extract`] with `strip: 1` yields `go/bin/go`.
fn go_toolchain(dest: &Path) -> Result<(), String> {
    let (os, arch) = go_platform()
        .ok_or_else(|| "no official Go build is published for this platform".to_string())?;
    let body = curl_text("https://go.dev/dl/?mode=json&include=all")
        .map_err(|err| format!("could not query the Go release metadata: {err}"))?;
    let releases: serde_json::Value = serde_json::from_str(&body)
        .map_err(|err| format!("could not parse the Go release metadata: {err}"))?;
    let release = releases
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|release| release.get("version").and_then(|v| v.as_str()) == Some(GO_VERSION))
        })
        .ok_or_else(|| format!("go.dev does not publish {GO_VERSION}"))?;
    let files = release
        .get("files")
        .and_then(|files| files.as_array())
        .ok_or_else(|| "the Go release has no files".to_string())?;
    let mut filename = None;
    let mut sha256 = None;
    for file in files {
        let matches = file.get("os").and_then(|v| v.as_str()) == Some(os)
            && file.get("arch").and_then(|v| v.as_str()) == Some(arch)
            && file.get("kind").and_then(|v| v.as_str()) == Some("archive");
        if matches {
            filename = file
                .get("filename")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            sha256 = file
                .get("sha256")
                .and_then(|v| v.as_str())
                .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
                .map(str::to_string);
        }
    }
    let filename = filename.ok_or_else(|| format!("Go published no {os}/{arch} archive"))?;
    // Fail closed: Go runs Kemori's tooling, so never install it unverified.
    let sha256 = sha256.ok_or_else(|| format!("Go published no checksum for {filename}"))?;
    download(
        &format!("https://go.dev/dl/{filename}"),
        dest,
        Some(&sha256),
    )
}

/// A sibling path with an extra extension appended (`a.tar.gz` → `a.tar.gz.sig`).
fn append_extension(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(extension);
    PathBuf::from(name)
}

/// Read `/etc/os-release` into `(ID, ID_LIKE, VERSION_ID)`.
///
/// This is how Kemori picks the right toolchain artifact for the running
/// distribution without guessing from a kernel version.
fn os_release() -> Option<(String, String, String)> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    let mut id = String::new();
    let mut id_like = String::new();
    let mut version = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "ID" => id = value,
            "ID_LIKE" => id_like = value,
            "VERSION_ID" => version = value,
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some((id, id_like, version))
}

/// Map a distribution to the swift.org platform tag and artifact suffix.
///
/// swift.org publishes one toolchain per distribution release, and the official
/// binaries link against that distribution's libraries. Mapping the running
/// system (including common derivatives through `ID_LIKE`) is what makes a
/// managed install reliable instead of a shot in the dark.
fn swift_platform(
    id: &str,
    id_like: &str,
    version: &str,
    arch: &str,
) -> Option<(&'static str, &'static str)> {
    if arch != "x86_64" && arch != "aarch64" {
        return None;
    }
    let candidates = std::iter::once(id).chain(id_like.split_whitespace());
    for candidate in candidates {
        let mapped = match (candidate, version) {
            ("ubuntu", "22.04") => ("ubuntu2204", "ubuntu22.04"),
            ("ubuntu", "24.04") => ("ubuntu2404", "ubuntu24.04"),
            ("ubuntu", "26.04") => ("ubuntu2604", "ubuntu26.04"),
            ("debian", "12") => ("debian12", "debian12"),
            ("debian", "13") => ("debian13", "debian13"),
            ("fedora", "39") => ("fedora39", "fedora39"),
            ("fedora", "41") => ("fedora41", "fedora41"),
            ("amzn", "2") => ("amazonlinux2", "amazonlinux2"),
            ("amzn", "2023") => ("amazonlinux2023", "amazonlinux2023"),
            _ => continue,
        };
        return Some(mapped);
    }
    None
}

/// The swift.org platform tag for a distribution it builds for directly.
fn swift_native_asset() -> Option<(&'static str, &'static str)> {
    if std::env::consts::OS != "linux" {
        return None;
    }
    let (id, id_like, version) = os_release()?;
    swift_platform(&id, &id_like, &version, std::env::consts::ARCH)
}

/// The Swift toolchain Kemori should provision on this host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SwiftToolchain {
    /// The swift.org release directory (`ubuntu2404`, `ubi10`, …).
    tag: &'static str,
    /// The artifact suffix (`ubuntu24.04`, `ubi10`, …).
    suffix: &'static str,
    /// Whether this is the build for the running distribution (no compat needed
    /// beyond what the distribution provides).
    native: bool,
}

/// Resolve the Swift toolchain for this host.
///
/// A distribution swift.org builds for directly gets its own artifact. Any
/// other glibc Linux system gets the portable UBI10 build plus Kemori's
/// compatibility layer (a ncurses soname alias and the distribution's
/// `libxml2.so.2`), which is how the maintained Arch package makes the same
/// toolchain run. musl systems and non-Linux hosts get nothing.
fn swift_toolchain() -> Option<SwiftToolchain> {
    if let Some((tag, suffix)) = swift_native_asset() {
        return Some(SwiftToolchain {
            tag,
            suffix,
            native: true,
        });
    }
    if std::env::consts::OS != "linux" || libc_is_musl() {
        return None;
    }
    let (tag, suffix) = match std::env::consts::ARCH {
        "x86_64" => ("ubi10", "ubi10"),
        "aarch64" => ("ubi10-aarch64", "ubi10-aarch64"),
        _ => return None,
    };
    Some(SwiftToolchain {
        tag,
        suffix,
        native: false,
    })
}

/// Libraries the Swift toolchain needs that this system does not provide.
///
/// `libncurses.so.6` is satisfied by the wide `libncursesw.so.6` (the same
/// library; the soname differs because Kemori's build is not the system's).
/// `libxml2.so.2` is a genuine ABI version some rolling distributions moved past
/// and provide through a compatibility package.
fn swift_missing_libs() -> Vec<&'static str> {
    let mut missing = Vec::new();
    if system_library(&["libncurses.so.6", "libncursesw.so.6"]).is_none() {
        missing.push("libncurses.so.6");
    }
    if system_library(&["libxml2.so.2"]).is_none() {
        missing.push("libxml2.so.2");
    }
    missing
}

/// Whether the running distribution is Arch or an Arch derivative.
fn distro_is_arch() -> bool {
    match os_release() {
        Some((id, id_like, _)) => {
            id == "arch" || id_like.split_whitespace().any(|like| like == "arch")
        }
        None => false,
    }
}

/// Find a system shared library by soname in the usual library directories.
fn system_library(names: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/lib"),
        PathBuf::from("/lib64"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/local/lib64"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        PathBuf::from("/usr/lib/aarch64-linux-gnu"),
    ];
    // A user-writable location lets a rootless user supply a compatibility
    // library it cannot install system-wide.
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/lib"));
        dirs.push(PathBuf::from(home).join(".local/lib64"));
    }
    dirs.iter().find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// The `builds.hex.pm` platform directory for this host, if `bob` publishes one.
fn bob_platform() -> Option<&'static str> {
    if std::env::consts::OS != "linux" {
        return None;
    }
    let Some((id, id_like, version)) = os_release() else {
        return bob_fallback_platform();
    };
    bob_platform_for(&id, &id_like, &version).or_else(bob_fallback_platform)
}

/// A best-effort `bob` platform for glibc distributions it does not name.
///
/// `bob`'s Ubuntu 24.04 builds link against a modern glibc and OpenSSL 3, which
/// current rolling distributions ship, so they run on many systems that have no
/// explicit build. Kemori still verifies the result by launching the real server,
/// so an incompatible system fails with a clear message instead of a false
/// success. musl systems (Alpine) are excluded because the glibc build cannot
/// run there at all.
fn bob_fallback_platform() -> Option<&'static str> {
    if libc_is_musl() {
        return None;
    }
    Some("ubuntu-24.04")
}

/// Map a distribution to the `bob` build directory (see [`bob_platform`]).
fn bob_platform_for(id: &str, id_like: &str, version: &str) -> Option<&'static str> {
    let candidates = std::iter::once(id).chain(id_like.split_whitespace());
    for candidate in candidates {
        let mapped = match (candidate, version) {
            ("ubuntu", "20.04") => "ubuntu-20.04",
            ("ubuntu", "22.04") => "ubuntu-22.04",
            ("ubuntu", "24.04") => "ubuntu-24.04",
            ("debian", "11") => "debian-11",
            ("debian", "12") => "debian-12",
            ("debian", "13") => "debian-13",
            ("fedora", "39") => "fedora-39",
            ("fedora", "41") => "fedora-41",
            _ => continue,
        };
        return Some(mapped);
    }
    None
}

/// The major version of an OTP release (`27.3.4` → `27`).
fn otp_major(version: &str) -> &str {
    version.split('.').next().unwrap_or(version)
}

/// Download and verify an Erlang/OTP or Elixir build from `builds.hex.pm`.
///
/// The service publishes the SHA-256 next to the build in its `builds.txt`, so
/// the checksum is read for the pinned version and verified before anything is
/// extracted. The archive still has to be extracted by a later step.
fn bob_build(package: BobPackage, dest: &Path) -> Result<(), String> {
    let platform = bob_platform().ok_or_else(|| {
        "no Erlang/Elixir build is published for this platform; install Erlang and Elixir manually"
            .to_string()
    })?;
    let (key, file, sums_url, base) = match package {
        BobPackage::Erlang => (
            format!("OTP-{OTP_VERSION}"),
            format!("OTP-{OTP_VERSION}.tar.gz"),
            format!("https://builds.hex.pm/builds/otp/{platform}/builds.txt"),
            format!("https://builds.hex.pm/builds/otp/{platform}"),
        ),
        BobPackage::Elixir => {
            let version = format!("v{ELIXIR_VERSION}-otp-{}", otp_major(OTP_VERSION));
            (
                version.clone(),
                format!("{version}.zip"),
                "https://builds.hex.pm/builds/elixir/builds.txt".to_string(),
                "https://builds.hex.pm/builds/elixir".to_string(),
            )
        }
    };
    let body = curl_text(&sums_url)?;
    let expected = builds_txt_checksum(&body, &key)
        .ok_or_else(|| format!("builds.hex.pm published no checksum for {key}"))?;
    // Fail closed: an unverified runtime is never extracted or run.
    download(&format!("{base}/{file}"), dest, Some(&expected))
}

/// Find the SHA-256 for `key` in a `builds.hex.pm` `builds.txt` body.
///
/// The body is one build per line, `<name> <git-sha> <timestamp> <sha256>`;
/// older entries omit the checksum column and are treated as unavailable.
fn builds_txt_checksum(body: &str, key: &str) -> Option<String> {
    body.lines()
        .find(|line| line.split_whitespace().next() == Some(key))
        .and_then(|line| line.split_whitespace().nth(3))
        .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string)
}
