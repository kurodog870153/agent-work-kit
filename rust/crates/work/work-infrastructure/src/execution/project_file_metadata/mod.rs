//! Platform-specific project file metadata adapters.

#[cfg(any(target_os = "macos", test))]
pub(super) mod macos;
#[cfg(any(windows, test))]
pub(super) mod windows;
