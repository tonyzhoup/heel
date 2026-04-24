# Windows AppContainer Backend for Heel

Date: 2026-04-24

## Purpose

Heel already exposes a platform-neutral sandbox API through `SandboxConfig`,
`Sandbox`, and the internal `Backend` trait. macOS and Linux have concrete
backends, while Windows currently compiles to a `WindowsBackend` that returns
`UnsupportedPlatform`.

This design adds a real Windows backend based on AppContainer. The first
production target is a reliable `sandbox_guaranteed` execution path for
Eureka Desktop's `python.run`, followed by a staged `exec.run` rollout. The
application layer must keep using one platform-neutral interface. It should not
branch on AppContainer, Landlock, or `sandbox-exec`.

## Current State

Heel's platform backend contract is:

- `execute(...) -> Output`
- `spawn(...) -> Child`

Both receive `SandboxConfigData`, proxy port, program, arguments, explicit
environment, current directory, and stdio configuration.

The current Windows backend has the right shape but no behavior:

- `WindowsBackend::new()` succeeds.
- `execute()` and `spawn()` return `Error::UnsupportedPlatform`.

The existing shared configuration already contains the policy information
needed for a Windows backend:

- `working_dir`
- `readable_paths`
- `writable_paths`
- `executable_paths`
- `filesystem_strict`
- `network_deny_all`
- `SecurityConfig`
- optional IPC metadata

Eureka's current integration is also already aligned with this model:

- `python.run` resolves scripts and cwd through handles.
- cwd is restricted to `@session` or `@skill_workspace`.
- runtime/package preparation happens on the host.
- sandboxed script execution receives explicit read/write/executable roots.
- `exec.run` still runs as a host process and should be migrated later in
  narrower modes.

## Goals

1. Implement a Windows backend that can run untrusted child processes inside an
   AppContainer.
2. Enforce file access through explicit AppContainer SID ACL grants derived from
   `SandboxConfigData`.
3. Enforce `network_deny_all` by default without granting network capabilities.
4. Track and clean up foreground and background process trees using Job
   Objects.
5. Preserve the existing `SandboxConfig` application API.
6. Fail closed whenever AppContainer setup, ACL setup, process creation, or
   cleanup cannot be made safe.

## Non-Goals

1. Do not implement network allowlist in the first Windows release.
2. Do not claim `HTTP_PROXY` injection is a network sandbox on Windows.
3. Do not grant broad `%USERPROFILE%`, Documents, Desktop, or Downloads access
   from `SecurityConfig::permissive`.
4. Do not support interactive PTY/shell parity in the first release.
5. Do not make Windows Sandbox, WSL, Hyper-V, or a remote executor part of this
   backend.

## Design Principles

The Windows backend must be policy-faithful before it is feature-complete.

- If `network_deny_all` is true, the AppContainer receives no internet
  capability.
- If `filesystem_strict` is true, only explicit roots plus minimal required
  system/runtime objects are granted.
- All grants are tied to the AppContainer SID, not the user's normal token.
- Temporary grants are scoped and reversible.
- `SandboxConfig` remains the only application-facing contract.

## Architecture

### Module Layout

Add Windows-specific modules under `src/platform/windows/`:

- `mod.rs`
  - Implements `WindowsBackend`.
  - Orchestrates profile, ACL, job, environment, process launch, and cleanup.

- `profile.rs`
  - Owns AppContainer profile naming.
  - Creates or derives the AppContainer SID.
  - Deletes transient profiles only after all process and file handles are
    closed.

- `acl.rs`
  - Applies inherited ACEs for read, write, and execute roots.
  - Records original security descriptors or reversible ACL edits.
  - Restores temporary grants in RAII cleanup.

- `process.rs`
  - Creates AppContainer processes using `STARTUPINFOEX`,
    `SECURITY_CAPABILITIES`, and
    `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`.
  - Builds Windows command lines and environment blocks.
  - Wires stdio handles.

- `job.rs`
  - Creates a Job Object per sandbox or process group.
  - Assigns every sandboxed process to the job.
  - Terminates child trees on sandbox drop.

- `paths.rs`
  - Canonicalizes and validates Windows paths.
  - Rejects non-local or unsupported roots when ACL semantics cannot be
    guaranteed.

### Process Creation

The backend should use the Win32 process APIs directly rather than rely on
`std::process::Command` for the first Windows implementation. AppContainer
launch requires extended startup attributes, and the required thread attribute
is not part of Heel's current generic `Command` abstraction.

Implementation shape:

1. Resolve and canonicalize `program`.
2. Build the Windows command line from `program` and `args`.
3. Create inheritable pipe handles according to `Stdio`.
4. Create or derive an AppContainer SID.
5. Fill `SECURITY_CAPABILITIES`.
6. Allocate and initialize `PROC_THREAD_ATTRIBUTE_LIST`.
7. Attach `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`.
8. Call `CreateProcessW` with `EXTENDED_STARTUPINFO_PRESENT`.
9. Assign the process to a Job Object.
10. Return a Heel child handle that exposes wait, kill, try_wait, and stdio.

This implies one internal refactor: `platform::Child` should stop assuming that
every backend returns `std::process::Child`. It should become a platform-owned
child wrapper or enum. Unix backends can still wrap `std::process::Child`;
Windows can wrap `PROCESS_INFORMATION`, stdio handles, and the owning job.

### AppContainer Profile Model

Use a stable profile name derived from the sandbox config and an application
prefix, for example:

`heel.<app-id>.<config-hash>`

The name must satisfy AppContainer profile constraints and remain shorter than
the Windows profile name limit. A stable profile is preferable to creating a
new profile per command because it reduces profile churn and Event Log noise.

For tests that need strict cleanup, support an internal transient profile mode.
Transient mode must delete the profile after all process handles and profile
storage handles are closed.

### ACL Policy

Map Heel roots to Windows access:

- `working_dir`
  - read, write, execute, list, create, delete inside the tree

- `readable_paths`
  - read and list

- `writable_paths`
  - read, write, list, create, delete inside the tree

- `executable_paths`
  - read, list, execute

Executable Python environments need more than the interpreter file. The runtime
root must include DLLs, `Lib`, `DLLs`, `Scripts`, and `site-packages` as
required by Windows Python.

The ACL implementation should use a dedicated Windows ACL crate where it can
correctly express inherited ACEs for an arbitrary SID. If the crate cannot
grant ACEs to an AppContainer SID cleanly, use `windows` APIs directly for this
module only. Do not duplicate ACL manipulation across the backend.

### Network Policy

First release supports one enforced mode:

- `DenyAll`: no internet capability, no private network capability, no
  loopback exemption.

For non-`DenyAll` policies, the Windows backend must return an explicit
unsupported error until a stronger design exists. Injecting `HTTP_PROXY` and
`HTTPS_PROXY` is useful for cooperative tools but is not a security boundary.
Granting `internetClient` would allow direct outbound traffic and would break
the meaning of `NetworkPolicy`.

Future allowlist work must be a separate design. Candidate mechanisms are WFP,
per-AppContainer firewall policy, or a brokered network service that cannot be
bypassed by direct sockets.

### IPC

Heel IPC currently starts a host TCP server and injects `HEEL_IPC_ENDPOINT`.
AppContainer has loopback restrictions, and loopback exemptions are too broad
for the first release.

First release behavior:

- If IPC is not configured, Windows backend works normally.
- If IPC is configured, Windows backend returns an explicit unsupported error.

Future IPC should use a named pipe or broker endpoint with an ACL that grants
only the AppContainer SID.

### SecurityConfig Mapping

Windows should interpret `SecurityConfig` conservatively:

- `protect_user_home=true`
  - Do not grant user profile roots except explicit config roots.

- `protect_user_home=false`
  - Do not automatically grant the entire profile.
  - Treat this as a no-op unless a future API defines broad home access
    explicitly.

- credential and cloud config protections
  - Explicit deny rules are optional in the first release because AppContainer
    lacks access without ACL grants. Add deny ACEs only if they are needed to
    defend against inherited grants from broad roots.

- hardware flags
  - No hardware capability grants in the first release.

This mapping intentionally differs from macOS's TCC-oriented permissive mode.
The Windows backend should be explicit-root-first.

## Public API Impact

The preferred public API impact is minimal:

- Existing `SandboxConfig` remains valid.
- Existing `Sandbox::command(...).output()` remains valid.
- Existing `Sandbox::command(...).spawn()` remains valid after the internal
  child abstraction is refactored.

Add one capability API so application layers can fail closed without platform
conditionals:

```rust
pub struct PlatformCapabilities {
    pub backend: &'static str,
    pub filesystem_strict: bool,
    pub network_deny_all: bool,
    pub network_allowlist: bool,
    pub ipc: bool,
    pub background_process_tree_cleanup: bool,
}

pub fn platform_capabilities() -> PlatformCapabilities;
```

Windows first release returns:

- `backend = "windows_appcontainer"`
- `filesystem_strict = true`
- `network_deny_all = true`
- `network_allowlist = false`
- `ipc = false`
- `background_process_tree_cleanup = true`

## Error Handling

All safety-critical setup failures must be fatal for the sandboxed command:

- profile creation or SID derivation failure
- ACL grant failure
- unsupported path root
- unsupported non-`DenyAll` network policy
- unsupported IPC
- process creation failure
- Job Object assignment failure
- ACL restore failure after command completion

Cleanup failures should be returned from `execute()` when they occur before the
result is returned. For `spawn()`, cleanup failures should be surfaced through
drop logging and a future diagnostic hook because drop cannot return a result.

No native fallback is allowed inside Heel.

## Implementation Phases

### Phase 1: Compile-Time Backend Skeleton

Deliverables:

- Replace `src/platform/windows.rs` with a module directory.
- Add Windows capability reporting.
- Add Windows-only tests for capability values and unsupported policy errors.
- Keep `execute()` and `spawn()` fail-closed until process creation lands.

### Phase 2: Foreground AppContainer Execution

Deliverables:

- Implement profile creation and SID derivation.
- Implement direct `CreateProcessW` launch with `SECURITY_CAPABILITIES`.
- Implement foreground `execute()`.
- Support null, inherited, and piped stdio.
- Add smoke tests for `cmd /c echo ok` and `whoami /all`.

### Phase 3: File Policy Enforcement

Deliverables:

- Implement ACL grant and restore.
- Grant working, readable, writable, and executable roots.
- Reject unsupported roots.
- Add tests that prove allowed reads/writes work and ungranted user-profile
  paths fail.
- Add Python runtime execution tests using a Windows venv layout.

### Phase 4: DenyAll Network Enforcement

Deliverables:

- Ensure AppContainer has no network capabilities in `DenyAll`.
- Return unsupported for non-`DenyAll` network policies.
- Add tests for failed outbound HTTP, DNS, and loopback access.

### Phase 5: Spawn and Job Object Lifecycle

Deliverables:

- Refactor `platform::Child` to support Windows-native child handles.
- Implement `spawn()`, `wait()`, `try_wait()`, `kill()`, and stdio access.
- Assign processes to a Job Object.
- Add tests proving child and grandchild processes are terminated on sandbox
  drop.

### Phase 6: Eureka Integration

Deliverables:

- Update Eureka to treat `windows_appcontainer` as a supported
  `SandboxGuaranteed` backend.
- Add `heel_windows_appcontainer` sandbox strength label.
- Keep `python.run` first.
- Add `exec.run` sandbox mode only for foreground, short-lived, session-scoped
  commands with `network_deny_all`.
- Keep native approved mode for package managers, dev servers, networked
  commands, and long-running processes until later phases.

## Test Matrix

Windows tests should run on Windows 10 and Windows 11 where possible.

Core tests:

- backend capability reports `windows_appcontainer`
- profile creation succeeds without admin privileges
- process token shows AppContainer SID
- `working_dir` write succeeds
- `readable_paths` read succeeds
- `readable_paths` write fails
- `writable_paths` read/write succeeds
- ungranted user home read fails
- ungranted credential paths fail
- Python interpreter executes from an executable root
- Python can import from granted runtime roots
- Python cannot write outside session/skill workspace
- outbound TCP/HTTP/DNS fails in `DenyAll`
- loopback access fails in `DenyAll`
- non-`DenyAll` network policy returns unsupported
- IPC configured returns unsupported
- process tree is killed when sandbox drops

Eureka tests:

- `python.run` on Windows reports `execution_mode="sandbox_guaranteed"`
- `python.run` reports `sandbox.strength="heel_windows_appcontainer"`
- untrusted uploaded Python script auto-approves only when backend capability is
  real
- sandbox backend unavailable fails closed
- `exec.run` sandbox mode rejects networked or long-running command families
- native approved `exec.run` remains available for unsupported command classes

## Rollout Criteria

Windows AppContainer support is ready for Eureka `python.run` when:

- foreground Python execution works with managed runtime roots
- file policy tests pass
- DenyAll network tests pass
- unsupported non-`DenyAll` and IPC paths fail closed
- no native fallback exists behind `sandbox_guaranteed`

Windows AppContainer support is ready for Eureka `exec.run` phase 1 when:

- foreground generic commands work
- output capture works
- declared cwd/output roots map to ACL grants
- command classes that need network, package manager mutation, or long-lived
  process control are still routed to native approved mode

## References

- Microsoft Windows application isolation:
  https://learn.microsoft.com/en-us/windows/security/book/application-security-application-isolation
- windows-rs `CreateAppContainerProfile`:
  https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/Isolation/fn.CreateAppContainerProfile.html
- windows-rs `DeriveAppContainerSidFromAppContainerName`:
  https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/Isolation/fn.DeriveAppContainerSidFromAppContainerName.html
- windows-rs `SECURITY_CAPABILITIES`:
  https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/struct.SECURITY_CAPABILITIES.html
- windows-rs `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`:
  https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/constant.PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES.html
- windows-rs `CreateProcessW`:
  https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/fn.CreateProcessW.html
- `windows-acl` crate:
  https://docs.rs/windows-acl
