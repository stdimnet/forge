//! CMSIS-SVD device descriptions for the Peripherals view of the Debug tool window:
//! peripherals (with `derivedFrom`), registers and clusters (with `dim` arrays), fields
//! (`bitOffset`/`bitWidth`, `bitRange`, `lsb`/`msb`) and enumerated values, with the size,
//! access and reset value inherited from the device and peripheral. [`read_ranges`] says
//! which bytes to read through the debugger and [`decode`] turns them into register and field
//! values.

use serde::{Deserialize, Serialize};

use cforge_analyzers::xml::{self, Element};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumValue {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub value: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    /// `read-write`, `read-only`, `write-only`, `writeOnce`, `read-writeOnce`.
    pub access: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<EnumValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Register {
    /// `MODER`, `CCR1`, `DMA.DCR` (registers of clusters are prefixed).
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Bytes from the peripheral's base address.
    pub offset: u64,
    /// Bits.
    pub size: u32,
    pub access: String,
    pub reset_value: u64,
    /// Reading has a side effect (`readAction`, e.g. `clear`): not read by the view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_action: Option<String>,
    pub fields: Vec<Field>,
}

impl Register {
    /// Whether the view may read it.
    pub fn readable(&self) -> bool {
        self.access != "write-only" && self.access != "writeOnce" && self.read_action.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peripheral {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    pub base_address: u64,
    pub registers: Vec<Register>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cpu: String,
    pub width: u32,
    pub peripherals: Vec<Peripheral>,
}

/// SVD's scaledNonNegativeInteger: decimal, `0x…`, `0b…` or `#…` (binary).
pub fn number(text: &str) -> Option<u64> {
    let t = text.trim().replace('_', "");
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        return u64::from_str_radix(h, 16).ok();
    }
    if let Some(b) = lower.strip_prefix("0b").or_else(|| lower.strip_prefix('#')) {
        return u64::from_str_radix(b, 2).ok();
    }
    // Scaled values (`4k`) are rare in register descriptions.
    if let Some(k) = lower.strip_suffix('k') {
        return k.parse::<u64>().ok().map(|v| v * 1024);
    }
    lower.parse().ok()
}

fn text(el: &Element, name: &str) -> String {
    el.child_text(name).unwrap_or("").to_string()
}

fn num(el: &Element, name: &str) -> Option<u64> {
    el.child_text(name).and_then(number)
}

/// Properties registers inherit (device → peripheral → cluster → register).
#[derive(Debug, Clone)]
struct Defaults {
    size: u32,
    access: String,
    reset_value: u64,
}

impl Defaults {
    fn over(&self, el: &Element) -> Defaults {
        Defaults {
            size: num(el, "size").map(|v| v as u32).unwrap_or(self.size),
            access: el
                .child_text("access")
                .map(str::to_string)
                .unwrap_or_else(|| self.access.clone()),
            reset_value: num(el, "resetValue").unwrap_or(self.reset_value),
        }
    }
}

/// The names and offsets of a `dim` element: `CCR%s` with dimIndex `1-4` → CCR1…CCR4.
fn dims(el: &Element, name: &str) -> Vec<(String, u64)> {
    let Some(count) = num(el, "dim").filter(|d| *d > 0) else {
        return vec![(name.to_string(), 0)];
    };
    let step = num(el, "dimIncrement").unwrap_or(0);
    let index = el.child_text("dimIndex").unwrap_or("");
    let names: Vec<String> = if let Some((a, b)) = index.split_once('-') {
        match (a.trim().parse::<u64>(), b.trim().parse::<u64>()) {
            (Ok(a), Ok(b)) if b >= a => (a..=b).map(|i| i.to_string()).collect(),
            _ => {
                // Letters: A-D.
                let (a, b) = (a.trim().chars().next(), b.trim().chars().next());
                match (a, b) {
                    (Some(a), Some(b)) if b >= a => (a..=b).map(|c| c.to_string()).collect(),
                    _ => Vec::new(),
                }
            }
        }
    } else if index.contains(',') {
        index.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        (0..count).map(|i| i.to_string()).collect()
    };
    names
        .into_iter()
        .take(count as usize)
        .enumerate()
        .map(|(i, n)| {
            let expanded = if name.contains("[%s]") {
                name.replace("[%s]", &n)
            } else {
                name.replace("%s", &n)
            };
            (expanded, i as u64 * step)
        })
        .collect()
}

fn fields(el: &Element, reg_access: &str) -> Vec<Field> {
    let Some(fields) = el.child("fields") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for f in fields.children_named("field") {
        let (offset, width) = if let Some(o) = num(f, "bitOffset") {
            (o as u32, num(f, "bitWidth").unwrap_or(1) as u32)
        } else if let Some(r) = f.child_text("bitRange") {
            let r = r.trim_matches(['[', ']']);
            let (msb, lsb) = r.split_once(':').unwrap_or((r, r));
            let (msb, lsb) = (
                number(msb).unwrap_or(0) as u32,
                number(lsb).unwrap_or(0) as u32,
            );
            (lsb.min(msb), msb.abs_diff(lsb) + 1)
        } else {
            let lsb = num(f, "lsb").unwrap_or(0) as u32;
            let msb = num(f, "msb").unwrap_or(lsb as u64) as u32;
            (lsb, msb.saturating_sub(lsb) + 1)
        };
        let values: Vec<EnumValue> = f
            .child("enumeratedValues")
            .map(|ev| {
                ev.children_named("enumeratedValue")
                    .filter_map(|v| {
                        Some(EnumValue {
                            name: text(v, "name"),
                            description: text(v, "description"),
                            // Values with don't-care bits (`#1x`) are left out.
                            value: num(v, "value")?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let access = f.child_text("access").unwrap_or(reg_access).to_string();
        let description = text(f, "description");
        for (name, step) in dims(f, &text(f, "name")) {
            out.push(Field {
                name,
                description: description.clone(),
                bit_offset: offset + step as u32,
                bit_width: width.max(1),
                access: access.clone(),
                values: values.clone(),
            });
        }
    }
    out.sort_by(|a, b| b.bit_offset.cmp(&a.bit_offset));
    out
}

/// Registers and clusters of `el` (a `registers` or `cluster` element).
fn registers(el: &Element, defaults: &Defaults, base: u64, prefix: &str, out: &mut Vec<Register>) {
    for c in &el.children {
        match c.name.as_str() {
            "register" => {
                let d = defaults.over(c);
                let offset = num(c, "addressOffset").unwrap_or(0);
                let fields = fields(c, &d.access);
                for (name, step) in dims(c, &text(c, "name")) {
                    out.push(Register {
                        name: format!("{prefix}{name}"),
                        description: text(c, "description"),
                        offset: base + offset + step,
                        size: d.size,
                        access: d.access.clone(),
                        reset_value: d.reset_value,
                        read_action: c.child_text("readAction").map(str::to_string),
                        fields: fields.clone(),
                    });
                }
            }
            "cluster" => {
                let d = defaults.over(c);
                let offset = num(c, "addressOffset").unwrap_or(0);
                for (name, step) in dims(c, &text(c, "name")) {
                    registers(
                        c,
                        &d,
                        base + offset + step,
                        &format!("{prefix}{name}."),
                        out,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Parse an SVD file.
pub fn parse(text: &str) -> Result<Device, String> {
    let root = xml::parse(text)?;
    if root.name != "device" {
        return Err("not a CMSIS-SVD file (no <device>)".into());
    }
    let device_defaults = Defaults {
        size: 32,
        access: "read-write".into(),
        reset_value: 0,
    }
    .over(&root);
    let mut device = Device {
        name: self::text(&root, "name"),
        vendor: self::text(&root, "vendor"),
        description: self::text(&root, "description"),
        cpu: root
            .child("cpu")
            .map(|c| self::text(c, "name"))
            .unwrap_or_default(),
        width: num(&root, "width").unwrap_or(32) as u32,
        peripherals: Vec::new(),
    };
    let Some(list) = root.child("peripherals") else {
        return Ok(device);
    };
    let elements: Vec<&Element> = list.children_named("peripheral").collect();
    for p in &elements {
        let d = device_defaults.over(p);
        let mut regs = Vec::new();
        if let Some(r) = p.child("registers") {
            registers(r, &d, 0, "", &mut regs);
        }
        regs.sort_by(|a, b| a.offset.cmp(&b.offset).then(a.name.cmp(&b.name)));
        device.peripherals.push(Peripheral {
            name: self::text(p, "name"),
            description: self::text(p, "description"),
            group: self::text(p, "groupName"),
            base_address: num(p, "baseAddress").unwrap_or(0),
            registers: regs,
        });
    }
    // derivedFrom: the base peripheral's registers (and texts) unless given.
    for (i, p) in elements.iter().enumerate() {
        let Some(from) = p.attr("derivedFrom") else {
            continue;
        };
        let Some(base) = device.peripherals.iter().find(|x| x.name == from).cloned() else {
            continue;
        };
        let target = &mut device.peripherals[i];
        if target.registers.is_empty() {
            target.registers = base.registers;
        }
        if target.description.is_empty() {
            target.description = base.description;
        }
        if target.group.is_empty() {
            target.group = base.group;
        }
    }
    Ok(device)
}

/// A peripheral in the view's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeripheralSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    pub base_address: u64,
    pub registers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub peripherals: Vec<PeripheralSummary>,
}

impl Device {
    pub fn summary(&self) -> DeviceSummary {
        DeviceSummary {
            name: self.name.clone(),
            vendor: self.vendor.clone(),
            description: self.description.clone(),
            peripherals: self
                .peripherals
                .iter()
                .map(|p| PeripheralSummary {
                    name: p.name.clone(),
                    description: p.description.clone(),
                    group: p.group.clone(),
                    base_address: p.base_address,
                    registers: p.registers.len(),
                })
                .collect(),
        }
    }

    pub fn peripheral(&self, name: &str) -> Option<&Peripheral> {
        self.peripherals.iter().find(|p| p.name == name)
    }
}

/// Byte ranges (offset from the base, length) to read for a peripheral: adjacent readable
/// registers are read together; registers that must not be read are skipped.
pub fn read_ranges(p: &Peripheral) -> Vec<(u64, u32)> {
    let mut out: Vec<(u64, u32)> = Vec::new();
    for r in p.registers.iter().filter(|r| r.readable()) {
        let len = r.size.div_ceil(8).max(1);
        match out.last_mut() {
            Some((start, l)) if *start + *l as u64 == r.offset && *l + len <= 1024 => *l += len,
            Some((start, l))
                if r.offset >= *start && r.offset + len as u64 <= *start + *l as u64 => {}
            _ => out.push((r.offset, len)),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldValue {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// `[11:10]` or `[5]`.
    pub bits: String,
    pub access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
    /// `Output (1)`, `0x1F (31)`, `1`.
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterValue {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub address: u64,
    pub size: u32,
    pub access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
    /// `0x00000400`, or why there is no value (`write-only`, `not read`).
    pub display: String,
    pub reset_value: u64,
    pub fields: Vec<FieldValue>,
}

fn hex(v: u64, bits: u32) -> String {
    format!("0x{:0width$X}", v, width = bits.div_ceil(4).max(1) as usize)
}

fn field_display(f: &Field, v: u64) -> String {
    if let Some(e) = f.values.iter().find(|e| e.value == v) {
        return format!("{} ({v})", e.name);
    }
    if f.bit_width == 1 {
        return v.to_string();
    }
    format!("{} ({v})", hex(v, f.bit_width))
}

/// Register and field values of a peripheral from bytes read at `base + offset` (one block
/// per range of [`read_ranges`]; little endian).
pub fn decode(p: &Peripheral, blocks: &[(u64, Vec<u8>)]) -> Vec<RegisterValue> {
    let value_of = |r: &Register| -> Option<u64> {
        let len = r.size.div_ceil(8).max(1) as u64;
        blocks.iter().find_map(|(start, bytes)| {
            let from = r.offset.checked_sub(*start)?;
            let slice = bytes.get(from as usize..(from + len) as usize)?;
            Some(
                slice
                    .iter()
                    .rev()
                    .fold(0u64, |acc, b| (acc << 8) | *b as u64),
            )
        })
    };
    p.registers
        .iter()
        .map(|r| {
            let value = if r.readable() { value_of(r) } else { None };
            let display = match value {
                Some(v) => hex(v, r.size),
                None if r.access == "write-only" || r.access == "writeOnce" => "write-only".into(),
                None if r.read_action.is_some() => "not read (read has side effects)".into(),
                None => "not readable".into(),
            };
            RegisterValue {
                name: r.name.clone(),
                description: r.description.clone(),
                address: p.base_address + r.offset,
                size: r.size,
                access: r.access.clone(),
                value,
                display,
                reset_value: r.reset_value,
                fields: r
                    .fields
                    .iter()
                    .map(|f| {
                        let v = value.map(|v| {
                            let mask = if f.bit_width >= 64 {
                                u64::MAX
                            } else {
                                (1u64 << f.bit_width) - 1
                            };
                            (v >> f.bit_offset) & mask
                        });
                        FieldValue {
                            name: f.name.clone(),
                            description: f.description.clone(),
                            bits: if f.bit_width == 1 {
                                format!("[{}]", f.bit_offset)
                            } else {
                                format!("[{}:{}]", f.bit_offset + f.bit_width - 1, f.bit_offset)
                            },
                            access: f.access.clone(),
                            display: v.map(|v| field_display(f, v)).unwrap_or_default(),
                            value: v,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVD: &str = include_str!("../tests/fixtures/stm32f4-mini.svd");

    #[test]
    fn parses_peripherals_registers_fields() {
        let d = parse(SVD).unwrap();
        assert_eq!(d.name, "STM32F401");
        assert_eq!(d.vendor, "STMicroelectronics");
        assert_eq!(d.cpu, "CM4");
        let names: Vec<_> = d.peripherals.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["GPIOA", "GPIOB", "RCC", "USART2", "TIM2"]);
        let gpioa = d.peripheral("GPIOA").unwrap();
        assert_eq!(gpioa.base_address, 0x4002_0000);
        assert_eq!(gpioa.group, "GPIO");
        let moder = &gpioa.registers[0];
        assert_eq!(
            (moder.name.as_str(), moder.offset, moder.size),
            ("MODER", 0, 32)
        );
        assert_eq!(moder.reset_value, 0xA800_0000);
        // Fields from the top bit down; bitRange works.
        assert_eq!(moder.fields[0].name, "MODER5");
        assert_eq!(
            (moder.fields[0].bit_offset, moder.fields[0].bit_width),
            (10, 2)
        );
        assert_eq!(moder.fields[0].values.len(), 4);
        assert_eq!(moder.fields[0].values[3].value, 3);
        assert_eq!(
            (moder.fields[1].bit_offset, moder.fields[1].bit_width),
            (0, 2)
        );
        let idr = &gpioa.registers[1];
        assert_eq!(idr.access, "read-only");
        assert_eq!(idr.fields.len(), 16);
        assert_eq!(idr.fields[0].name, "IDR15");
        assert_eq!(idr.fields[0].bit_offset, 15);
        let odr = &gpioa.registers[2];
        assert_eq!((odr.fields[0].bit_offset, odr.fields[0].bit_width), (5, 1));
        assert!(!gpioa.registers[3].readable(), "BSRR is write-only");
        // derivedFrom copies registers and the group.
        let gpiob = d.peripheral("GPIOB").unwrap();
        assert_eq!(gpiob.base_address, 0x4002_0400);
        assert_eq!(gpiob.registers.len(), 4);
        assert_eq!(gpiob.group, "GPIO");
        // Register arrays and clusters.
        let tim = d.peripheral("TIM2").unwrap();
        let names: Vec<_> = tim
            .registers
            .iter()
            .map(|r| (r.name.as_str(), r.offset))
            .collect();
        assert_eq!(
            names,
            [
                ("CCR1", 0x34),
                ("CCR2", 0x38),
                ("CCR3", 0x3C),
                ("CCR4", 0x40),
                ("DMA.DCR", 0x48),
                ("DMA.DMAR", 0x4C)
            ]
        );
        assert_eq!(tim.registers[4].size, 16);
        // The peripheral's size (16 bits) applies to USART2's registers.
        assert_eq!(d.peripheral("USART2").unwrap().registers[0].size, 16);
        let s = d.summary();
        assert_eq!(s.peripherals[0].registers, 4);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["peripherals"][0]["baseAddress"], 0x4002_0000u64);
    }

    #[test]
    fn decodes_register_values() {
        let d = parse(SVD).unwrap();
        let gpioa = d.peripheral("GPIOA").unwrap();
        let ranges = read_ranges(gpioa);
        // MODER alone, then IDR + ODR; BSRR (write-only) is not read.
        assert_eq!(ranges, [(0, 4), (0x10, 8)]);
        let blocks = vec![
            (0u64, 0xA800_0400u32.to_le_bytes().to_vec()),
            (
                0x10u64,
                [0x2000u32.to_le_bytes(), 0x0020u32.to_le_bytes()].concat(),
            ),
        ];
        let values = decode(gpioa, &blocks);
        let moder = &values[0];
        assert_eq!(moder.address, 0x4002_0000);
        assert_eq!(moder.display, "0xA8000400");
        assert_eq!(moder.fields[0].bits, "[11:10]");
        assert_eq!(moder.fields[0].display, "Output (1)");
        let idr = &values[1];
        assert_eq!(idr.value, Some(0x2000));
        assert_eq!(idr.fields[2].name, "IDR13");
        assert_eq!(idr.fields[2].display, "1");
        let odr = &values[2];
        assert_eq!(odr.fields[0].value, Some(1));
        assert_eq!(odr.fields[0].bits, "[5]");
        assert_eq!(values[3].display, "write-only");
        assert!(values[3].fields[0].value.is_none());
        let tim = d.peripheral("TIM2").unwrap();
        let dcr = decode(tim, &[(0x48, vec![0x05, 0x03, 0, 0, 0, 0, 0, 0])]);
        let dcr = dcr.iter().find(|r| r.name == "DMA.DCR").unwrap();
        assert_eq!(dcr.display, "0x0305");
        assert_eq!(dcr.fields[0].display, "0x03 (3)");
        assert_eq!(dcr.fields[1].display, "0x05 (5)");
    }

    #[test]
    fn numbers_and_errors() {
        assert_eq!(number("0x1F"), Some(31));
        assert_eq!(number("#101"), Some(5));
        assert_eq!(number("0b11"), Some(3));
        assert_eq!(number("12"), Some(12));
        assert_eq!(number("#1x"), None);
        assert!(parse("<other/>").is_err());
    }
}
