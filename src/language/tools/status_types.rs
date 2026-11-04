
/// What Kemori learned about one tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolStatus {
    pub tool: Tool,
    /// Whether the program exists and ran successfully.
    pub available: bool,
    /// The first line of `--version`, when there was one.
    pub version: Option<String>,
    /// The resolved executable path, when one was found. Tools installed in a
    /// user bin directory may not be on the process `PATH`.
    pub path: Option<PathBuf>,
    /// Why a present-but-unusable tool failed to run (a missing shared library,
    /// a broken launcher), so install failures can explain themselves.
    pub error: Option<String>,
}

impl ToolStatus {
    /// A one-line summary for the setup view.
    pub fn summary(&self) -> String {
        if self.available {
            match &self.version {
                Some(version) => version.clone(),
                None => "installed".to_string(),
            }
        } else {
            self.tool.install_hint().to_string()
        }
    }

    /// Whether Kemori owns this install (it lives under Kemori's tools directory).
    ///
    /// Only a managed tool may be updated or removed by Kemori; a user's own
    /// `rust-analyzer`, `clangd` or `gem` install is never touched.
    pub fn is_managed(&self) -> bool {
        self.path.as_deref().is_some_and(is_managed_path)
    }

    /// The on-disk size of the Kemori-managed component, when Kemori owns the
    /// install. User/system installs return `None`.
    pub fn managed_size(&self) -> Option<u64> {
        let dir = managed_component(self.path.as_deref()?)?;
        directory_size(&dir)
    }

    /// The managed component directory Kemori would remove for this tool.
    pub fn managed_dir(&self) -> Option<PathBuf> {
        managed_component(self.path.as_deref()?)
    }
}

/// Whether `path` was installed by Kemori under its own tools directory.
pub fn is_managed_path(path: &Path) -> bool {
    tools_dir().is_some_and(|tools| path.starts_with(tools))
}

/// The Kemori-managed component directory containing `path`: the first path
/// segment under the tools directory.
pub fn managed_component(path: &Path) -> Option<PathBuf> {
    let tools = tools_dir()?;
    let relative = path.strip_prefix(&tools).ok()?;
    let first = relative.components().next()?;
    Some(tools.join(first))
}

/// The total size of the files under `dir`, bounded so a large tree cannot stall
/// the worker. Returns `None` only when `dir` cannot be walked at all.
pub fn directory_size(dir: &Path) -> Option<u64> {
    // A cap on visited entries keeps a pathological tree predictable; a managed
    // component is never expected to approach it.
    const MAX_ENTRIES: usize = 50_000;
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return Some(total);
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Some(total)
}
