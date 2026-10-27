//! CForge's embedded development support, in the style of CLion. Pure Rust (no I/O), shared
//! by the desktop backend and the WebAssembly build of browser mode.
//!
//! - [`platformio`]: `platformio.ini` and the PlatformIO tasks.
//! - [`espidf`]: ESP-IDF projects and `idf.py`.
//! - [`zephyr`]: Zephyr applications and `west`.
//! - [`openocd`]: OpenOCD's GDB server and flashing.
//! - [`svd`]: CMSIS-SVD device files and register decoding (the Peripherals view).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod espidf;
pub mod openocd;
pub mod platformio;
pub mod svd;
pub mod zephyr;

/// A program to start, with arguments, working folder and extra environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// The embedded frameworks of a project, for the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedKinds {
    pub platformio: bool,
    pub esp_idf: bool,
    pub zephyr: bool,
}

/// Which embedded frameworks a project uses: `exists` checks project files, `cmake_lists` is
/// the root CMakeLists.txt.
pub fn detect(exists: &dyn Fn(&str) -> bool, cmake_lists: Option<&str>) -> EmbeddedKinds {
    EmbeddedKinds {
        platformio: exists("platformio.ini"),
        esp_idf: espidf::detect(exists, cmake_lists),
        zephyr: zephyr::detect(exists, cmake_lists),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_frameworks() {
        let k = super::detect(&|p| p == "platformio.ini", None);
        assert!(k.platformio && !k.esp_idf && !k.zephyr);
        let k = super::detect(&|p| p == "west.yml", None);
        assert!(k.zephyr);
    }
}
