# MP305A 和 MP305B 电源调用说明

在连接 MP305A 或 MP305B 的本机运行 CLI。Windows 发布文件位于仓库的 `target\release\mp305-cli.exe`。如需构建，先运行 `cargo build --release --locked`。Linux 和 macOS 的实机支持仍待验证。

## 查询设备

从仓库根目录运行以下只读命令。

```powershell
.\target\release\mp305-cli.exe --json list
.\target\release\mp305-cli.exe --json info
.\target\release\mp305-cli.exe --json status
```

只有一台候选设备时 CLI 自动选择。多台时从 `list` 获取 ID，给后续命令加上 `--device <id>`。确认型号为 MP305A 或 MP305B，并区分实测值 `voltage_v`、`current_a` 和设定值 `set_voltage_v`、`current_limit_a`。

## 控制输出

- 只有用户要求实际操作电源时才调用写入命令。电压和限流必须来自用户明确指定的值或该被测硬件已批准的测试配置。
- 上限由工位负责人维护，保存在 EXE 同目录的 `mp305-limits.json`。Agent 不得创建、修改或绕过上限配置来执行写入。
- 使用 `set --voltage <V> --current <A>` 前确认输出已关闭。使用 `on` 前先读取状态并确认设定值符合本次测试和工位上限。只有用户明确要求上电时才运行 `on`。
- 使用 `off` 关闭直流输出。任何写入后都要核对返回的状态，确认设定值和输出开关符合预期。

调用时使用 `--json`。成功需同时满足退出码为 0 和 `ok: true`。如果出现 `state_unknown` 或 `verification_failed`，停止后续写入，请用户通过电源面板确认实际输出。遇到保护拒绝时，报告原因并等待有效的工位配置或明确的测试参数。

普通代码测试运行 `cargo test --locked`。实机写入和通断验收须明确确认负载条件及测试参数。更多用法见 [README](README.md)。
