# mp305a-cli

通过 USB HID 查询和控制 ISDT MP305A。Windows x64 是当前实机验收目标。Linux 和 macOS 共用相同的命令接口和协议解析。

## 构建

```powershell
cargo test
cargo build --release
```

Windows 发布产物位于 `target\release\mp305a-cli.exe`。Linux 和 macOS 产物为 `target/release/mp305a-cli`。

## 查询和设备选择

```powershell
mp305a-cli --json list
mp305a-cli --json info
mp305a-cli --json status
```

连接多台时，使用 `mp305a-cli --device <id> --json status` 指定 `list` 返回的目标。`id` 根据本机 HID 路径生成，换端口或换电脑后可能变化。

## 控制输出

负责人在 EXE 同目录维护 `mp305a-limits.json`，填写以下字段。部署时应通过文件权限限制修改者，并在更换被测硬件后核对上限。

| 字段 | 说明 |
| --- | --- |
| `device_id` | `list` 返回的目标设备 ID。 |
| `max_voltage_v` | 被测硬件允许的最高电压，使用十进制字符串。 |
| `max_current_a` | 被测硬件允许的最大电流，使用十进制字符串。 |

- `set --voltage <V> --current <A>` 在输出关闭时更新两项设定值。电压最多两位小数，限流最多三位小数。
- `on` 使用当前设定值。设备必须处于直流模式，且上限配置有效。
- `off` 关闭直流输出，保护配置缺失时仍可调用。其他模式请使用前面板。

每次写入都会回读确认。若 CLI 返回 `state_unknown` 或 `verification_failed`，请在前面板确认实际状态。当前控制流程取得远程控制后保留设备的远程控制状态，请按设备面板提示恢复本地操作。

`--json` 为成功返回 `ok` 和 `data`，为运行失败返回 `ok` 和 `error`。参数语法错误由命令行解析器通过 stderr 报告。

协议和验收条件见 [项目需求文档](docs/mp305a-cli%20%E9%9C%80%E6%B1%82%E6%96%87%E6%A1%A3.md)。

## 项目资料

- [Agent 调用说明](AGENTS.md)
- [更新日志](CHANGELOG.md)
- [MIT 许可证](LICENSE)
