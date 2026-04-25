# Windows 版 Heel 沙箱验证手册

本文用于在真实 Windows 环境中验证 Heel 的 Windows AppContainer 后端。当前云电脑路径已确认可用：

- 源码仓库: `G:\MyCodeRepo\Heel`
- 建议验证工作区: Windows 本地 NTFS 目录，例如 `$env:TEMP\heel-win-validation`

验证原则：

- 先验证能力边界，再验证易用性。不能只看命令是否能跑通。
- AppContainer 文件权限依赖 Windows ACL。验证读写隔离时使用本地 NTFS 目录，不使用 `G:` 挂载盘作为沙箱工作区或受保护目录。
- 不要求管理员权限。如果某一步必须管理员才能通过，默认视为设计问题，除非该能力明确声明需要管理员。
- 每个失败场景都必须检查两件事：命令返回失败，以及受保护文件没有被创建、修改或读取。
- 当前 Windows 后端仍处于 fail-closed 阶段时，执行测试应返回明确 unsupported error；真正执行落地后，再切到完整验收矩阵。

## Computer Use 操作约定

云电脑里的 Windows 控件不会像本机 macOS 应用一样暴露完整可访问性树。为了让 Computer Use 高效、稳定地完成验证，默认采用 PowerShell 驱动的非交互流程。

推荐工作方式：

- 保持 Cloud Computer 窗口尺寸和位置稳定。验证过程中不要频繁缩放、移动窗口或切换显示比例，避免坐标点击失准。
- 以 PowerShell 为主，不用图形界面逐项操作测试。资源管理器只用于定位 `G:\MyCodeRepo\Heel`、打开目录或确认文件是否存在。
- 尽量把多步验证写成幂等 PowerShell 脚本，然后一次运行；不要让 Computer Use 长时间逐字输入大量命令。
- 每个脚本步骤输出清晰标记，例如 `=== STEP: network deny-all ===`，最后输出 `PASS` 或抛出明确错误。
- 使用 `Start-Transcript` 或显式 log 文件保存验证输出，方便之后回看，而不是只依赖截图里的终端文本。
- 需要创建、清理测试文件时，只操作 `$env:TEMP\heel-win-validation` 这类专用目录。避免对仓库、用户目录、`G:` 根目录做递归删除或批量改 ACL。
- 需要验证 ACL/AppContainer 文件隔离时，测试根目录放在 Windows 本地 NTFS，例如 `$env:TEMP\heel-win-validation`。`G:` 是宿主挂载盘，适合读源码和构建，不适合作为隔离语义的判断依据。
- 运行命令统一使用非交互参数，例如 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File ...`，避免 profile、执行策略或提示框影响结果。
- 如果用户在云电脑里手动操作过窗口，Computer Use 下一步动作前需要重新看当前截图状态，再继续点击或输入。
- 如果快捷键不稳定，优先使用鼠标点击目标输入区后输入文本。已观察到坐标点击和普通文本输入可用，但部分快捷键可能被宿主或云电脑客户端截获。

推荐优先使用仓库内的脚本化验证入口，避免在云电脑中手敲长命令：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File G:\MyCodeRepo\Heel\scripts\windows-sandbox-validation.ps1 -Mode Auto
```

脚本会从自身位置解析仓库根目录，默认在 `$env:TEMP\heel-win-validation\run-*` 下创建唯一验证目录，并输出 transcript log 和 markdown report 路径。需要强制完整 AppContainer 验收时使用 `-Mode Full`；需要验证旧的 unsupported fail-closed 合同时使用 `-Mode FailClosed`。

建议的脚本执行外壳：

```powershell
$ErrorActionPreference = "Stop"
$RunId = Get-Date -Format "yyyyMMdd-HHmmss"
$Root = Join-Path $env:TEMP "heel-win-validation"
$Log = Join-Path $Root "validation-$RunId.log"
New-Item -ItemType Directory -Force $Root | Out-Null

Start-Transcript -Path $Log -Force
try {
    Write-Host "=== STEP: environment ==="
    Set-Location "G:\MyCodeRepo\Heel"
    git status --short
    rustc --version
    cargo --version

    Write-Host "=== STEP: validation commands ==="
    # Run the checks from this document here.

    Write-Host "PASS"
} finally {
    Stop-Transcript
    Write-Host "log: $Log"
}
```

## 1. 环境准备

在云电脑 Windows PowerShell 中执行：

```powershell
$ErrorActionPreference = "Stop"

$Repo = "G:\MyCodeRepo\Heel"
$Root = Join-Path $env:TEMP "heel-win-validation"
$Sandbox = Join-Path $Root "sandbox"
$Readable = Join-Path $Root "readable"
$Writable = Join-Path $Root "writable"
$Outside = Join-Path $Root "outside"

Set-Location $Repo
New-Item -ItemType Directory -Force $Root, $Sandbox, $Readable, $Writable, $Outside | Out-Null

"readable-secret" | Set-Content -Encoding UTF8 (Join-Path $Readable "readable.txt")
"outside-secret" | Set-Content -Encoding UTF8 (Join-Path $Outside "secret.txt")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Outside "blocked-write.txt")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Readable "blocked-write.txt")
```

记录基础信息：

```powershell
git status --short
rustc --version
cargo --version
Get-ComputerInfo | Select-Object WindowsProductName, WindowsVersion, OsBuildNumber
```

通过标准：能进入 `$Repo`，Rust 工具链可用，仓库状态已记录。

## 2. 构建与单元测试

```powershell
cargo test -p heel --lib
cargo build -p heel --bin heel

$Heel = Join-Path $Repo "target\debug\heel.exe"
& $Heel --version
```

通过标准：

- `cargo test -p heel --lib` 通过，或者只有与当前 Windows 未落地能力一致的 fail-closed 断言变化。
- `target\debug\heel.exe` 构建成功。
- `heel --version` 可输出版本。

## 3. 当前 fail-closed 阶段检查

在 `src/platform/windows/process.rs` 仍返回 `UnsupportedPlatform`，且 `platform_capabilities().execution_supported == false` 时，执行命令不应退化成宿主直接运行。

```powershell
& $Heel run --working-dir $Sandbox cmd.exe /C "echo should-not-run"
if ($LASTEXITCODE -eq 0) {
    throw "Windows backend unexpectedly executed a command while capability contract is fail-closed."
}
```

通过标准：

- 命令失败。
- 错误信息明确指向 unsupported platform 或未支持的 Windows AppContainer 能力。
- 不能出现 `should-not-run` 被当作正常沙箱执行成功的结果。

当 Windows 执行能力落地后，本节应不再作为失败预期使用，改跑后续完整验收矩阵。

## 4. 执行冒烟测试

适用条件：`platform_capabilities().execution_supported == true`。

```powershell
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Sandbox "hello.txt")

& $Heel run --working-dir $Sandbox cmd.exe /C "cd && echo heel-ok>hello.txt && type hello.txt"
if ($LASTEXITCODE -ne 0) {
    throw "sandbox command failed"
}

Get-Content (Join-Path $Sandbox "hello.txt")
```

通过标准：

- 输出包含 `heel-ok`。
- `hello.txt` 只创建在 `$Sandbox` 内。
- 当前目录为 `$Sandbox`，不是仓库目录或 PowerShell 启动目录。

## 5. 文件系统隔离矩阵

### 5.1 默认 strict 模式拒绝外部读写

```powershell
& $Heel run --working-dir $Sandbox cmd.exe /C "type `"$Outside\secret.txt`""
if ($LASTEXITCODE -eq 0) {
    throw "sandbox read outside root unexpectedly succeeded"
}

& $Heel run --working-dir $Sandbox cmd.exe /C "echo blocked>`"$Outside\blocked-write.txt`""
if ($LASTEXITCODE -eq 0) {
    throw "sandbox write outside root unexpectedly succeeded"
}

if (Test-Path (Join-Path $Outside "blocked-write.txt")) {
    throw "outside write left a file behind"
}
```

通过标准：

- 外部读取失败。
- 外部写入失败。
- `$Outside\blocked-write.txt` 不存在。

### 5.2 `--readable` 只允许读取，不允许写入

```powershell
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Readable "blocked-write.txt")

& $Heel run --working-dir $Sandbox --readable $Readable cmd.exe /C "type `"$Readable\readable.txt`""
if ($LASTEXITCODE -ne 0) {
    throw "readable root could not be read"
}

& $Heel run --working-dir $Sandbox --readable $Readable cmd.exe /C "echo blocked>`"$Readable\blocked-write.txt`""
if ($LASTEXITCODE -eq 0) {
    throw "readable root unexpectedly accepted writes"
}

if (Test-Path (Join-Path $Readable "blocked-write.txt")) {
    throw "readable root write left a file behind"
}
```

通过标准：

- `$Readable\readable.txt` 可读。
- `$Readable\blocked-write.txt` 不可创建。

### 5.3 `--writable` 允许读写指定目录

```powershell
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Writable "created.txt")

& $Heel run --working-dir $Sandbox --writable $Writable cmd.exe /C "echo writable-ok>`"$Writable\created.txt`" && type `"$Writable\created.txt`""
if ($LASTEXITCODE -ne 0) {
    throw "writable root could not be written"
}

Get-Content (Join-Path $Writable "created.txt")
```

通过标准：

- 输出包含 `writable-ok`。
- 文件只出现在 `$Writable`。
- `$Outside` 没有新增文件。

## 6. 网络策略

Windows 首个 AppContainer 版本只验收 `DenyAll`。`AllowAll` 和 `AllowList` 在没有强网络隔离设计前必须 fail closed。

### 6.1 默认拒绝网络

```powershell
& $Heel run --working-dir $Sandbox powershell.exe -NoProfile -Command "try { Invoke-WebRequest https://example.com -UseBasicParsing -TimeoutSec 5 | Out-Null; exit 10 } catch { exit 0 }"
if ($LASTEXITCODE -ne 0) {
    throw "default DenyAll network policy was bypassed"
}
```

通过标准：沙箱内请求失败，外层命令返回 0。

### 6.2 非 DenyAll 必须明确拒绝

```powershell
& $Heel run --working-dir $Sandbox --network allow cmd.exe /C "echo should-not-run"
if ($LASTEXITCODE -eq 0) {
    throw "--network allow unexpectedly ran on Windows"
}

& $Heel run --working-dir $Sandbox --network allow-list --allow-domain example.com cmd.exe /C "echo should-not-run"
if ($LASTEXITCODE -eq 0) {
    throw "--network allow-list unexpectedly ran on Windows"
}
```

通过标准：

- 两条命令都失败。
- 错误信息包含 Windows network policy 不支持的语义，例如 `windows-appcontainer-network`。
- 不能通过注入 `HTTP_PROXY` 或 `HTTPS_PROXY` 把 allowlist 伪装成安全边界。

## 7. Python 场景

先验证无第三方包的脚本执行。包安装和 venv 准备属于宿主准备阶段，不应在沙箱内获得额外网络能力。

```powershell
$Python = (Get-Command python.exe).Source
$Probe = Join-Path $Sandbox "probe.py"

@"
from pathlib import Path
root = Path.cwd()
(root / "python-ok.txt").write_text("python-ok", encoding="utf-8")
print((root / "python-ok.txt").read_text(encoding="utf-8"))
"@ | Set-Content -Encoding UTF8 $Probe

& $Heel python --working-dir $Sandbox --python $Python $Probe
if ($LASTEXITCODE -ne 0) {
    throw "heel python smoke test failed"
}

Get-Content (Join-Path $Sandbox "python-ok.txt")
```

通过标准：

- 输出包含 `python-ok`。
- 写入只发生在 `$Sandbox`。
- Python 解释器及其运行时根必须由 Windows 后端按 runtime/executable root 处理，不应通过放宽用户目录实现。

## 8. 环境变量与当前目录

```powershell
& $Heel run --working-dir $Sandbox --env HEEL_VALIDATION=ok cmd.exe /C "echo %CD% && echo %HEEL_VALIDATION%"
if ($LASTEXITCODE -ne 0) {
    throw "env/current-dir test failed"
}
```

通过标准：

- 第一行当前目录指向 `$Sandbox`。
- 输出包含 `ok`。
- 未显式传入的敏感环境变量不要作为通过条件依赖。

## 9. 子进程树清理

适用条件：Windows 后端声明 `background_process_tree_cleanup == true`。

```powershell
$Loop = Join-Path $Sandbox "loop.ps1"
$Marker = Join-Path $Sandbox "loop-marker.txt"
Remove-Item -Force -ErrorAction SilentlyContinue $Marker

@"
while (`$true) {
    Add-Content -Encoding UTF8 "$Marker" "tick"
    Start-Sleep -Seconds 1
}
"@ | Set-Content -Encoding UTF8 $Loop

& $Heel run --working-dir $Sandbox powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Start-Process powershell.exe -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File', '$Loop'; Start-Sleep -Seconds 2"
if ($LASTEXITCODE -ne 0) {
    throw "background process setup command failed"
}

$Before = if (Test-Path $Marker) { (Get-Content $Marker).Count } else { 0 }
Start-Sleep -Seconds 4
$After = if (Test-Path $Marker) { (Get-Content $Marker).Count } else { 0 }

if ($After -gt $Before) {
    throw "background child process survived sandbox cleanup"
}
```

通过标准：

- 父命令退出后，后台子进程不再继续写 marker。
- 没有残留的 sandbox 子进程。
- Job Object 分配失败必须导致命令失败，不能降级为只清理前台进程。

## 10. 不支持能力的 fail-closed 验证

在对应能力正式设计和实现前，以下行为必须明确拒绝：

```powershell
& $Heel run --working-dir $Sandbox --permissive cmd.exe /C "echo should-not-run"
if ($LASTEXITCODE -eq 0) {
    throw "--permissive unexpectedly ran on Windows"
}
```

通过标准：

- `--permissive` 失败，错误指向 non-strict filesystem 或 globally writable filesystem 不支持。
- IPC 配置相关单元测试仍能证明 Windows 后端拒绝 IPC。
- 任何 unsupported 都必须在创建非沙箱子进程前发生。

建议配套单测：

```powershell
cargo test -p heel windows_policy_rejects_non_deny_all_network
cargo test -p heel windows_policy_rejects_non_strict_filesystem
cargo test -p heel windows_policy_rejects_globally_writable_filesystem
cargo test -p heel windows_policy_rejects_ipc
```

## 11. 验收记录模板

每次真实 Windows 验证后记录以下信息，建议保存为 `docs/windows-validation-runs/YYYY-MM-DD.md`：

```markdown
# Windows Heel Validation - YYYY-MM-DD

- Repo commit:
- Windows version:
- Rust version:
- Heel binary:
- Validation root:
- Execution capability:
- Filesystem strict:
- Network deny-all:
- Process tree cleanup:

## Results

| Area | Result | Evidence |
| --- | --- | --- |
| Build and unit tests | PASS/FAIL | command output summary |
| Current fail-closed or execution smoke | PASS/FAIL | command output summary |
| Default outside read/write denied | PASS/FAIL | command output summary |
| Readable root read-only | PASS/FAIL | command output summary |
| Writable root read/write | PASS/FAIL | command output summary |
| Network DenyAll | PASS/FAIL | command output summary |
| Non-DenyAll rejected | PASS/FAIL | command output summary |
| Python smoke | PASS/FAIL | command output summary |
| Env and cwd | PASS/FAIL | command output summary |
| Process tree cleanup | PASS/FAIL/NA | command output summary |

## Notes

- Unexpected behavior:
- Follow-up fixes:
```

## 12. 发布前通过标准

Windows 版 Heel 不能只因为 `heel run cmd.exe /C echo ok` 成功就发布。至少满足：

- 构建和 Windows 相关单测通过。
- 默认 strict 文件系统只允许工作区。
- `--readable`、`--writable`、`--executable` 或 runtime grants 的语义与设计一致。
- 默认 `DenyAll` 网络不可绕过。
- 非 `DenyAll` 网络在未实现强隔离前明确 fail closed。
- IPC 在未完成 Windows 安全设计前明确 fail closed。
- 后台进程树能够被 Job Object 清理，或者能力标记保持 false 且产品层不依赖它。
- 所有 unsupported case 都在启动未沙箱化子进程前失败。
- 验证记录中包含 commit、Windows 版本、命令摘要和失败证据。
