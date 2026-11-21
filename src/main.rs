mod protocol;

use clap::{Parser, Subcommand};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(version, about = "Control an ISDT MP305A or MP305B over USB HID")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true, help = "Device identifier from list")]
    device: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected USB HID candidates.
    List,
    /// Read model and firmware versions.
    Info,
    /// Read output state and telemetry.
    Status,
    /// Set voltage and current with output disabled.
    Set {
        #[arg(long, value_parser = voltage)]
        voltage: u16,
        #[arg(long, value_parser = current)]
        current: u16,
    },
    /// Enable output using the existing setpoints.
    On,
    /// Disable output using the existing setpoints.
    Off,
}

#[derive(Debug)]
struct Error(&'static str, String);
type Result<T> = std::result::Result<T, Error>;

fn error(code: &'static str, message: impl ToString) -> Error {
    Error(code, message.to_string())
}

fn parse_units(input: &str, scale: u16, max: u16) -> std::result::Result<u16, String> {
    let (whole, fraction) = input.split_once('.').unwrap_or((input, ""));
    let precision = scale.ilog10() as usize;
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit()) || fraction.len() > precision
    {
        return Err(format!("expected a nonnegative decimal with up to {precision} decimal places"));
    }
    let whole = whole.parse::<u16>().map_err(|e| e.to_string())?;
    let fraction_units = fraction.parse::<u16>().unwrap_or(0)
        * 10u16.pow((precision - fraction.len()) as u32);
    whole.checked_mul(scale).and_then(|n| n.checked_add(fraction_units))
        .filter(|&value| value <= max)
        .ok_or_else(|| format!("value exceeds {}", f64::from(max) / f64::from(scale)))
}

fn voltage(input: &str) -> std::result::Result<u16, String> {
    parse_units(input, 100, 3000)
}

fn current(input: &str) -> std::result::Result<u16, String> {
    parse_units(input, 1000, 5000)
}

fn device_id(device: &DeviceInfo) -> String {
    let mut id = String::new();
    use std::fmt::Write;
    for byte in device.path().to_bytes() {
        write!(id, "{byte:02x}").unwrap();
    }
    id
}

fn request(device: &HidDevice, command: u8, data: &[u8]) -> Result<Vec<u8>> {
    let report = protocol::encode(command, data);
    let bytes = device.write(&report).map_err(|e| error("usb_write", e))?;
    // hidapi's Windows native backend returns 0 when WriteFile completes synchronously.
    // A backend can omit trailing padding. The complete protocol frame must fit.
    let frame_end = usize::from(report[1]) + 2;
    if bytes < frame_end && !(cfg!(windows) && bytes == 0) {
        return Err(error("usb_write", format!("short HID write: {bytes} bytes")));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buffer = [0; protocol::REPORT_SIZE];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(error("timeout", format!("waiting for response to {command:02X}")));
        }
        let n = device.read_timeout(&mut buffer, remaining.as_millis().max(1) as i32)
            .map_err(|e| error("usb_read", e))?;
        if n == 0 {
            continue;
        }
        let (reply, data) = protocol::decode(&buffer[..n])
            .map_err(|e| error("invalid_report", e))?;
        if reply == command + 1 {
            return Ok(data);
        }
    }
}

fn info(device: &HidDevice) -> Result<Value> {
    let data = request(device, 0xe0, &[])?;
    parse_info(&data)
}

fn supported_model(model: &str) -> bool {
    matches!(model, "MP305A" | "MP305B")
}

fn hid_candidate(vid: u16, pid: u16, page: u16, usage: u16, product: Option<&str>) -> bool {
    vid == 0x28e9
        && (pid == 0x028a || (page == 1 && usage == 4)
            || product.is_some_and(|name| supported_model(name.trim())))
}

fn parse_info(data: &[u8]) -> Result<Value> {
    if data.len() < 30 {
        return Err(error("invalid_report", "hardware information is truncated"));
    }
    let model = String::from_utf8_lossy(&data[20..30]).trim_end_matches('\0').to_owned();
    if !supported_model(&model) {
        return Err(error("unsupported_device", format!("expected MP305A or MP305B, received {model}")));
    }
    let version = |offset: usize| {
        data[offset..offset + 4].iter().map(u8::to_string).collect::<Vec<_>>().join(".")
    };
    Ok(json!({"model": model, "hardware_version": version(8),
        "boot_version": version(12), "firmware_version": version(16)}))
}

struct State(Vec<u8>);

impl State {
    fn read(device: &HidDevice) -> Result<Self> {
        let data = request(device, 0xc2, &[])?;
        if data.len() < 26 || data[24] > 1 {
            return Err(error("invalid_report", "invalid state payload"));
        }
        Ok(Self(data))
    }

    fn word(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.0[offset], self.0[offset + 1]])
    }

    fn settings(&self) -> (u16, u16) {
        (self.word(5), self.word(9))
    }

    fn output(&self) -> bool {
        self.0[24] == 1
    }

    fn dc_only(&self) -> Result<()> {
        if self.0[25] != 0 {
            return Err(error("unsupported_mode", "control requires DC mode; use the front panel"));
        }
        Ok(())
    }

    fn json(&self) -> Value {
        json!({
            "voltage_v": f64::from(self.word(3)) / 100.0,
            "current_a": f64::from(self.word(7)) / 1000.0,
            "power_w": f64::from(self.word(19)) / 100.0,
            "set_voltage_v": f64::from(self.word(5)) / 100.0,
            "current_limit_a": f64::from(self.word(9)) / 1000.0,
            "output_enabled": self.output(),
            "mode": self.0[25]
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    device_id: String,
    max_voltage_v: String,
    max_current_a: String,
}

fn check_limits(id: &str, model: &str, (v, a): (u16, u16)) -> Result<()> {
    let path = std::env::current_exe().map_err(|e| error("invalid_limits", e))?
        .with_file_name("mp305-limits.json");
    let bytes = std::fs::read(&path).or_else(|e| {
        if e.kind() == std::io::ErrorKind::NotFound && model == "MP305A" {
            let legacy = path.with_file_name("mp305a-limits.json");
            return std::fs::read(legacy);
        }
        Err(e)
    })
        .map_err(|e| error("invalid_limits", format!("{}: {e}", path.display())))?;
    let limits: Limits = serde_json::from_slice(&bytes).map_err(|e| error("invalid_limits", e))?;
    if limits.device_id != id {
        return Err(error("invalid_limits", "limits are assigned to a different device"));
    }
    let max_v = voltage(&limits.max_voltage_v).map_err(|e| error("invalid_limits", e))?;
    let max_a = current(&limits.max_current_a).map_err(|e| error("invalid_limits", e))?;
    if v > max_v || a > max_a {
        return Err(error("limit_exceeded", "voltage or current exceeds approved limits"));
    }
    Ok(())
}

fn control(device: &HidDevice, state: &State, remote: u8,
           (v, a): (u16, u16), flags: u8, output: bool) -> Result<()> {
    let [vl, vh] = v.to_le_bytes();
    let [al, ah] = a.to_le_bytes();
    let payload = [remote, vl, vh, al, ah, flags, state.0[23], state.0[21],
                   u8::from(output), 0, 0];
    let response = request(device, 0xc8, &payload)?;
    match response.first() {
        Some(0) => Ok(()),
        Some(code) => Err(error("device_rejected", format!("control status {code}"))),
        None => Err(error("invalid_report", "missing control response status")),
    }
}

fn state_unknown(e: Error) -> Error {
    error("state_unknown", format!("{}: {}. Verify the output on the front panel.", e.0, e.1))
}

fn change(device: &HidDevice, id: &str, model: &str, command: &Command) -> Result<Value> {
    let previous = State::read(device)?;
    previous.dc_only()?;
    let (settings, flags, output) = match *command {
        Command::Set { voltage, current } => {
            if previous.output() {
                return Err(error("output_enabled", "turn off output before changing setpoints"));
            }
            ((voltage, current), 3, false)
        }
        Command::On => (previous.settings(), 0, true),
        Command::Off => (previous.settings(), 0, false),
        _ => unreachable!(),
    };
    let turning_off = matches!(command, Command::Off);
    if !turning_off {
        check_limits(id, model, settings)?;
    }
    if flags == 0 && previous.output() == output {
        return Ok(previous.json());
    }
    control(device, &previous, 2, previous.settings(), 0, previous.output()).map_err(state_unknown)?;
    let latest = State::read(device).map_err(state_unknown)?;
    latest.dc_only()?;
    if !turning_off && (latest.settings() != previous.settings() || latest.output() != previous.output()) {
        return Err(error("state_changed", "device changed while requesting remote control"));
    }
    let settings = if turning_off { latest.settings() } else { settings };
    control(device, &latest, 1, settings, flags, output).map_err(state_unknown)?;
    let after = State::read(device).map_err(state_unknown)?;
    if after.output() != output || after.settings() != settings || after.0[25] != 0 {
        return Err(error("verification_failed", format!("unexpected readback: {}", after.json())));
    }
    Ok(after.json())
}

fn lock() -> Result<File> {
    // Share the lock with earlier mp305a-cli builds.
    let path = std::env::temp_dir().join("mp305a-cli-usb.lock");
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)
        .map_err(|e| error("lock_failed", e))?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|e| error("device_busy", e))?;
    Ok(file)
}

fn run(cli: &Cli) -> Result<Value> {
    let api = HidApi::new().map_err(|e| error("usb_access", e))?;
    let devices: Vec<_> = api.device_list().filter(|device| hid_candidate(
        device.vendor_id(), device.product_id(), device.usage_page(),
        device.usage(), device.product_string(),
    )).collect();
    if matches!(cli.command, Command::List) {
        return Ok(json!(devices.iter().map(|device| json!({"id": device_id(device),
            "product": device.product_string(), "usage_page": device.usage_page(),
            "usage": device.usage()})).collect::<Vec<_>>()));
    }
    let selected = match (&cli.device, devices.as_slice()) {
        (Some(id), _) => devices.iter().copied().find(|device| device_id(device) == *id),
        (None, [device]) => Some(*device),
        (None, []) => None,
        _ => return Err(error("multiple_devices", "run list, then specify --device <id>")),
    }.ok_or_else(|| error("device_not_found", "requested MP305 HID interface was not found"))?;
    let _lock = lock()?;
    let device = selected.open_device(&api).map_err(|e| error("usb_access", e))?;
    let identity = info(&device)?;
    match cli.command {
        Command::Info => Ok(identity),
        Command::Status => Ok(State::read(&device)?.json()),
        _ => change(&device, &device_id(selected), identity["model"].as_str()
            .expect("parse_info returns a validated model"), &cli.command),
    }
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(data) => {
            if cli.json {
                println!("{}", json!({"ok": true, "data": data}));
            } else {
                println!("{}", serde_json::to_string_pretty(&data).expect("valid JSON"));
            }
            std::process::ExitCode::SUCCESS
        }
        Err(Error(code, message)) => {
            if cli.json {
                println!("{}", json!({"ok": false, "error": {"code": code, "message": message}}));
            }
            eprintln!("{code}: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_both_models_and_rejects_other_devices() {
        for model in ["MP305A", "MP305B"] {
            let mut data = [0; 30];
            data[20..26].copy_from_slice(model.as_bytes());
            assert_eq!(parse_info(&data).unwrap()["model"], model);
        }
        assert_eq!(parse_info(&[0; 30]).unwrap_err().0, "unsupported_device");
        assert!(hid_candidate(0x28e9, 0x028a, 1, 0, None));
        assert!(hid_candidate(0x28e9, 0xffff, 1, 4, None));
        assert!(hid_candidate(0x28e9, 0xffff, 0, 0, Some("MP305B")));
        assert!(!hid_candidate(0x1234, 0x028a, 1, 4, Some("MP305B")));
        assert!(!hid_candidate(0x28e9, 0xffff, 0, 0, Some("Other device")));
    }

    #[test]
    fn setpoints_cannot_round_up_or_exceed_device_limits() {
        assert_eq!(voltage("24.00").unwrap(), 2400);
        assert_eq!(current("0.125").unwrap(), 125);
        for input in ["30.01", "5.001", "-1", "NaN", "1e1"] {
            assert!(voltage(input).is_err(), "{input}");
        }
        for input in ["5.001", "0.0009", "-1", "NaN", "1e0"] {
            assert!(current(input).is_err(), "{input}");
        }
    }
}
