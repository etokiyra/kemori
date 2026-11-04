/// The result of probing every known tool.
#[derive(Clone, Debug)]
pub struct ToolRegistry {
    statuses: Vec<ToolStatus>,
}

impl ToolRegistry {
    /// Probe every known tool. Runs a handful of short-lived processes and is
    /// meant for the background worker.
    pub fn discover() -> Self {
        ToolRegistry {
            statuses: Tool::ALL.iter().map(|&tool| probe(tool)).collect(),
        }
    }

    /// Discovery for the background worker.
    ///
    /// In tests every `App` would otherwise re-probe every tool, and a parallel
    /// test binary saturates the machine spawning thousands of `--version`
    /// processes at once. The environment does not change during a test run, so
    /// the first result is shared. Production always probes fresh, preserving
    /// Kemori's ability to notice a tool installed while it is running.
    pub fn discover_cached() -> Self {
        #[cfg(test)]
        {
            use std::sync::{Mutex, OnceLock};
            static CACHE: OnceLock<Mutex<Option<ToolRegistry>>> = OnceLock::new();
            let cache = CACHE.get_or_init(|| Mutex::new(None));
            let mut guard = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(registry) = guard.as_ref() {
                return registry.clone();
            }
            let registry = Self::discover();
            *guard = Some(registry.clone());
            registry
        }
        #[cfg(not(test))]
        {
            Self::discover()
        }
    }

    pub fn all(&self) -> &[ToolStatus] {
        &self.statuses
    }

    pub fn status(&self, tool: Tool) -> Option<&ToolStatus> {
        self.statuses.iter().find(|status| status.tool == tool)
    }

    pub fn available(&self, tool: Tool) -> bool {
        self.status(tool).is_some_and(|status| status.available)
    }

    /// The resolved executable path for `tool`, if it was found.
    pub fn program_path(&self, tool: Tool) -> Option<&Path> {
        self.status(tool).and_then(|status| status.path.as_deref())
    }

    /// Whether any language server for `language` is available.
    pub fn has_language_server(&self, language: LanguageId) -> bool {
        self.statuses.iter().any(|status| {
            status.tool.language() == language
                && status.tool.purpose() == ToolPurpose::LanguageServer
                && status.available
        })
    }
}

fn probe(tool: Tool) -> ToolStatus {
    // `java-debug` is a JDT LS extension jar, not an executable; its presence is
    // the useful check, while jdtls itself is the prerequisite above it.
    if tool == Tool::JavaDebug {
        return match java_debug_jar().filter(|path| path.is_file()) {
            Some(path) => ToolStatus {
                tool,
                available: true,
                version: Some(JAVA_DEBUG_VERSION.to_string()),
                path: Some(path),
                error: None,
            },
            None => ToolStatus {
                tool,
                available: false,
                version: None,
                path: None,
                error: None,
            },
        };
    }
    let Some(path) = locate_tool(tool) else {
        return ToolStatus {
            tool,
            available: false,
            version: None,
            path: None,
            error: None,
        };
    };
    let mut command = Command::new(&path);
    // Managed runtimes (the JDK for jdtls, the .NET SDK for OmniSharp) are
    // found through the launch environment.
    for (key, value) in launch_env(tool) {
        command.env(key, value);
    }
    if tool.probe_as_server() {
        return probe_server(tool, path, command);
    }
    command.args(tool.version_args());
    // A probe is bounded in both time and output so a hung or chatty tool
    // cannot wedge the worker. `stdin` is null, so `gofmt` reads an empty
    // document and exits.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return ToolStatus {
            tool,
            available: false,
            version: None,
            path: Some(path),
            error: None,
        };
    };
    match wait_captured(&mut child, PROBE_TIMEOUT, MAX_TOOL_OUTPUT) {
        Ok(captured) if captured.status.is_some_and(|status| status.success()) => {
            // Only keep a version line that actually looks like one; `--help`
            // usage output should not masquerade as a version.
            let version = String::from_utf8_lossy(&captured.stdout)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .filter(|line| line.chars().any(|c| c.is_ascii_digit()))
                .map(str::to_string);
            ToolStatus {
                tool,
                available: true,
                version,
                path: Some(path),
                error: None,
            }
        }
        Ok(captured) => ToolStatus {
            tool,
            available: false,
            version: None,
            path: Some(path),
            error: probe_error(&captured.stderr),
        },
        Err(_) => ToolStatus {
            tool,
            available: false,
            version: None,
            path: Some(path),
            error: None,
        },
    }
}
