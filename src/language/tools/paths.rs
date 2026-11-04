/// Kemori's own directory for tools it installs, kept under the user's data
/// directory so no elevated permissions are ever required.
pub fn tools_dir() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(data).join("kemori/tools"));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share/kemori/tools"))
}

/// The npm prefix Kemori installs global packages into. It is user-writable, so
/// `npm install -g --prefix` never hits a permission error.
pub fn npm_prefix() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("npm"))
}

/// Kemori's managed Python virtualenv, used to install the Python language
/// server even when the system Python has no `pip`.
pub fn venv_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("python"))
}

/// Kemori's managed Node.js runtime, used to run `npm` and the npm-based language
/// servers without requiring the user to install Node.js themselves.
pub fn node_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("node"))
}

/// The extracted `js-debug` DAP-only bundle.
pub fn js_debug_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("js-debug"))
}

/// The `js-debug-dap` launcher Kemori writes into its own bin directory.
pub fn js_debug_launcher() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("bin/js-debug-dap"))
}

/// The extracted `netcoredbg` C# debug adapter.
pub fn netcoredbg_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("netcoredbg"))
}

/// The extracted `vscode-php-debug` DAP adapter bundle.
pub fn php_debug_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("php-debug"))
}

/// The `php-debug-dap` launcher Kemori writes into its own bin directory.
pub fn php_debug_launcher() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("bin/php-debug-dap"))
}

/// The pinned `java-debug` plugin jar loaded into JDT LS.
pub fn java_debug_jar() -> Option<PathBuf> {
    tools_dir().map(|dir| {
        dir.join(format!(
            "java-debug/com.microsoft.java.debug.plugin-{JAVA_DEBUG_VERSION}.jar"
        ))
    })
}

/// The `bin` directory of Kemori's managed Node.js runtime (the runtime root on
/// Windows, where `node.exe`/`npm.cmd` sit directly in the extracted folder).
fn node_bin_dir() -> Option<PathBuf> {
    let dir = node_dir()?;
    if cfg!(windows) {
        Some(dir)
    } else {
        Some(dir.join("bin"))
    }
}

/// The `npm` bundled with Kemori's managed Node.js, whether or not it exists yet.
fn managed_npm() -> Option<PathBuf> {
    let dir = node_dir()?;
    Some(if cfg!(windows) {
        dir.join("npm.cmd")
    } else {
        dir.join("bin/npm")
    })
}

/// The scratch path for the managed Node.js archive.
fn node_archive() -> Option<PathBuf> {
    let dir = downloads_dir()?;
    let asset = node_asset()?;
    Some(dir.join(asset))
}

/// Kemori's managed .NET install directory (the SDK OmniSharp runs on).
pub fn dotnet_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("dotnet"))
}

/// Kemori's managed JDK directory, used to run `jdtls` without touching the
/// user's system Java.
pub fn jdk_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("jdk"))
}

/// Kemori's managed OmniSharp directory.
pub fn omnisharp_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("omnisharp"))
}

/// Kemori's managed `jdtls` directory.
pub fn jdtls_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("jdtls"))
}

/// Kemori's managed `lua-language-server` directory.
pub fn lua_ls_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("lua-language-server"))
}

/// Kemori's dedicated JDK 21, used only to run `kotlin-language-server`. It is
/// kept separate from the JDK 25 that `jdtls` requires, because the Kotlin
/// compiler bundled with the server rejects the newer version string.
pub fn kotlin_jdk_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("kotlin-jdk"))
}

/// Kemori's managed `kotlin-language-server` directory.
pub fn kotlin_ls_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("kotlin-language-server"))
}

/// Kemori's managed Dart SDK directory.
pub fn dart_sdk_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("dart-sdk"))
}

/// Kemori's isolated Perl `local::lib`, used to install `Perl::LanguageServer`
/// without touching the system Perl.
pub fn perl_local_lib_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("perl5"))
}

/// Kemori's managed Swift toolchain directory. The archive extracts a `usr/`
/// subtree, so the launcher is `tools/swift/usr/bin/sourcekit-lsp`.
pub fn swift_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("swift"))
}

/// Kemori's Swift compatibility directory: links to the system libraries the
/// official toolchain needs but does not ship, used through a scoped
/// `LD_LIBRARY_PATH` without modifying the system.
pub fn swift_compat_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("swift-compat"))
}

/// Kemori's managed Erlang/OTP installation directory.
pub fn otp_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("otp"))
}

/// Kemori's managed Elixir installation directory.
pub fn elixir_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("elixir"))
}

/// Kemori's managed ElixirLS installation directory (the release launcher).
pub fn elixir_ls_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("elixir-ls"))
}

/// Kemori's private GPG keyring, used only to verify downloaded release
/// signatures. Keeping it separate means the user's `~/.gnupg` is untouched and
/// a stale key in it can never be trusted by mistake.
fn gnupg_home() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("gnupg"))
}

/// Kemori's private Mix/Hex home for the managed Elixir runtime and ElixirLS.
fn mix_home() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("mix"))
}

/// Where Kemori keeps the checksum-verified `cpanm` it bootstraps.
fn cpanm_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("cpanm"))
}

/// Kemori's managed Go toolchain (`go/bin/go`), kept isolated from `/usr/local/go`.
pub fn go_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("go"))
}

/// Kemori's private `GOPATH`, so `go install` never writes to the user's `~/go`.
pub fn go_path() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("gopath"))
}

/// The `GOBIN` inside Kemori's private `GOPATH`, where `gopls`/`sqls`/`shfmt`
/// are installed.
fn go_bin_dir() -> Option<PathBuf> {
    go_path().map(|dir| dir.join("bin"))
}

/// Kemori's managed `clangd` directory (`bin/clangd` plus its resource headers).
pub fn clangd_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("clangd"))
}

/// Kemori's managed prebuilt `asm-lsp` directory.
pub fn asm_lsp_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("asm-lsp"))
}

/// Kemori's managed `phpactor.phar` directory.
pub fn phpactor_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("phpactor"))
}

/// Kemori's isolated Ruby gem home, so `solargraph` never pollutes the user's.
pub fn gem_home() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("gems"))
}

/// Scratch space for downloaded archives.
fn downloads_dir() -> Option<PathBuf> {
    tools_dir().map(|dir| dir.join("downloads"))
}
