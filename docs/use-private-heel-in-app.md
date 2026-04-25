# 在应用项目中使用私有 Heel 开发版

本文用于把当前私有仓库 `https://github.com/tonyzhoup/heel` 中的 Windows AppContainer 版 Heel 接入正在开发的应用，并指导 Codex 按当前能力边界开发和测试。

## 0. 给另一台开发机上的 Codex 的最短指令

可以把下面这段直接交给另一个项目里的 Codex：

```text
请使用私有仓库 https://github.com/tonyzhoup/heel 的 feat/windows-support 分支集成 Heel。

不要使用 crates.io 上的 heel = "0.1.1"，那个版本不包含当前 Windows AppContainer first-phase 改动。

如果应用是 Rust 项目，优先用 Cargo git dependency，并在验证稳定后固定到具体 rev：

heel = { git = "ssh://git@github.com/tonyzhoup/heel.git", branch = "feat/windows-support" }

如果 Cargo 拉私库失败，在应用项目添加 .cargo/config.toml：

[net]
git-fetch-with-cli = true

Windows 集成必须验证：
- heel run 可以执行命令。
- heel python 可以运行 Python 脚本。
- heel python --venv <path> 不传 --python 也能创建和使用 venv。
- sandbox 内 Python 不能联网。
- sandbox 内不能读写未授权外部路径。
- CPython 的 "Failed to find real location of ...python.exe" stderr warning 是已知行为，不作为失败条件。
```

另一台机器同步 Heel 仓库时：

```powershell
git clone ssh://git@github.com/tonyzhoup/heel.git
cd heel
git checkout feat/windows-support
git pull --ff-only
```

如果已经 clone：

```powershell
cd path/to/heel
git fetch origin
git checkout feat/windows-support
git pull --ff-only
```

## 1. 当前能力边界

当前 `feat/windows-support` 分支的第一阶段目标是：在 Windows 上运行 agent 生成的 Python 代码，默认无网络，并严格限制文件访问。

Windows 当前已验证能力：

- `heel run` 可以在 AppContainer 中执行命令。
- `heel python` 可以运行系统 Python 脚本。
- `heel python --venv <path>` 可以创建并使用 Windows venv。
- 默认只允许读写 sandbox working dir。
- `--readable <path>` 只读授权指定目录。
- `--writable <path>` 读写授权指定目录。
- 默认 `DenyAll` 网络策略生效，Python socket/HTTP 出网失败。
- 后台子进程树会被 Job Object 清理。

Windows 当前不作为第一阶段目标：

- `--network allow`
- `--network allow-list`
- IPC
- 交互式 PTY/shell parity
- 在 sandbox 内安装 Python package 或执行需要联网的 package manager 操作

使用时应把 package/venv 准备放在宿主侧完成，再把准备好的 runtime 放进 Heel sandbox 中运行。

## 2. 推荐接入方式

如果应用是 Rust 项目，推荐先使用私有 git dependency，并固定 commit：

```toml
[dependencies]
heel = { git = "ssh://git@github.com/tonyzhoup/heel.git", rev = "<HEEL_COMMIT_SHA>" }
```

开发调试期间也可以临时用本地 path dependency：

```toml
[dependencies]
heel = { path = "C:/Users/tonyzp/CodeBase/Heel" }
```

不建议长期依赖 branch：

```toml
heel = { git = "ssh://git@github.com/tonyzhoup/heel.git", branch = "feat/windows-support" }
```

branch 依赖会让构建结果随分支变化，不利于复现。

实践建议：

1. 集成初期可以先用 `branch = "feat/windows-support"`，方便拿到最新开发版。
2. 验证通过后执行 `git rev-parse HEAD` 获取 Heel commit。
3. 把应用项目里的依赖改成 `rev = "<HEEL_COMMIT_SHA>"`。
4. 每次升级 Heel 时，用 `cargo update -p heel` 或删除 `Cargo.lock` 中对应条目后重新解析依赖。

## 3. 私有仓库认证

因为 `tonyzhoup/heel` 是 private repo，应用项目和 CI 都必须有访问权限。

本机开发建议使用 SSH：

```powershell
ssh -T git@github.com
git ls-remote ssh://git@github.com/tonyzhoup/heel.git
```

如果 Cargo 拉私库失败，在应用项目中加入：

```toml
# .cargo/config.toml
[net]
git-fetch-with-cli = true
```

CI 中需要配置 GitHub token、SSH deploy key，或其他能读取该私库的凭证。

在 CI 里，推荐优先使用 SSH deploy key，并确保 Cargo 使用 git CLI：

```toml
# .cargo/config.toml
[net]
git-fetch-with-cli = true
```

然后在 CI job 中先验证：

```powershell
git ls-remote ssh://git@github.com/tonyzhoup/heel.git
```

## 4. 安装 CLI 供应用测试

在当前机器上开发应用时，可以直接安装本地 Heel CLI：

```powershell
cargo install --path C:/Users/tonyzp/CodeBase/Heel --force
heel --version
```

也可以从私有 git 仓库安装：

```powershell
cargo install --git ssh://git@github.com/tonyzhoup/heel.git --branch feat/windows-support heel --force
heel --version
```

如果应用通过子进程调用 Heel CLI，测试前确认 `heel.exe` 在 `PATH` 上，或者在配置中使用绝对路径。

## 5. Windows 快速验证命令

以下命令用于在应用项目机器上确认 Heel 基本可用。

### 5.1 命令执行

```powershell
$Root = Join-Path $env:TEMP "heel-app-smoke"
New-Item -ItemType Directory -Force $Root | Out-Null
heel run --working-dir $Root cmd.exe /C "echo heel-ok>hello.txt && type hello.txt"
```

预期输出包含：

```text
heel-ok
```

### 5.2 Python 无网络执行

```powershell
$Root = Join-Path $env:TEMP "heel-app-python"
New-Item -ItemType Directory -Force $Root | Out-Null
$Probe = Join-Path $Root "probe.py"
@"
import socket
from pathlib import Path

Path("python-ok.txt").write_text("python-ok", encoding="utf-8")
print(Path("python-ok.txt").read_text(encoding="utf-8"))

try:
    socket.create_connection(("example.com", 80), timeout=5)
    print("NETWORK_ALLOWED")
except OSError as exc:
    print("NETWORK_DENIED:" + exc.__class__.__name__)
"@ | Set-Content -Encoding UTF8 $Probe

heel python --working-dir $Root $Probe
```

预期：

- 输出包含 `python-ok`
- 输出包含 `NETWORK_DENIED`
- 可能出现 `Failed to find real location of ...\python.exe`

最后一条是 CPython 3.14 在 AppContainer 内的已知 stderr warning，不代表 sandbox 失败。

### 5.3 Python venv

```powershell
$Root = Join-Path $env:TEMP "heel-app-venv"
$Sandbox = Join-Path $Root "sandbox"
$Venv = Join-Path $Root "venv"
New-Item -ItemType Directory -Force $Sandbox | Out-Null
$Probe = Join-Path $Sandbox "probe.py"
@"
import sys
print("EXE=" + sys.executable)
print("PREFIX=" + sys.prefix)
"@ | Set-Content -Encoding UTF8 $Probe

heel python --working-dir $Sandbox --venv $Venv $Probe
```

预期：

- `EXE=` 指向 `$Venv\Scripts\python.exe`
- `PREFIX=` 指向 `$Venv`
- 不需要显式传 `--python`；Heel 会跳过 Microsoft Store `WindowsApps` alias 并解析真实 Python。

## 6. 应用侧开发建议

应用要运行 agent 生成的 Python 代码时，建议流程如下：

1. 为每次 agent 任务创建独立 working dir。
2. 把 agent 生成的脚本写入 working dir。
3. 需要读取的输入文件用 `--readable` 显式授权。
4. 需要输出到工作区外部时，用 `--writable` 显式授权目标目录。
5. 默认不传 `--network allow` 或 `--network allow-list`。
6. package 安装、模型下载、依赖准备都在宿主侧完成，不在 Heel sandbox 内执行。
7. 执行后只信任 working dir 或显式 writable 目录中的产物。

CLI 示例：

```powershell
heel python `
  --working-dir C:/path/to/task-workdir `
  --readable C:/path/to/input `
  --writable C:/path/to/output `
  --venv C:/path/to/prepared-venv `
  C:/path/to/task-workdir/main.py
```

Rust 库侧建议用 `SandboxConfig` 明确描述边界，避免把用户目录整体加入 readable/writable/executable。

## 7. 应用项目中的最小测试矩阵

把 Heel 接进应用后，至少保留这些测试或手工验证：

| 场景 | 预期 |
| --- | --- |
| 普通命令执行 | `heel run` 在 working dir 内创建文件成功 |
| 默认文件隔离 | 未授权外部文件读取失败 |
| 默认写隔离 | 未授权外部路径写入失败，且没有残留文件 |
| Python 脚本 | `heel python` 输出预期结果 |
| Python venv | `heel python --venv` 能创建/使用 venv，并从 venv `site-packages` import |
| 网络拒绝 | Python `socket.create_connection` 或 HTTP 请求失败 |
| 进程清理 | 后台子进程不会在 Heel 退出后继续运行 |

Windows 上 `--network allow`、`--network allow-list` 和 IPC 当前应 fail closed。应用不要依赖这些能力。

## 8. Codex 开发约束

让 Codex 在应用项目中开发 Heel 集成时，应遵守：

- 优先用 private git `rev` 或本地 `path` 接入 Heel，不要直接依赖 crates.io `heel = "0.1.1"`，因为 crates.io 上的 `0.1.1` 不包含当前 Windows first-phase 改动。
- Windows 测试必须覆盖 `heel python` 和 `heel python --venv`。
- 网络测试必须确认 sandbox 内请求失败，且宿主网络正向控制可用。
- 不要为了让 Python 运行而放宽整个用户目录。
- 不要把 CPython realpath warning 当成失败条件；应以退出码和隔离断言判断。
- 如果应用要在 CI 上构建，先确认 CI 能读取 `ssh://git@github.com/tonyzhoup/heel.git`。

## 9. 另一台机器上的常见问题

### Cargo 找不到私有仓库

先确认 Git 本身能访问：

```powershell
git ls-remote ssh://git@github.com/tonyzhoup/heel.git
```

如果 Git 能访问但 Cargo 失败，添加：

```toml
# .cargo/config.toml
[net]
git-fetch-with-cli = true
```

### `heel python --venv` 创建失败

确认机器上有真实 Python 安装，而不是只有 Microsoft Store alias：

```powershell
Get-Command python.exe -All
Get-ChildItem (Join-Path $env:LOCALAPPDATA "Programs\Python") -Recurse -Filter python.exe
```

当前 Heel 会跳过 `WindowsApps\python*.exe` alias，但机器上仍需要安装真实 Python。

### 看到 CPython realpath warning

类似输出：

```text
Failed to find real location of C:\...\python.exe
```

这是 CPython 在 AppContainer 内调用 Windows realpath API 的已知 stderr warning。只要命令退出码为 0，且隔离断言通过，就不算失败。

### 应用已经锁定旧的 Heel

如果应用项目使用 `Cargo.lock`，更新 Heel 依赖后运行：

```powershell
cargo update -p heel
```

然后重新运行应用侧测试。

## 10. 发布判断

当前不必为了应用本地集成立刻发布 crates.io。

推荐顺序：

1. 应用先用 private git `rev` 或本地 `path` 集成。
2. 跑真实 agent Python 任务。
3. 确认 Windows first-phase 稳定后，再考虑把 Heel 升到 `0.1.2` 或 `0.2.0` 并发布 crates.io。
