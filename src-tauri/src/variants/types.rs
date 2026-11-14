//! Serde types shared with the frontend (`src/types.ts`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraState {
    Disconnected,
    NotReady,
    Busy,
    Ready,
    Error,
    /// A camera call is hung (frozen camera): waiting for it to return.
    Unresponsive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraStatus {
    pub state: CameraState,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub message: Option<String>,
    pub simulated: bool,
}

impl CameraStatus {
    pub fn new(state: CameraState, message: impl Into<Option<String>>) -> Self {
        Self {
            state,
            model: None,
            firmware: None,
            message: message.into(),
            simulated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraPreset {
    pub id: String,
    pub bank: Option<u8>,
    pub name: String,
    pub folder_name: String,
    pub film_simulation: Option<String>,
    pub summary: String,
    pub unapplied_properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantRequest {
    pub sources: Vec<String>,
    pub output_dir: Option<String>,
    pub preset_ids: Vec<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedPreset {
    pub id: String,
    pub folder_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantPlan {
    pub source_root: String,
    pub output_dir: String,
    pub raf_count: usize,
    pub presets: Vec<PlannedPreset>,
    pub total_conversions: usize,
    pub pending_conversions: usize,
    pub already_done: usize,
    pub estimated_bytes: u64,
    pub free_bytes: u64,
    pub enough_space: bool,
    pub estimated_seconds: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VariantJobState {
    Running,
    Cancelling,
    Paused,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantProgress {
    pub state: VariantJobState,
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_raf: Option<String>,
    pub current_preset: Option<String>,
    pub eta_seconds: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantFailure {
    pub raf: String,
    pub preset: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantReport {
    pub state: VariantJobState,
    pub output_dir: String,
    pub converted: usize,
    pub skipped: usize,
    pub failures: Vec<VariantFailure>,
    pub paused_reason: Option<String>,
}
