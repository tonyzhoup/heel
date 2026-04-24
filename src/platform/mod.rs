use std::future::Future;
use std::process::{ExitStatus, Output};

use blocking::unblock;

use crate::config::SandboxConfigData;
use crate::error::Result;
use crate::sandbox::ProcessTracker;
use crate::stdio::StdioConfig;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(any(test, target_os = "windows"))]
pub mod windows;

/// A spawned child process in the sandbox
pub struct Child {
    inner: Option<std::process::Child>,
    tracker: Option<ProcessTracker>,
    pid: u32,
}

impl Child {
    pub(crate) fn new(inner: std::process::Child) -> Self {
        let pid = inner.id();
        Self {
            inner: Some(inner),
            tracker: None,
            pid,
        }
    }

    pub(crate) fn with_tracker(mut self, tracker: ProcessTracker) -> Self {
        self.tracker = Some(tracker);
        self
    }

    fn unregister_if_tracked(&mut self) {
        if let Some(tracker) = self.tracker.take() {
            tracker.unregister(self.id());
        }
    }

    /// Access the child's stdin
    pub fn stdin(&mut self) -> Option<&mut std::process::ChildStdin> {
        self.inner.as_mut().and_then(|child| child.stdin.as_mut())
    }

    /// Access the child's stdout
    pub fn stdout(&mut self) -> Option<&mut std::process::ChildStdout> {
        self.inner.as_mut().and_then(|child| child.stdout.as_mut())
    }

    /// Access the child's stderr
    pub fn stderr(&mut self) -> Option<&mut std::process::ChildStderr> {
        self.inner.as_mut().and_then(|child| child.stderr.as_mut())
    }

    /// Take ownership of the child's stdin
    pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.inner.as_mut().and_then(|child| child.stdin.take())
    }

    /// Take ownership of the child's stdout
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.inner.as_mut().and_then(|child| child.stdout.take())
    }

    /// Take ownership of the child's stderr
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.inner.as_mut().and_then(|child| child.stderr.take())
    }

    /// Get the process ID
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Wait for the child to exit
    pub async fn wait(&mut self) -> Result<ExitStatus> {
        let pid = self.pid;
        let tracker = self.tracker.take();
        let mut inner = self
            .inner
            .take()
            .expect("child process no longer available");
        let (inner, status) = unblock(move || {
            let status = inner.wait();
            (inner, status)
        })
        .await;
        self.inner = Some(inner);
        if let Some(tracker) = tracker {
            tracker.unregister(pid);
        }
        Ok(status?)
    }

    /// Wait for the child to exit and collect all output
    pub async fn wait_with_output(self) -> Result<Output> {
        let inner = self.inner.expect("child process no longer available");
        let pid = inner.id();
        let tracker = self.tracker;
        let output = unblock(move || inner.wait_with_output()).await?;
        if let Some(tracker) = tracker {
            tracker.unregister(pid);
        }
        Ok(output)
    }

    /// Attempt to kill the child process
    pub fn kill(&mut self) -> Result<()> {
        let inner = self
            .inner
            .as_mut()
            .expect("child process no longer available");
        Ok(inner.kill()?)
    }

    /// Check if the child has exited without blocking
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        let inner = self
            .inner
            .as_mut()
            .expect("child process no longer available");
        let status = inner.try_wait()?;
        if status.is_some() {
            self.unregister_if_tracked();
        }
        Ok(status)
    }
}

/// Static sandbox capability metadata for the current target platform.
///
/// This describes which backend features this crate build implements for the
/// target OS. Runtime initialization can still fail because of OS version,
/// kernel, entitlement, or host policy requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlatformCapabilities {
    /// Diagnostic backend identifier.
    ///
    /// This string is intended for logs, telemetry, and user-facing status. Do
    /// not branch on it for capability decisions; use the typed boolean fields.
    pub backend: &'static str,
    /// Whether sandboxed command execution is implemented for this target.
    ///
    /// If this is false, `Sandbox::command(...).output()`, `status()`, and
    /// `spawn()` are expected to fail closed rather than run natively.
    pub execution_supported: bool,
    /// Whether the backend supports strict filesystem root enforcement.
    pub filesystem_strict: bool,
    /// Whether the backend can enforce a deny-all network policy.
    pub network_deny_all: bool,
    /// Whether the backend can enforce network allowlists through Heel policy.
    pub network_allowlist: bool,
    /// Whether Heel IPC is supported from sandboxed children on this platform.
    pub ipc: bool,
    /// Whether child process tree cleanup is implemented by the platform backend.
    pub background_process_tree_cleanup: bool,
}

#[cfg(target_os = "macos")]
pub fn platform_capabilities() -> PlatformCapabilities {
    macos_capabilities()
}

#[cfg(any(test, target_os = "macos"))]
fn macos_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "macos_sandbox_exec",
        execution_supported: true,
        filesystem_strict: false,
        network_deny_all: true,
        network_allowlist: true,
        ipc: true,
        background_process_tree_cleanup: false,
    }
}

#[cfg(target_os = "linux")]
pub fn platform_capabilities() -> PlatformCapabilities {
    linux_capabilities()
}

#[cfg(any(test, target_os = "linux"))]
fn linux_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "linux_landlock_seccomp",
        execution_supported: true,
        filesystem_strict: true,
        network_deny_all: true,
        network_allowlist: true,
        ipc: true,
        background_process_tree_cleanup: false,
    }
}

#[cfg(target_os = "windows")]
pub fn platform_capabilities() -> PlatformCapabilities {
    windows_capabilities()
}

#[cfg(any(test, target_os = "windows"))]
fn windows_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "windows_appcontainer",
        execution_supported: false,
        filesystem_strict: false,
        network_deny_all: false,
        network_allowlist: false,
        ipc: false,
        background_process_tree_cleanup: false,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "unsupported",
        execution_supported: false,
        filesystem_strict: false,
        network_deny_all: false,
        network_allowlist: false,
        ipc: false,
        background_process_tree_cleanup: false,
    }
}

/// Internal trait for platform-specific sandbox backends
pub(crate) trait Backend: Sized + Send + Sync {
    /// Execute a command and wait for completion
    #[allow(clippy::too_many_arguments)]
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
    ) -> impl Future<Output = Result<Output>> + Send;

    /// Spawn a command as a child process
    #[allow(clippy::too_many_arguments)]
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
    ) -> impl Future<Output = Result<Child>> + Send;
}

/// Create the native backend for the current platform
#[cfg(target_os = "macos")]
pub(crate) fn create_native_backend() -> Result<macos::MacOSBackend> {
    macos::MacOSBackend::new()
}

#[cfg(target_os = "linux")]
pub(crate) fn create_native_backend() -> Result<linux::LinuxBackend> {
    linux::LinuxBackend::new()
}

#[cfg(target_os = "windows")]
pub(crate) fn create_native_backend() -> Result<windows::WindowsBackend> {
    windows::WindowsBackend::new()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn create_native_backend() -> Result<()> {
    Err(crate::error::Error::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::platform_capabilities;

    #[test]
    fn platform_capabilities_report_backend_name() {
        let capabilities = platform_capabilities();

        #[cfg(target_os = "macos")]
        assert_eq!(capabilities.backend, "macos_sandbox_exec");

        #[cfg(target_os = "linux")]
        assert_eq!(capabilities.backend, "linux_landlock_seccomp");

        #[cfg(target_os = "windows")]
        assert_eq!(capabilities.backend, "windows_appcontainer");
    }

    #[test]
    fn windows_first_release_capability_contract_is_explicit() {
        let capabilities = platform_capabilities();

        if cfg!(target_os = "windows") {
            assert!(!capabilities.execution_supported);
            assert!(!capabilities.filesystem_strict);
            assert!(!capabilities.network_deny_all);
            assert!(!capabilities.network_allowlist);
            assert!(!capabilities.ipc);
            assert!(!capabilities.background_process_tree_cleanup);
        }
    }

    #[test]
    fn windows_stub_capability_contract_is_fail_closed() {
        let capabilities = super::windows_capabilities();

        assert_eq!(capabilities.backend, "windows_appcontainer");
        assert!(!capabilities.execution_supported);
        assert!(!capabilities.filesystem_strict);
        assert!(!capabilities.network_deny_all);
        assert!(!capabilities.network_allowlist);
        assert!(!capabilities.ipc);
        assert!(!capabilities.background_process_tree_cleanup);
    }

    #[test]
    fn macos_capability_contract_is_explicit() {
        let capabilities = super::macos_capabilities();

        assert_eq!(capabilities.backend, "macos_sandbox_exec");
        assert!(capabilities.execution_supported);
        assert!(!capabilities.filesystem_strict);
        assert!(capabilities.network_deny_all);
        assert!(capabilities.network_allowlist);
        assert!(capabilities.ipc);
        assert!(!capabilities.background_process_tree_cleanup);
    }

    #[test]
    fn linux_capability_contract_is_explicit() {
        let capabilities = super::linux_capabilities();

        assert_eq!(capabilities.backend, "linux_landlock_seccomp");
        assert!(capabilities.execution_supported);
        assert!(capabilities.filesystem_strict);
        assert!(capabilities.network_deny_all);
        assert!(capabilities.network_allowlist);
        assert!(capabilities.ipc);
        assert!(!capabilities.background_process_tree_cleanup);
    }

    #[test]
    fn active_platform_capability_contract_is_explicit() {
        let capabilities = platform_capabilities();

        #[cfg(target_os = "macos")]
        assert_eq!(
            capabilities,
            super::PlatformCapabilities {
                backend: "macos_sandbox_exec",
                execution_supported: true,
                filesystem_strict: false,
                network_deny_all: true,
                network_allowlist: true,
                ipc: true,
                background_process_tree_cleanup: false,
            }
        );

        #[cfg(target_os = "linux")]
        assert_eq!(
            capabilities,
            super::PlatformCapabilities {
                backend: "linux_landlock_seccomp",
                execution_supported: true,
                filesystem_strict: true,
                network_deny_all: true,
                network_allowlist: true,
                ipc: true,
                background_process_tree_cleanup: false,
            }
        );
    }
}
