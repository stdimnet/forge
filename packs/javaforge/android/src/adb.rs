//! adb and emulator output: `adb devices -l`, `emulator -list-avds`, `adb shell ps` and the
//! AVD name of a running emulator (`adb -s emulator-5554 emu avd name`).

use serde::Serialize;

/// A device or emulator adb knows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub serial: String,
    /// `device` (online), `offline`, `unauthorized`, `recovery`, `sideload`, `bootloader`,
    /// `no permissions`, …
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
    /// `usb:1-1`
    pub usb: Option<String>,
    pub emulator: bool,
    /// The AVD of a running emulator (filled from `emu avd name`).
    pub avd: Option<String>,
}

impl Device {
    pub fn online(&self) -> bool {
        self.state == "device"
    }

    /// What the UI shows: `Pixel 8 API 35` (the AVD), `Pixel 7` (the model), else the serial.
    pub fn display_name(&self) -> String {
        if let Some(a) = &self.avd {
            return a.replace('_', " ");
        }
        match &self.model {
            Some(m) => m.replace('_', " "),
            None => self.serial.clone(),
        }
    }
}

/// Parse `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<Device> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb server")
        {
            continue;
        }
        // `serial  state  key:value …`; the state may be two words (`no permissions`).
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let rest: Vec<&str> = parts.collect();
        let split = rest
            .iter()
            .position(|p| p.contains(':'))
            .unwrap_or(rest.len());
        // `no permissions (user in plugdev group…); see [http://…]` has no key:value pairs.
        let state_words: Vec<&str> = rest[..split]
            .iter()
            .take_while(|w| !w.starts_with('('))
            .copied()
            .collect();
        let mut d = Device {
            serial: serial.to_string(),
            state: state_words.join(" "),
            emulator: serial.starts_with("emulator-"),
            ..Default::default()
        };
        for kv in &rest[split..] {
            let Some((k, v)) = kv.split_once(':') else {
                continue;
            };
            let v = Some(v.to_string());
            match k {
                "model" => d.model = v,
                "product" => d.product = v,
                "device" => d.device = v,
                "transport_id" => d.transport_id = v,
                "usb" => d.usb = v,
                _ => {}
            }
        }
        out.push(d);
    }
    out
}

/// Parse `emulator -list-avds` (log lines such as `INFO | …` are skipped).
pub fn parse_avds(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(" | ") && !l.contains(' '))
        .map(str::to_string)
        .collect()
}

/// The AVD name of `adb emu avd name` (first line; `OK` follows).
pub fn parse_avd_name(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "OK")
        .map(str::to_string)
}

/// Processes of `adb shell ps -A -o PID,NAME` (or the plain `ps` columns): (pid, name).
pub fn parse_ps(output: &str) -> Vec<(u32, String)> {
    let mut lines = output.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols: Vec<&str> = header.split_whitespace().collect();
    let pid_col = cols.iter().position(|c| *c == "PID");
    let name_col = cols
        .iter()
        .position(|c| *c == "NAME" || *c == "CMD" || *c == "ARGS");
    let (Some(pid_col), Some(name_col)) = (pid_col, name_col) else {
        return Vec::new();
    };
    lines
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let pid = f.get(pid_col)?.parse().ok()?;
            // NAME is the last column; with fewer fields (empty columns) take the last.
            let name = if name_col == cols.len() - 1 {
                f.last()?
            } else {
                f.get(name_col)?
            };
            Some((pid, name.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices() {
        let d = parse_devices(include_str!("../tests/fixtures/adb-devices.txt"));
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].serial, "emulator-5554");
        assert!(d[0].online() && d[0].emulator);
        assert_eq!(d[0].model.as_deref(), Some("sdk_gphone64_x86_64"));
        assert_eq!(d[0].transport_id.as_deref(), Some("1"));
        assert_eq!(d[1].serial, "R58N12ABCDE");
        assert_eq!(d[1].display_name(), "SM S918B");
        assert_eq!(d[1].usb.as_deref(), Some("1-1"));
        assert_eq!(d[2].state, "unauthorized");
        assert!(!d[2].online());
        assert_eq!(d[3].state, "no permissions");
        let mut e = d[0].clone();
        e.avd = Some("Pixel_8_API_35".into());
        assert_eq!(e.display_name(), "Pixel 8 API 35");
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert!(parse_devices("* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\n").is_empty());
    }

    #[test]
    fn avds_ps_and_names() {
        assert_eq!(
            parse_avds("INFO    | Storing crashdata in: /tmp/android-me/emu-crash-35.2.10.db\nPixel_8_API_35\nTablet_API_34\n"),
            ["Pixel_8_API_35", "Tablet_API_34"]
        );
        assert_eq!(
            parse_avd_name("Pixel_8_API_35\r\nOK\r\n").as_deref(),
            Some("Pixel_8_API_35")
        );
        let ps = parse_ps(include_str!("../tests/fixtures/adb-ps.txt"));
        assert!(ps.contains(&(12034, "com.acme.notes".to_string())));
        assert!(ps.contains(&(1, "init".to_string())));
        assert!(parse_ps("").is_empty());
    }
}
