//! Fujifilm camera connector: PTP over Windows Portable Devices (WPD).

pub mod banks;
pub mod fuji;
pub mod profile;
pub mod ptp;
pub mod sim;
#[cfg(windows)]
pub mod wpd;

/// Fujifilm USB vendor id.
pub const FUJIFILM_USB_VID: &str = "VID_04CB";
