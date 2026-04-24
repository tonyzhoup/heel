mod policy;

use std::future::Future;
use std::process::Output;

use crate::config::SandboxConfigData;
use crate::error::{Error, Result};
use crate::platform::{Backend, Child};
use crate::stdio::StdioConfig;

pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl Backend for WindowsBackend {
    fn execute(
        &self,
        config: &SandboxConfigData,
        _proxy_port: u16,
        _program: &str,
        _args: &[String],
        _envs: &[(String, String)],
        _current_dir: Option<&std::path::Path>,
        _stdin: StdioConfig,
        _stdout: StdioConfig,
        _stderr: StdioConfig,
    ) -> impl Future<Output = Result<Output>> + Send {
        async {
            policy::validate_config_supported(config)?;
            Err(Error::UnsupportedPlatform)
        }
    }

    fn spawn(
        &self,
        config: &SandboxConfigData,
        _proxy_port: u16,
        _program: &str,
        _args: &[String],
        _envs: &[(String, String)],
        _current_dir: Option<&std::path::Path>,
        _stdin: StdioConfig,
        _stdout: StdioConfig,
        _stderr: StdioConfig,
    ) -> impl Future<Output = Result<Child>> + Send {
        async {
            policy::validate_config_supported(config)?;
            Err(Error::UnsupportedPlatform)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsBackend;
    use crate::config::SandboxConfig;
    use crate::network::AllowAll;
    use crate::platform::Backend;
    use crate::stdio::StdioConfig;

    #[test]
    fn windows_backend_constructs() {
        let _backend = WindowsBackend::new().expect("backend");
    }

    #[test]
    fn execute_runs_policy_validation_before_unsupported_platform() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .network(AllowAll)
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");

            let err = backend
                .execute(
                    &config,
                    0,
                    "cmd.exe",
                    &[],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Null,
                    StdioConfig::Null,
                )
                .await
                .expect_err("policy validation should reject before process launch");

            assert!(err.to_string().contains("windows-appcontainer-network"));
        });
    }

    #[test]
    fn spawn_runs_policy_validation_before_unsupported_platform() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .network(AllowAll)
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");

            let result = backend
                .spawn(
                    &config,
                    0,
                    "cmd.exe",
                    &[],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Null,
                    StdioConfig::Null,
                )
                .await;

            match result {
                Ok(_) => panic!("policy validation should reject before process launch"),
                Err(err) => assert!(err.to_string().contains("windows-appcontainer-network")),
            }
        });
    }
}
