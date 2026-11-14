//! Scripted widgets, as the settings app sees them: which widget files have
//! a script, what each asks to be allowed to do, and whether the user has
//! agreed.

use serde::{Deserialize, Serialize};

use crate::widget_file::Permissions;

/// One widget file's script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInfo {
    /// The widget file that names the script: `stocks.yaml`.
    pub file: String,
    /// The widget's name, for showing.
    pub name: String,
    /// The script file: `stocks.luau`.
    pub script: String,
    /// What the widget file says the script needs.
    pub permissions: Permissions,
    /// A fingerprint of the script's text as it is now. Agreeing is to this exact text.
    pub hash: String,
    /// Whether the user agreed to this text with these permissions. A script that is not approved never runs.
    pub approved: bool,
    /// True if the user agreed to an earlier version and the script or what it asks for has changed since.
    pub changed: bool,
    /// The script file is not in the folder (or is too large to be one).
    pub missing: bool,
    /// What went wrong the last time it ran, if anything.
    pub error: Option<String>,
}

/// Params for `approveScript`: the widget file, and the fingerprint the user was shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveScriptParams {
    pub file: String,
    pub hash: String,
}
