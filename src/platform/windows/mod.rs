mod acl;
#[cfg(target_os = "windows")]
pub(crate) mod job;
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
    #[ignore = "requires Windows AppContainer process launch and host process polling"]
    fn windows_job_kills_process_tree() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");
            let marker = unique_marker("heel-job-tree");

            let mut child = backend
                .spawn(
                    &config,
                    0,
                    "cmd.exe",
                    &["/C".to_string(), background_sleep_command(&marker, 120)],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Null,
                    StdioConfig::Null,
                )
                .await
                .expect("cmd should launch sleep descendant in the process tree");

            let root_pid = child.id();
            poll_until("sleep descendant starts", || {
                child_process_exists(root_pid, "powershell.exe", &marker)
            })
            .expect("sleep child should appear before killing root");

            child.kill().expect("job kill should succeed");
            drop(child);

            poll_until("sleep descendant exits after job cleanup", || {
                !command_line_process_exists(&marker)
            })
            .expect("sleep child should exit when the job is terminated");
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires Windows AppContainer process launch and host process polling"]
    fn windows_wait_closes_job_after_root_exits_with_background_descendant() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");
            let marker = unique_marker("heel-wait-tree");

            let mut child = backend
                .spawn(
                    &config,
                    0,
                    "cmd.exe",
                    &[
                        "/C".to_string(),
                        format!(
                            "{} & powershell.exe -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 2\"",
                            background_sleep_command(&marker, 120)
                        ),
                    ],
                    &[],
                    None,
                    StdioConfig::Null,
                    StdioConfig::Null,
                    StdioConfig::Null,
                )
                .await
                .expect("cmd should launch sleep descendant in the process tree");

            let root_pid = child.id();
            poll_until("sleep descendant starts before root exits", || {
                child_process_exists(root_pid, "powershell.exe", &marker)
            })
            .expect("sleep child should appear before waiting for root");

            let status = child.wait().await.expect("root wait should succeed");
            assert!(status.success(), "root command should exit successfully");

            poll_until("sleep descendant exits after wait closes the job", || {
                !command_line_process_exists(&marker)
            })
            .expect("sleep child should exit when wait observes root exit");
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires Windows AppContainer process launch and host process polling"]
    fn windows_output_closes_job_before_joining_piped_background_descendant() {
        smol::block_on(async {
            let (_, config) = SandboxConfig::builder()
                .build()
                .expect("config")
                .into_parts();
            let backend = WindowsBackend::new().expect("backend");
            let marker = unique_marker("heel-output-tree");
            let background_marker = marker.clone();

            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let output = smol::block_on(async {
                    backend
                        .execute(
                            &config,
                            0,
                            "cmd.exe",
                            &[
                                "/C".to_string(),
                                format!(
                                    "{} & echo heel-output-root-done",
                                    background_sleep_command(&background_marker, 120)
                                ),
                            ],
                            &[],
                            None,
                            StdioConfig::Null,
                            StdioConfig::Piped,
                            StdioConfig::Piped,
                        )
                        .await
                });
                sender.send(output).expect("send output result");
            });

            let output = receiver
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("output() should finish after root exits and job closes");
            let output = output.expect("cmd should launch sleep descendant");
            let stdout = String::from_utf8_lossy(&output.stdout);

            assert!(
                stdout.contains("heel-output-root-done"),
                "stdout should contain root marker, got {stdout:?}"
            );
            assert!(
                output.status.success(),
                "root command should exit successfully: stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !command_line_process_exists(&marker),
                "background sleep descendant should be gone after output() closes the job"
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
    fn poll_until(label: &str, mut predicate: impl FnMut() -> bool) -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

        while std::time::Instant::now() < deadline {
            if predicate() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out waiting for {label}"),
        ))
    }

    #[cfg(target_os = "windows")]
    fn child_process_exists(parent_pid: u32, image_name: &str, command_marker: &str) -> bool {
        powershell_process_query(&format!(
            "ParentProcessId = {parent_pid} AND Name = '{image_name}' AND CommandLine LIKE '%{command_marker}%'"
        ))
    }

    #[cfg(target_os = "windows")]
    fn command_line_process_exists(command_marker: &str) -> bool {
        powershell_process_query(&format!("CommandLine LIKE '%{command_marker}%'"))
    }

    #[cfg(target_os = "windows")]
    fn powershell_process_query(filter: &str) -> bool {
        let script = format!(
            "$p = Get-CimInstance Win32_Process -Filter \"{filter}\"; if ($p) {{ exit 0 }} else {{ exit 1 }}"
        );
        let Ok(status) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
        else {
            return false;
        };

        status.success()
    }

    #[cfg(target_os = "windows")]
    fn background_sleep_command(marker: &str, seconds: u32) -> String {
        format!(
            "start \"\" /B powershell.exe -NoProfile -NonInteractive -Command \"$m='{marker}'; Start-Sleep -Seconds {seconds}\""
        )
    }

    #[cfg(target_os = "windows")]
    fn unique_marker(prefix: &str) -> String {
        format!("{prefix}-{}", std::process::id())
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
