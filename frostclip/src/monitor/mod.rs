#[cfg(target_os = "linux")]
mod wayland;

#[cfg(target_os = "linux")]
pub use wayland::list as list_monitors;

#[cfg(not(target_os = "linux"))]
pub fn list_monitors() -> Vec<config::state::MonitorState> {
    Vec::new()
}
