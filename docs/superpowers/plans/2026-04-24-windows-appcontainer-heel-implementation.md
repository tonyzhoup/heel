# Windows AppContainer Heel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reliable Windows AppContainer backend foundation to Heel while preserving the platform-neutral `SandboxConfig` and `Sandbox::command` API.

**Architecture:** Keep macOS/Linux behavior unchanged. Add a public capability probe, then build the Windows backend in focused modules for policy validation, profile/SID management, process launching, ACL grants, and Job Object cleanup. The first enforceable Windows release supports explicit filesystem roots and `DenyAll` network; non-`DenyAll` network and IPC fail closed until separate designs land.

**Tech Stack:** Rust 2024, `windows` crate 0.58, Win32 AppContainer APIs, Win32 process/thread attributes, Windows DACLs, Windows Job Objects, existing Heel `Backend` and `SandboxConfigData`.

---

## File Structure

- Modify `src/platform/mod.rs`
  - Add `PlatformCapabilities`.
  - Add `platform_capabilities()`.
  - Keep `Backend` trait stable during the first task.

- Modify `src/lib.rs`
  - Re-export `platform_capabilities` and `PlatformCapabilities`.

- Modify `src/platform/windows.rs`
  - Keep `WindowsBackend` as the public Windows backend module entry.
  - Add submodule declarations for Windows implementation units.
  - Delegate policy checks and process launch to submodules as they land.

- Create `src/platform/windows/policy.rs`
  - Validate Windows-supported `SandboxConfigData` combinations.
  - Return fail-closed errors for unsupported network and IPC.

- Create `src/platform/windows/profile.rs`
  - Generate AppContainer profile names.
  - Create or derive AppContainer SIDs on Windows.

- Create `src/platform/windows/process.rs`
  - Build Windows command lines and environment blocks.
  - Launch AppContainer processes with `STARTUPINFOEXW`.

- Create `src/platform/windows/acl.rs`
  - Map Heel roots to Windows access categories.
  - Apply and restore AppContainer SID grants.

- Create `src/platform/windows/job.rs`
  - Own Job Object creation, assignment, and tree termination.

- Create `src/platform/windows/paths.rs`
  - Canonicalize Windows roots.
  - Reject unsupported path roots.

- Modify `Cargo.toml`
  - Add missing `windows` crate feature flags as each Win32 API is introduced.

---

### Task 1: Add Platform Capability Reporting

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests for public capability values**

Add this test module near the end of `src/platform/mod.rs`:

```rust
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
            assert!(capabilities.filesystem_strict);
            assert!(capabilities.network_deny_all);
            assert!(!capabilities.network_allowlist);
            assert!(!capabilities.ipc);
            assert!(capabilities.background_process_tree_cleanup);
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test platform::tests::platform_capabilities_report_backend_name
```

Expected: FAIL to compile with an unresolved import or missing function for `platform_capabilities`.

- [ ] **Step 3: Add `PlatformCapabilities` and `platform_capabilities()`**

Add this before the `Backend` trait in `src/platform/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub backend: &'static str,
    pub filesystem_strict: bool,
    pub network_deny_all: bool,
    pub network_allowlist: bool,
    pub ipc: bool,
    pub background_process_tree_cleanup: bool,
}

#[cfg(target_os = "macos")]
pub fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "macos_sandbox_exec",
        filesystem_strict: false,
        network_deny_all: true,
        network_allowlist: true,
        ipc: true,
        background_process_tree_cleanup: false,
    }
}

#[cfg(target_os = "linux")]
pub fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "linux_landlock_seccomp",
        filesystem_strict: true,
        network_deny_all: true,
        network_allowlist: true,
        ipc: true,
        background_process_tree_cleanup: false,
    }
}

#[cfg(target_os = "windows")]
pub fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "windows_appcontainer",
        filesystem_strict: true,
        network_deny_all: true,
        network_allowlist: false,
        ipc: false,
        background_process_tree_cleanup: true,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        backend: "unsupported",
        filesystem_strict: false,
        network_deny_all: false,
        network_allowlist: false,
        ipc: false,
        background_process_tree_cleanup: false,
    }
}
```

- [ ] **Step 4: Re-export the capability API**

Change this line in `src/lib.rs`:

```rust
pub use platform::Child;
```

to:

```rust
pub use platform::{platform_capabilities, Child, PlatformCapabilities};
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cargo test platform::tests::platform_capabilities_report_backend_name
cargo test platform::tests::windows_first_release_capability_contract_is_explicit
```

Expected: both tests PASS on the current platform.

- [ ] **Step 6: Commit**

```bash
git add src/platform/mod.rs src/lib.rs
git commit -m "feat: expose platform sandbox capabilities"
```

---

### Task 2: Add Windows Policy Validation Module

**Files:**
- Modify: `src/platform/windows.rs`
- Create: `src/platform/windows/policy.rs`
- Modify: `Cargo.toml` only if compile requires module-related feature changes

- [ ] **Step 1: Write failing Windows policy tests**

Create `src/platform/windows/policy.rs` with this initial test-focused shape:

```rust
use crate::config::SandboxConfigData;
use crate::error::{Error, Result};

pub(crate) fn validate_config_supported(config: &SandboxConfigData) -> Result<()> {
    let _ = config;
    Err(Error::UnsupportedPlatform)
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
        assert!(err.to_string().contains("Windows AppContainer backend only supports DenyAll"));
    }
}
```

Add this line to `src/platform/windows.rs`:

```rust
mod policy;
```

- [ ] **Step 2: Run tests to verify the first test fails**

Run:

```bash
cargo test platform::windows::policy::tests::windows_policy_accepts_default_deny_all_without_ipc
```

Expected: FAIL because `validate_config_supported()` returns `UnsupportedPlatform`.

- [ ] **Step 3: Implement minimal policy validation**

Replace the function body in `src/platform/windows/policy.rs`:

```rust
pub(crate) fn validate_config_supported(config: &SandboxConfigData) -> Result<()> {
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
```

Adjust the second test assertion to check the structured error text:

```rust
assert!(err.to_string().contains("windows-appcontainer-network"));
assert!(err.to_string().contains("DenyAll network policy"));
```

- [ ] **Step 4: Call policy validation from the Windows backend**

At the beginning of `WindowsBackend::execute()` and `WindowsBackend::spawn()` in `src/platform/windows.rs`, rename `_config` to `config` and add:

```rust
policy::validate_config_supported(config)?;
```

Keep returning `Error::UnsupportedPlatform` after validation until process launch lands.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test platform::windows::policy::tests::
cargo test platform::tests::
```

Expected: policy tests compile and pass on Windows. On non-Windows, `platform::windows` tests are not compiled because the module is gated.

- [ ] **Step 6: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/policy.rs
git commit -m "feat: validate windows sandbox policy support"
```

---

### Task 3: Add Windows Profile Name and SID Module

**Files:**
- Modify: `src/platform/windows.rs`
- Create: `src/platform/windows/profile.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add failing tests for deterministic profile names**

Create `src/platform/windows/profile.rs`:

```rust
use crate::error::{Error, Result};

pub(crate) fn profile_name(app_id: &str, seed: &str) -> Result<String> {
    let _ = (app_id, seed);
    Err(Error::ConfigError("profile name not implemented".to_string()))
}

#[cfg(test)]
mod tests {
    use super::profile_name;

    #[test]
    fn profile_name_is_stable_and_sanitized() {
        let first = profile_name("Eureka Desktop", "abc/DEF:123").expect("profile name");
        let second = profile_name("Eureka Desktop", "abc/DEF:123").expect("profile name");

        assert_eq!(first, second);
        assert!(first.starts_with("heel.eureka-desktop."));
        assert!(first.len() <= 64);
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.'));
    }
}
```

Add to `src/platform/windows.rs`:

```rust
mod profile;
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test platform::windows::profile::tests::profile_name_is_stable_and_sanitized
```

Expected: FAIL with `profile name not implemented`.

- [ ] **Step 3: Implement deterministic profile name generation**

Replace `profile.rs` contents:

```rust
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

const PROFILE_PREFIX: &str = "heel";
const PROFILE_MAX_LEN: usize = 64;

pub(crate) fn profile_name(app_id: &str, seed: &str) -> Result<String> {
    let app_slug = slug(app_id)?;
    let mut hasher = Sha256::new();
    hasher.update(app_id.as_bytes());
    hasher.update([0]);
    hasher.update(seed.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let short_hash = &hash[..16];
    let candidate = format!("{PROFILE_PREFIX}.{app_slug}.{short_hash}");

    if candidate.len() <= PROFILE_MAX_LEN {
        Ok(candidate)
    } else {
        let reserved = PROFILE_PREFIX.len() + 1 + 1 + short_hash.len();
        let max_slug = PROFILE_MAX_LEN.saturating_sub(reserved);
        Ok(format!(
            "{PROFILE_PREFIX}.{}.{short_hash}",
            app_slug.chars().take(max_slug).collect::<String>()
        ))
    }
}

fn slug(input: &str) -> Result<String> {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in input.chars().flat_map(char::to_lowercase) {
        let keep = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        if keep {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        return Err(Error::ConfigError(
            "AppContainer profile app id cannot be empty after sanitization".to_string(),
        ));
    }

    Ok(trimmed)
}
```

- [ ] **Step 4: Add Windows-only SID owner skeleton**

Append this Windows-only type to `profile.rs`:

```rust
#[cfg(target_os = "windows")]
pub(crate) struct AppContainerProfile {
    name: String,
}

#[cfg(target_os = "windows")]
impl AppContainerProfile {
    pub(crate) fn create_or_open(name: String) -> Result<Self> {
        Ok(Self { name })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}
```

This is a skeleton. The next task replaces `create_or_open()` with real Win32 calls.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test profile_name_is_stable_and_sanitized
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/profile.rs
git commit -m "feat: add windows appcontainer profile naming"
```

---

### Task 4: Add Windows AppContainer SID Creation

**Files:**
- Modify: `src/platform/windows/profile.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Extend Windows dependency features**

In `Cargo.toml`, ensure the Windows dependency contains:

```toml
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_Security_Isolation",
    "Win32_System_JobObjects",
    "Win32_System_Threading",
] }
```

- [ ] **Step 2: Add a Windows-only ignored integration test**

Append to `profile.rs`:

```rust
#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::{profile_name, AppContainerProfile};

    #[test]
    #[ignore = "creates local AppContainer profile state"]
    fn appcontainer_profile_create_or_open_returns_sid() {
        let name = profile_name("heel-test", "profile-create-or-open").expect("name");
        let profile = AppContainerProfile::create_or_open(name).expect("profile");

        assert!(profile.sid_ptr().is_some());
        assert!(profile.name().starts_with("heel."));
    }
}
```

Expected initial compile failure: `sid_ptr()` is missing.

- [ ] **Step 3: Implement SID storage and cleanup**

Replace the Windows-only skeleton with a real owner type:

```rust
#[cfg(target_os = "windows")]
pub(crate) struct AppContainerProfile {
    name: String,
    sid: windows::Win32::Foundation::PSID,
}

#[cfg(target_os = "windows")]
impl AppContainerProfile {
    pub(crate) fn create_or_open(name: String) -> Result<Self> {
        use std::ptr::null_mut;
        use windows::core::HSTRING;
        use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, PSID};
        use windows::Win32::Security::Isolation::{
            CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
        };

        let display = HSTRING::from(name.as_str());
        let description = HSTRING::from("Heel sandbox profile");
        let mut sid = PSID::default();

        unsafe {
            let created = CreateAppContainerProfile(
                &HSTRING::from(name.as_str()),
                &display,
                &description,
                None,
                0,
                &mut sid,
            );

            if let Err(error) = created {
                if error.code().0 as u32 != ERROR_ALREADY_EXISTS.0 {
                    return Err(Error::FfiError(format!(
                        "CreateAppContainerProfile failed for '{}': {error}",
                        name
                    )));
                }

                DeriveAppContainerSidFromAppContainerName(
                    &HSTRING::from(name.as_str()),
                    &mut sid,
                )
                .map_err(|error| {
                    Error::FfiError(format!(
                        "DeriveAppContainerSidFromAppContainerName failed for '{}': {error}",
                        name
                    ))
                })?;
            }
        }

        if sid == PSID(null_mut()) {
            return Err(Error::FfiError(format!(
                "AppContainer SID for '{}' was null",
                name
            )));
        }

        Ok(Self { name, sid })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn sid_ptr(&self) -> Option<windows::Win32::Foundation::PSID> {
        Some(self.sid)
    }
}

#[cfg(target_os = "windows")]
impl Drop for AppContainerProfile {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::Security::FreeSid(self.sid);
        }
    }
}
```

- [ ] **Step 4: Run Windows target check**

Run:

```bash
cargo check --target x86_64-pc-windows-msvc
```

Expected: compile succeeds for the library or reports missing target/toolchain. If the target is missing, install it with `rustup target add x86_64-pc-windows-msvc` and rerun.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/platform/windows/profile.rs
git commit -m "feat: create windows appcontainer profiles"
```

---

### Task 5: Refactor Child for Platform-Native Handles

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/macos/mod.rs`
- Modify: `src/platform/linux/mod.rs`
- Modify: `src/platform/windows.rs`

- [ ] **Step 1: Write tests for existing child behavior on Unix**

Add to `src/platform/mod.rs` tests:

```rust
#[cfg(unix)]
#[test]
fn child_wraps_std_process_id() {
    let std_child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawn test child");
    let expected_pid = std_child.id();

    let child = super::Child::from_std(std_child);

    assert_eq!(child.id(), expected_pid);
}
```

Expected compile failure: `Child::from_std` is missing.

- [ ] **Step 2: Add internal child representation**

Replace the private fields of `Child` in `src/platform/mod.rs` with:

```rust
pub struct Child {
    inner: ChildInner,
    tracker: Option<ProcessTracker>,
    pid: u32,
}

enum ChildInner {
    Std(Option<std::process::Child>),
}
```

Add:

```rust
impl Child {
    pub(crate) fn from_std(inner: std::process::Child) -> Self {
        let pid = inner.id();
        Self {
            inner: ChildInner::Std(Some(inner)),
            tracker: None,
            pid,
        }
    }
}
```

Keep `Child::new(inner)` as a delegating compatibility constructor:

```rust
pub(crate) fn new(inner: std::process::Child) -> Self {
    Self::from_std(inner)
}
```

- [ ] **Step 3: Update methods to match `ChildInner::Std`**

For `stdin`, `stdout`, `stderr`, `take_stdin`, `take_stdout`, `take_stderr`,
`wait`, `wait_with_output`, `kill`, and `try_wait`, match on `self.inner`.

For example:

```rust
pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
    match &mut self.inner {
        ChildInner::Std(inner) => inner.as_mut().and_then(|child| child.stdout.take()),
    }
}
```

Use the same existing behavior for the `Std` variant.

- [ ] **Step 4: Run existing tests**

Run:

```bash
cargo test platform::tests::child_wraps_std_process_id
cargo test
```

Expected: new test passes and existing tests keep passing.

- [ ] **Step 5: Commit**

```bash
git add src/platform/mod.rs src/platform/macos/mod.rs src/platform/linux/mod.rs src/platform/windows.rs
git commit -m "refactor: prepare child abstraction for native windows handles"
```

---

### Task 6: Add Windows Path Policy Model

**Files:**
- Create: `src/platform/windows/paths.rs`
- Modify: `src/platform/windows.rs`

- [ ] **Step 1: Add tests for root access classification**

Create `src/platform/windows/paths.rs`:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootAccess {
    Read,
    Write,
    Execute,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootGrant {
    pub path: PathBuf,
    pub access: RootAccess,
}

pub(crate) fn grant(path: impl AsRef<Path>, access: RootAccess) -> RootGrant {
    RootGrant {
        path: path.as_ref().to_path_buf(),
        access,
    }
}

#[cfg(test)]
mod tests {
    use super::{grant, RootAccess};

    #[test]
    fn grant_keeps_path_and_access() {
        let item = grant("C:/Eureka/session", RootAccess::Full);

        assert_eq!(item.path.to_string_lossy(), "C:/Eureka/session");
        assert_eq!(item.access, RootAccess::Full);
    }
}
```

Add to `src/platform/windows.rs`:

```rust
mod paths;
```

- [ ] **Step 2: Run test**

Run:

```bash
cargo test grant_keeps_path_and_access
```

Expected: PASS because this is a pure model introduction.

- [ ] **Step 3: Add config-to-grants function and tests**

Append to `paths.rs`:

```rust
use crate::config::SandboxConfigData;

pub(crate) fn grants_from_config(config: &SandboxConfigData) -> Vec<RootGrant> {
    let mut grants = Vec::new();
    grants.push(grant(config.working_dir(), RootAccess::Full));
    grants.extend(config.readable_paths().iter().map(|path| grant(path, RootAccess::Read)));
    grants.extend(config.writable_paths().iter().map(|path| grant(path, RootAccess::Full)));
    grants.extend(config.executable_paths().iter().map(|path| grant(path, RootAccess::Execute)));
    grants
}
```

Add test:

```rust
#[test]
fn grants_from_config_maps_roots_to_access_classes() {
    let (_, config) = crate::config::SandboxConfig::builder()
        .working_dir("C:/Eureka/work")
        .readable_path("C:/Eureka/read")
        .writable_path("C:/Eureka/write")
        .executable_path("C:/Eureka/python")
        .build()
        .expect("config")
        .into_parts();

    let grants = super::grants_from_config(&config);

    assert!(grants.iter().any(|item| item.access == RootAccess::Full && item.path.ends_with("work")));
    assert!(grants.iter().any(|item| item.access == RootAccess::Read && item.path.ends_with("read")));
    assert!(grants.iter().any(|item| item.access == RootAccess::Full && item.path.ends_with("write")));
    assert!(grants.iter().any(|item| item.access == RootAccess::Execute && item.path.ends_with("python")));
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test grants_from_config_maps_roots_to_access_classes
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/paths.rs
git commit -m "feat: model windows sandbox path grants"
```

---

### Task 7: Add Windows ACL Grant Skeleton

**Files:**
- Create: `src/platform/windows/acl.rs`
- Modify: `src/platform/windows.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add ACL plan tests for RAII shape**

Create `src/platform/windows/acl.rs`:

```rust
use crate::error::Result;

use super::paths::RootGrant;

pub(crate) struct AclGrantGuard {
    count: usize,
}

impl AclGrantGuard {
    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

pub(crate) fn apply_grants_for_sid(
    grants: &[RootGrant],
    _sid_debug_label: &str,
) -> Result<AclGrantGuard> {
    Ok(AclGrantGuard { count: grants.len() })
}

#[cfg(test)]
mod tests {
    use super::apply_grants_for_sid;
    use crate::platform::windows::paths::{grant, RootAccess};

    #[test]
    fn acl_guard_records_grant_count() {
        let grants = vec![grant("C:/Eureka/work", RootAccess::Full)];

        let guard = apply_grants_for_sid(&grants, "S-1-test").expect("guard");

        assert_eq!(guard.count(), 1);
    }
}
```

Add to `src/platform/windows.rs`:

```rust
mod acl;
```

- [ ] **Step 2: Run tests**

Run:

```bash
cargo test acl_guard_records_grant_count
```

Expected: PASS. This introduces the RAII seam before real ACL mutation.

- [ ] **Step 3: Add Windows-only real implementation gate**

Add this function signature to `acl.rs`:

```rust
#[cfg(target_os = "windows")]
pub(crate) fn apply_grants_for_appcontainer_sid(
    grants: &[RootGrant],
    sid: windows::Win32::Foundation::PSID,
) -> Result<AclGrantGuard> {
    let _ = sid;
    apply_grants_for_sid(grants, "appcontainer")
}
```

This is intentionally not a security-complete implementation. It marks the
exact function that the next worker must replace with real DACL mutation before
WindowsBackend can report execution support beyond skeleton work.

- [ ] **Step 4: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/acl.rs
git commit -m "feat: add windows acl grant guard seam"
```

---

### Task 8: Add Windows Process Launch Skeleton

**Files:**
- Create: `src/platform/windows/process.rs`
- Modify: `src/platform/windows.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add command-line quoting tests**

Create `src/platform/windows/process.rs`:

```rust
pub(crate) fn quote_arg(input: &str) -> String {
    if input.is_empty() || input.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        let escaped = input.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        input.to_string()
    }
}

pub(crate) fn command_line(program: &str, args: &[String]) -> String {
    std::iter::once(quote_arg(program))
        .chain(args.iter().map(|arg| quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::command_line;

    #[test]
    fn command_line_quotes_spaces_and_quotes() {
        let line = command_line(
            "C:/Program Files/Python/python.exe",
            &["-c".to_string(), "print(\"hi\")".to_string()],
        );

        assert_eq!(
            line,
            "\"C:/Program Files/Python/python.exe\" -c \"print(\\\"hi\\\")\""
        );
    }
}
```

Add to `src/platform/windows.rs`:

```rust
mod process;
```

- [ ] **Step 2: Run tests**

Run:

```bash
cargo test command_line_quotes_spaces_and_quotes
```

Expected: PASS for pure command-line construction.

- [ ] **Step 3: Add Windows-only launch function skeleton**

Append to `process.rs`:

```rust
#[cfg(target_os = "windows")]
pub(crate) struct WindowsLaunch<'a> {
    pub(crate) program: &'a str,
    pub(crate) args: &'a [String],
    pub(crate) current_dir: &'a std::path::Path,
    pub(crate) envs: &'a [(String, String)],
}

#[cfg(target_os = "windows")]
pub(crate) fn launch_appcontainer_process(
    launch: WindowsLaunch<'_>,
    _profile: &super::profile::AppContainerProfile,
) -> crate::error::Result<crate::platform::Child> {
    let _ = command_line(launch.program, launch.args);
    let _ = (launch.current_dir, launch.envs);
    Err(crate::error::Error::UnsupportedPlatform)
}
```

The next implementation task replaces this skeleton with `CreateProcessW`.

- [ ] **Step 4: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/process.rs
git commit -m "feat: add windows process launch seam"
```

---

### Task 9: Wire WindowsBackend to Fail Closed Through the New Units

**Files:**
- Modify: `src/platform/windows.rs`

- [ ] **Step 1: Add backend tests for unsupported execution after successful policy validation**

In `src/platform/windows.rs`, add:

```rust
#[cfg(all(test, target_os = "windows"))]
mod tests {
    use std::process::Stdio;

    use crate::config::SandboxConfig;
    use crate::platform::Backend;

    use super::WindowsBackend;

    #[tokio::test]
    async fn windows_backend_still_fails_closed_until_process_launch_lands() {
        let (_, config) = SandboxConfig::builder()
            .build()
            .expect("config")
            .into_parts();
        let backend = WindowsBackend::new().expect("backend");

        let err = backend
            .execute(
                &config,
                0,
                "cmd.exe",
                &["/C".to_string(), "echo ok".to_string()],
                &[],
                None,
                Stdio::null(),
                Stdio::piped(),
                Stdio::piped(),
            )
            .await
            .expect_err("launch is not implemented yet");

        assert!(err.to_string().contains("unsupported platform"));
    }
}
```

- [ ] **Step 2: Run Windows target check**

Run:

```bash
cargo check --target x86_64-pc-windows-msvc
```

Expected: compile succeeds, or missing target is reported clearly.

- [ ] **Step 3: Commit**

```bash
git add src/platform/windows.rs
git commit -m "feat: wire windows backend skeleton"
```

---

### Task 10: Replace Process Skeleton With Real AppContainer Launch

**Files:**
- Modify: `src/platform/windows/process.rs`
- Modify: `src/platform/windows.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add ignored Windows integration test for foreground execution**

In `src/platform/windows.rs`, add:

```rust
#[tokio::test]
#[ignore = "requires Windows AppContainer process launch"]
async fn windows_backend_executes_cmd_echo_in_appcontainer() {
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
            Stdio::null(),
            Stdio::piped(),
            Stdio::piped(),
        )
        .await
        .expect("execute");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("heel-windows-ok"));
}
```

- [ ] **Step 2: Implement `CreateProcessW` launch**

Replace `launch_appcontainer_process()` with a real implementation that:

1. Builds the command line with `command_line()`.
2. Builds a double-null-terminated UTF-16 environment block from `envs`.
3. Allocates `STARTUPINFOEXW`.
4. Calls `InitializeProcThreadAttributeList`.
5. Sets `SECURITY_CAPABILITIES` using the profile SID.
6. Calls `UpdateProcThreadAttribute` with
   `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`.
7. Calls `CreateProcessW` with `EXTENDED_STARTUPINFO_PRESENT`.
8. Wraps the resulting handles in the Windows-native `Child` variant from Task
   5.
9. Calls `DeleteProcThreadAttributeList` before returning.

If Task 5 has not added a Windows-native `Child` variant yet, stop and finish
Task 5 first.

- [ ] **Step 3: Run Windows tests**

Run on Windows:

```bash
cargo test windows_backend_executes_cmd_echo_in_appcontainer -- --ignored
```

Expected: PASS and stdout contains `heel-windows-ok`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/platform/windows/process.rs src/platform/windows.rs src/platform/mod.rs
git commit -m "feat: launch commands in windows appcontainer"
```

---

### Task 11: Replace ACL Skeleton With Real AppContainer Grants

**Files:**
- Modify: `src/platform/windows/acl.rs`
- Modify: `src/platform/windows.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add ignored Windows integration tests for file boundaries**

Add Windows-only ignored tests that:

1. Create `working`, `read`, `write`, and `outside` temp directories.
2. Grant `readable_path(read)`.
3. Grant `writable_path(write)`.
4. Run a sandboxed command that reads from `read`.
5. Run a sandboxed command that writes to `write`.
6. Run a sandboxed command that tries to read from `outside` and expects failure.

Use command snippets through `cmd.exe /C` and `powershell -NoProfile -Command`
only if `cmd.exe` cannot express the file operation clearly.

- [ ] **Step 2: Implement real ACL grants**

Replace `apply_grants_for_appcontainer_sid()` so it:

1. Reads the current DACL/security descriptor for each root.
2. Adds inherited ACEs for the AppContainer SID according to `RootAccess`.
3. Stores enough original state in `AclGrantGuard` to restore on drop.
4. Restores in `Drop`.
5. Returns an error if any root cannot be granted safely.

Prefer a dedicated ACL crate if it can apply ACEs to an arbitrary SID without
shelling out. If it cannot, use `windows` APIs directly inside `acl.rs`.

- [ ] **Step 3: Run Windows file boundary tests**

Run:

```bash
cargo test windows_appcontainer_file_boundaries -- --ignored
```

Expected: allowed reads/writes succeed; outside reads fail.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/platform/windows/acl.rs src/platform/windows.rs
git commit -m "feat: enforce windows appcontainer file grants"
```

---

### Task 12: Add Job Object Process Tree Cleanup

**Files:**
- Create: `src/platform/windows/job.rs`
- Modify: `src/platform/windows/process.rs`
- Modify: `src/platform/windows.rs`

- [ ] **Step 1: Add ignored Windows process-tree cleanup test**

Add a Windows-only ignored test that starts:

```cmd
cmd.exe /C start /B ping 127.0.0.1 -t
```

The test should drop the sandbox or kill the returned child, then verify the
job terminates the process tree.

- [ ] **Step 2: Implement `JobGuard`**

Create `src/platform/windows/job.rs`:

```rust
pub(crate) struct JobGuard {
    handle: windows::Win32::Foundation::HANDLE,
}
```

Implement:

- create job
- set kill-on-job-close limit
- assign process handle
- terminate on explicit kill
- close handle on drop

- [ ] **Step 3: Wire process launch into `JobGuard`**

In `process.rs`, create the job before process launch or immediately after
launch. Assign the created process to the job before returning the child.

- [ ] **Step 4: Run Windows process-tree test**

Run:

```bash
cargo test windows_job_kills_process_tree -- --ignored
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/platform/windows/job.rs src/platform/windows/process.rs src/platform/windows.rs
git commit -m "feat: clean up windows sandbox process trees"
```

---

### Task 13: Add DenyAll Network Enforcement Tests

**Files:**
- Modify: `src/platform/windows.rs`
- Modify: `src/platform/windows/policy.rs`

- [ ] **Step 1: Add ignored Windows DenyAll tests**

Add Windows-only ignored tests for:

- outbound HTTP fails
- DNS lookup fails
- loopback connection fails
- `AllowAll` policy returns unsupported before launch

- [ ] **Step 2: Verify no internet/private network capabilities are granted**

Inspect `profile.rs` and `process.rs` and ensure `SECURITY_CAPABILITIES` has:

```rust
CapabilityCount = 0
Capabilities = std::ptr::null_mut()
```

Do not add `internetClient`, `privateNetworkClientServer`, or loopback
exemptions.

- [ ] **Step 3: Run Windows DenyAll tests**

Run:

```bash
cargo test windows_appcontainer_network_deny_all -- --ignored
```

Expected: all network attempts fail.

- [ ] **Step 4: Commit**

```bash
git add src/platform/windows.rs src/platform/windows/policy.rs src/platform/windows/process.rs
git commit -m "test: verify windows appcontainer deny-all network"
```

---

## Verification Commands

Run after each local non-Windows slice:

```bash
cargo test platform::tests::
cargo test profile_name_is_stable_and_sanitized
cargo test grant_keeps_path_and_access
cargo test command_line_quotes_spaces_and_quotes
cargo test
```

Run on a Windows machine after Windows API tasks:

```bash
cargo check --target x86_64-pc-windows-msvc
cargo test --target x86_64-pc-windows-msvc
cargo test windows_backend_executes_cmd_echo_in_appcontainer -- --ignored
cargo test windows_appcontainer_file_boundaries -- --ignored
cargo test windows_job_kills_process_tree -- --ignored
cargo test windows_appcontainer_network_deny_all -- --ignored
```

---

## Initial Execution Strategy

Start with Tasks 1, 3, 5, 6, and 8 on macOS because they are mostly pure Rust
or preserve existing Unix behavior. Use parallel agents for exploration and
review, but avoid parallel worker edits to the same Windows backend files.

Tasks 4, 10, 11, 12, and 13 require Windows verification. They may be
implemented with `cargo check --target x86_64-pc-windows-msvc` from macOS, but
they are not complete until the ignored Windows integration tests pass on a real
Windows host.
