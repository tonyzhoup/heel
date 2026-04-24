mod acl;
mod paths;
mod policy;
mod process;
mod profile;

#[cfg(target_os = "windows")]
pub(crate) use process::AppContainerLaunchState;

use std::future::Future;
use std::path::Path;
use std::process::Output;

use crate::config::SandboxConfigData;
use crate::error::Result;
use crate::platform::{Backend, Child};
use crate::stdio::StdioConfig;

pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

const HEEL_APP_ID: &str = "heel";

pub(crate) fn appcontainer_profile_name(
    config: &SandboxConfigData,
) -> Result<profile::ProfileName> {
    profile::profile_name(HEEL_APP_ID, &config.working_dir().to_string_lossy())
}

#[allow(clippy::too_many_arguments)]
fn launch_supported_config(
    config: &SandboxConfigData,
    proxy_port: u16,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    current_dir: Option<&Path>,
    stdin: StdioConfig,
    stdout: StdioConfig,
    stderr: StdioConfig,
) -> Result<Child> {
    let grants = policy::validate_config_supported(config)?;
    let profile_name = appcontainer_profile_name(config)?;
    let current_dir = current_dir.unwrap_or(config.working_dir());

    #[cfg(target_os = "windows")]
    {
        let base_envs =
            process::allowed_environment_from(std::env::vars(), config.env_passthrough())?;
        let profile = profile::AppContainerProfile::create_or_open(profile_name)?;
        let grant_guard = acl::apply_grants_for_appcontainer_sid(&grants, profile.sid())?;
        let launch_state = process::AppContainerLaunchState::new(profile, grant_guard);
        let launch = process::WindowsLaunch {
            program,
            args,
            current_dir,
            base_envs: &base_envs,
            envs,
            stdin,
            stdout,
            stderr,
        };
        let _ = proxy_port;

        process::launch_appcontainer_process(launch, launch_state)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (
            grants,
            profile_name,
            proxy_port,
            program,
            args,
            envs,
            current_dir,
            stdin,
            stdout,
            stderr,
        );

        Err(crate::error::Error::UnsupportedPlatform)
    }
}

impl Backend for WindowsBackend {
    fn execute(
        &self,
        config: &SandboxConfigData,
        proxy_port: u16,
        program: &str,
        args: &[String],
        envs: &[(String, String)],
        current_dir: Option<&std::path::Path>,
        stdin: StdioConfig,
        stdout: StdioConfig,
        stderr: StdioConfig,
    ) -> impl Future<Output = Result<Output>> + Send {
        async move {
            let child = launch_supported_config(
                config,
                proxy_port,
                program,
                args,
                envs,
                current_dir,
                stdin,
                stdout,
                stderr,
            )?;

            child.wait_with_output().await
        }
    }

    fn spawn(
        &self,
        config: &SandboxConfigData,
        proxy_port: u16,
        program: &str,
        args: &[String],
        envs: &[(String, String)],
        current_dir: Option<&std::path::Path>,
        stdin: StdioConfig,
        stdout: StdioConfig,
        stderr: StdioConfig,
    ) -> impl Future<Output = Result<Child>> + Send {
        async move {
            launch_supported_config(
                config,
                proxy_port,
                program,
                args,
                envs,
                current_dir,
                stdin,
                stdout,
                stderr,
            )
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

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn windows_backend_still_fails_closed_until_process_launch_lands() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .working_dir("C:/Heel/session")
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");
            let profile_name = super::appcontainer_profile_name(&config).expect("profile name");

            assert_eq!(
                profile_name,
                super::appcontainer_profile_name(&config).unwrap()
            );
            assert!(profile_name.as_str().starts_with("heel.heel."));

            let result = backend
                .spawn(
                    &config,
                    0,
                    "cmd.exe",
                    &["/C".to_string(), "echo ok".to_string()],
                    &[("HEEL_TEST".to_string(), "1".to_string())],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Piped,
                    StdioConfig::Piped,
                )
                .await;

            match result {
                Ok(_) => panic!("process launch is intentionally fail-closed until Task 10"),
                Err(err) => assert!(matches!(err, crate::error::Error::UnsupportedPlatform)),
            }
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires Windows AppContainer process launch"]
    fn windows_backend_executes_cmd_echo_in_appcontainer() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");

            let output = backend
                .execute(
                    &config,
                    0,
                    "cmd.exe",
                    &["/C".to_string(), "echo heel-windows-ok".to_string()],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Piped,
                    StdioConfig::Piped,
                )
                .await
                .expect("cmd echo should launch inside AppContainer");

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("heel-windows-ok"),
                "stdout should contain marker, got {stdout:?}"
            );
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires Windows AppContainer file boundary enforcement"]
    fn windows_appcontainer_file_boundaries() {
        smol::block_on(async {
            let temp_root = std::env::temp_dir().join(format!(
                "heel-windows-file-boundaries-{}",
                std::process::id()
            ));
            if temp_root.exists() {
                std::fs::remove_dir_all(&temp_root).expect("clean stale temp root");
            }
            std::fs::create_dir(&temp_root).expect("temp root");
            let temp_root_guard = TempDirGuard(temp_root.clone());

            let working = temp_root.join("working");
            let read = temp_root.join("read");
            let write = temp_root.join("write");
            let outside = temp_root.join("outside");
            for dir in [&working, &read, &write, &outside] {
                std::fs::create_dir(dir).expect("test directory");
            }

            let read_file = read.join("allowed.txt");
            let write_file = write.join("created.txt");
            let outside_file = outside.join("secret.txt");
            std::fs::write(&read_file, "heel-read-ok").expect("read fixture");
            std::fs::write(&outside_file, "heel-outside-secret").expect("outside fixture");

            let (_, config) = SandboxConfig::builder()
                .working_dir(&working)
                .readable_path(&read)
                .writable_path(&write)
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");

            let read_output = backend
                .execute(
                    &config,
                    0,
                    "cmd.exe",
                    &[
                        "/C".to_string(),
                        format!("type {}", cmd_quoted_path(&read_file)),
                    ],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Piped,
                    StdioConfig::Piped,
                )
                .await
                .expect("allowed read should launch");
            assert!(
                read_output.status.success(),
                "allowed read failed: stderr={}",
                String::from_utf8_lossy(&read_output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&read_output.stdout).contains("heel-read-ok"),
                "allowed read stdout should contain fixture"
            );

            let write_output = backend
                .execute(
                    &config,
                    0,
                    "cmd.exe",
                    &[
                        "/C".to_string(),
                        format!("echo heel-write-ok>{}", cmd_quoted_path(&write_file)),
                    ],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Piped,
                    StdioConfig::Piped,
                )
                .await
                .expect("allowed write should launch");
            assert!(
                write_output.status.success(),
                "allowed write failed: stderr={}",
                String::from_utf8_lossy(&write_output.stderr)
            );
            assert_eq!(
                std::fs::read_to_string(&write_file)
                    .expect("written file")
                    .trim(),
                "heel-write-ok"
            );

            let outside_output = backend
                .execute(
                    &config,
                    0,
                    "cmd.exe",
                    &[
                        "/C".to_string(),
                        format!("type {}", cmd_quoted_path(&outside_file)),
                    ],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Piped,
                    StdioConfig::Piped,
                )
                .await
                .expect("outside read command should launch");
            assert!(
                !outside_output.status.success(),
                "outside read unexpectedly succeeded: stdout={}",
                String::from_utf8_lossy(&outside_output.stdout)
            );

            drop(temp_root_guard);
        });
    }

    #[cfg(target_os = "windows")]
    fn cmd_quoted_path(path: &std::path::Path) -> String {
        format!("\"{}\"", path.display())
    }

    #[cfg(target_os = "windows")]
    struct TempDirGuard(std::path::PathBuf);

    #[cfg(target_os = "windows")]
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                tracing::warn!(
                    path = %self.0.display(),
                    "failed to remove Windows AppContainer file-boundary temp root: {error}"
                );
            }
        }
    }
}
