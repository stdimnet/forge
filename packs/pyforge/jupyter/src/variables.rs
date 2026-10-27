//! Variables of a kernel's namespace for the Variables view (the Python Console's shape,
//! plus whether View as DataFrame applies).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: String,
    #[serde(default)]
    pub size: Option<u64>,
    /// Expression that reads it (`orders[0].total`), for its members.
    pub expr: String,
    #[serde(default)]
    pub children: bool,
    /// A pandas DataFrame or Series or a NumPy array (View as DataFrame).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub frame: bool,
}
