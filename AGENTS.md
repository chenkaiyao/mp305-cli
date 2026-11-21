# MP305A 和 MP305B 电源调用说明

在连接 MP305A 或 MP305B 的本机运行 CLI。Windows 发布文件位于仓库的 `target\release\mp305-cli.exe`。如需构建，先运行 `cargo build --release --locked`。Linux 和 macOS 的实机支持仍待验证。

## 查询设备

从仓库根目录运行以下只读命令。

```powershell
.\target\release\mp305-cli.exe --json list
.\target\release\mp305-cli.exe --json info
.\target\release\mp305-cli.exe --json status
```

只有一台候选设备时 CLI 自动选择。多台时从 `list` 获取 ID，并在查询和写入中使用同一个 `--device <id>`。用 `info` 确认型号为 MP305A 或 MP305B。用 `status` 区分实测值 `voltage_v`、`current_a` 和设定值 `set_voltage_v`、`current_limit_a`，并检查 `output_enabled`。

## 控制输出

- 只有用户要求实际操作电源时才调用写入命令。电压和限流必须来自用户明确指定的值或该被测硬件已批准的测试配置。
- 上限由工位负责人维护，保存在 EXE 同目录的 `mp305-limits.json`。Agent 不得创建、修改或绕过上限配置来执行写入。
- 使用 `set --voltage <V> --current <A>` 前确认输出已关闭。使用 `on` 前先读取状态并确认设定值符合本次测试和工位上限。只有用户明确要求上电时才运行 `on`。
- `set`、`on` 和 `off` 仅适用于直流模式（`mode: 0`）。其他模式通过前面板操作。核对控制命令返回的状态，确认设定值和输出开关符合预期。

调用时使用 `--json`。成功需同时满足退出码为 0 和 `ok: true`。如果出现 `state_unknown` 或 `verification_failed`，停止后续写入，请用户通过电源面板确认实际输出。收到 `state_changed` 时先读状态，避免按旧设定重试。遇到保护拒绝时，报告原因并等待有效的工位配置或明确的测试参数。

普通代码测试运行 `cargo test --locked`。实机写入和通断验收须明确确认负载条件及测试参数。更多用法见 [README](README.md)。

## 在其他项目中使用

在连接电源的主机上安装 `mp305-cli`，将可执行文件所在目录加入 `PATH`，或在项目说明中填写该主机上的绝对路径。把以下内容加入目标项目根目录的 `AGENTS.md`。Agent 的终端需要运行在能访问电源 USB HID 接口的主机上。

长期部署时，由工位负责人把 `mp305-cli` 和 `mp305-limits.json` 放在同一个固定目录。CLI 从实际运行的可执行文件所在目录读取配置，项目工作目录可以不同。更换被测硬件时，即使 USB 电源未变，也须由负责人重新核对限值。

```markdown
## 调试台电源

通过本机的 `mp305-cli` 操作 ISDT MP305A 或 MP305B。
完整用法见 [mp305-cli Agent 调用说明](https://github.com/chenkaiyao/mp305-cli/blob/main/AGENTS.md)。

- 先运行 `mp305-cli --json list`。多个候选时，后续查询和控制使用同一个
  `--device <id>`。用 `mp305-cli --json info` 确认型号，用
  `mp305-cli --json status` 读取当前状态。
- 查询任务只调用只读命令。设值和上电须获得用户授权，并使用用户明确指定的电压和限流，
  或该被测硬件已批准的测试配置。
- 确认输出关闭后再执行 `set --voltage <V> --current <A>`。只有用户明确要求
  上电时才调用 `on`。工位负责人维护可执行文件同目录的 `mp305-limits.json`，
  Agent 不得修改或绕过保护上限。
- 检查退出码、JSON 的 `ok` 和回读状态。遇到 `state_unknown` 或
  `verification_failed`，停止后续写入，并通过电源面板确认实际状态。
```

如果多个项目共用同一台调试主机，也可以把通用说明放到 Codex 的用户级 `~/.codex/AGENTS.md`，在各项目的 `AGENTS.md` 中补充被测硬件和已批准的测试参数。 [Codex 项目指令文档](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
