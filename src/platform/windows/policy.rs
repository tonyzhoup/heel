use crate::config::SandboxConfigData;
use crate::error::{Error, Result};

pub(crate) fn validate_config_supported(config: &SandboxConfigData) -> Result<()> {
    if !config.filesystem_strict() {
        return Err(Error::UnsupportedPlatformVersion {
            platform: "windows-appcontainer-filesystem",
            minimum: "strict filesystem",
            current: "non-strict filesystem".to_string(),
        });
    }

    if config.writable_file_system() {
        return Err(Error::UnsupportedPlatformVersion {
            platform: "windows-appcontainer-filesystem",
            minimum: "explicit writable roots",
            current: "globally writable filesystem".to_string(),
        });
    }

    if !config.network_deny_all() {
        return Err(Error::UnsupportedPlatformVersion {
            platform: "windows-appcontainer-network",
            minimum: "DenyAll network policy",
            current: "non-DenyAll policy".to_string(),
        });
    }

    if config.ipc_port().is_some() || config.ipc().is_some() {
        return Err(Error::UnsupportedPlatformVersion {
            platform: "windows-appcontainer-ipc",
            minimum: "IPC disabled",
            current: "IPC configured".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::SandboxConfig;
    use crate::network::AllowAll;

    use super::validate_config_supported;

    #[test]
    fn windows_policy_accepts_default_deny_all_without_ipc() {
        let (_, config) = SandboxConfig::builder()
            .build()
            .expect("config")
            .into_parts();

        validate_config_supported(&config).expect("default deny-all should be supported");
    }

    #[test]
    fn windows_policy_rejects_non_deny_all_network() {
        let (_, config) = SandboxConfig::builder()
            .network(AllowAll)
            .build()
            .expect("config")
            .into_parts();

        let err = validate_config_supported(&config).expect_err("AllowAll should be unsupported");
        assert!(err.to_string().contains("windows-appcontainer-network"));
        assert!(err.to_string().contains("DenyAll network policy"));
    }

    #[test]
    fn windows_policy_rejects_ipc() {
        let (_, config) = SandboxConfig::builder()
            .ipc(crate::IpcRouter::new())
            .build()
            .expect("config")
            .into_parts();

        let err = validate_config_supported(&config).expect_err("IPC should be unsupported");
        assert!(err.to_string().contains("windows-appcontainer-ipc"));
        assert!(err.to_string().contains("IPC disabled"));
    }

    #[test]
    fn windows_policy_rejects_non_strict_filesystem() {
        let (_, config) = SandboxConfig::builder()
            .filesystem_strict(false)
            .build()
            .expect("config")
            .into_parts();

        let err =
            validate_config_supported(&config).expect_err("non-strict filesystem is unsupported");
        assert!(err.to_string().contains("windows-appcontainer-filesystem"));
        assert!(err.to_string().contains("strict filesystem"));
    }

    #[test]
    fn windows_policy_rejects_globally_writable_filesystem() {
        let (_, config) = SandboxConfig::builder()
            .writable_file_system(true)
            .build()
            .expect("config")
            .into_parts();

        let err =
            validate_config_supported(&config).expect_err("writable filesystem is unsupported");
        assert!(err.to_string().contains("windows-appcontainer-filesystem"));
        assert!(err.to_string().contains("explicit writable roots"));
    }
}
