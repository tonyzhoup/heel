use std::process::Stdio;

/// Standard I/O configuration for a sandboxed command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioConfig {
    /// Inherit from parent process.
    Inherit,
    /// Create a new pipe.
    Piped,
    /// Redirect to null.
    Null,
}

impl From<StdioConfig> for Stdio {
    fn from(config: StdioConfig) -> Self {
        match config {
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Piped => Stdio::piped(),
            StdioConfig::Null => Stdio::null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StdioConfig;

    #[test]
    fn stdio_config_is_copyable_and_comparable() {
        let config = StdioConfig::Piped;
        let copied = config;

        assert_eq!(config, copied);
    }
}
