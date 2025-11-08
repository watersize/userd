//! GUI backend stub for `userd` language
//!
//! This module provides placeholder types and documentation for integrating
//! native desktop GUI backends (winit/egui, gtk, or native platform toolkits).
//!
//! Right now this file contains only documentation and light helpers — the
//! real implementation will be provided later behind feature flags.

/// Represents a platform-window handle (placeholder)
pub struct WindowHandle;

impl WindowHandle {
    /// Placeholder: open a native window with title and size.
    /// Currently this is a no-op stub. Implementations should create
    /// an actual window and return a handle.
    pub fn open(_title: &str, _w: u32, _h: u32) -> Self {
        WindowHandle
    }

    /// Placeholder: show a label in the window. No-op for now.
    pub fn add_label(&self, _text: &str) {}

    /// Placeholder: run or present the window. No-op for now.
    pub fn show(&self) {}
}

// Future: provide traits for backends and implementations behind Cargo features:
// - `gui-winit` feature -> use winit + egui
// - `gui-gtk` feature -> use gtk bindings
// - `gui-native` feature -> platform APIs
