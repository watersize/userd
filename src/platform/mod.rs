// Platform-specific helpers. Each OS backend lives in a submodule.
#[cfg(target_os = "windows")]
pub mod windows;

// Future: add linux and mac backends here.
