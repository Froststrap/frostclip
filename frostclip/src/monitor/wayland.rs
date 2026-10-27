use config::state::MonitorState;
use tracing::warn;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
};

#[derive(Default)]
struct RawOutput {
    name: Option<String>,
    description: Option<String>,
    xdg_name: Option<String>,
    xdg_description: Option<String>,
    make: String,
    model: String,
    width: i32,
    height: i32,
    refresh_mhz: i32,
    x: i32,
    y: i32,
    transform: u32,
    scale: i32,
    logical_x: Option<i32>,
    logical_y: Option<i32>,
    logical_width: Option<i32>,
}

struct State {
    outputs: Vec<RawOutput>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(*index) else {
            return;
        };

        match event {
            wl_output::Event::Geometry {
                x,
                y,
                make,
                model,
                transform,
                ..
            } => {
                output.x = x;
                output.y = y;
                output.make = make;
                output.model = model;
                output.transform = match transform {
                    WEnum::Value(value) => u32::from(value),
                    WEnum::Unknown(value) => value,
                };
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                let current = match flags {
                    WEnum::Value(flags) => flags.contains(wl_output::Mode::Current),
                    WEnum::Unknown(_) => false,
                };

                if current {
                    output.width = width;
                    output.height = height;
                    output.refresh_mhz = refresh;
                }
            }
            wl_output::Event::Scale { factor } => output.scale = factor,
            wl_output::Event::Name { name } => output.name = Some(name),
            wl_output::Event::Description { description } => output.description = Some(description),
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(*index) else {
            return;
        };

        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                output.logical_x = Some(x);
                output.logical_y = Some(y);
            }
            zxdg_output_v1::Event::LogicalSize { width, .. } => {
                output.logical_width = Some(width);
            }
            zxdg_output_v1::Event::Name { name } => output.xdg_name = Some(name),
            zxdg_output_v1::Event::Description { description } => {
                output.xdg_description = Some(description)
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore ZxdgOutputManagerV1);

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn convert(index: usize, raw: RawOutput) -> Option<MonitorState> {
    if raw.width <= 0 || raw.height <= 0 {
        return None;
    }

    let rotated = raw.transform & 1 == 1;

    let (width, height) = if rotated {
        (raw.height, raw.width)
    } else {
        (raw.width, raw.height)
    };

    let name = raw
        .name
        .or(raw.xdg_name)
        .unwrap_or_else(|| format!("output-{index}"));

    let description = raw
        .description
        .or(raw.xdg_description)
        .unwrap_or_else(|| format!("{} {}", raw.make, raw.model).trim().to_string());

    let scale = match raw.logical_width {
        Some(logical) if logical > 0 => width as f64 / logical as f64,
        _ => raw.scale.max(1) as f64,
    };

    Some(MonitorState {
        name,
        description,
        width: width as u32,
        height: height as u32,
        refresh_hz: round2(raw.refresh_mhz as f64 / 1000.0),
        x: raw.logical_x.unwrap_or(raw.x),
        y: raw.logical_y.unwrap_or(raw.y),
        scale: round2(scale),
        restore_token: None,
    })
}

pub fn list() -> Vec<MonitorState> {
    let connection = match Connection::connect_to_env() {
        Ok(connection) => connection,
        Err(err) => {
            warn!("MONITOR: wayland connect failed: {}", err);
            return Vec::new();
        }
    };

    let (globals, mut queue) = match registry_queue_init::<State>(&connection) {
        Ok(pair) => pair,
        Err(err) => {
            warn!("MONITOR: wayland registry init failed: {}", err);
            return Vec::new();
        }
    };

    let qh = queue.handle();
    let mut state = State {
        outputs: Vec::new(),
    };

    let manager = globals
        .bind::<ZxdgOutputManagerV1, State, ()>(&qh, 1..=3, ())
        .ok();

    let found = globals.contents().with_list(|list| {
        list.iter()
            .filter(|global| global.interface == "wl_output")
            .map(|global| (global.name, global.version))
            .collect::<Vec<_>>()
    });

    let mut outputs = Vec::new();
    let mut xdg_outputs = Vec::new();

    for (index, (name, version)) in found.into_iter().enumerate() {
        state.outputs.push(RawOutput::default());

        let output =
            globals
                .registry()
                .bind::<wl_output::WlOutput, _, _>(name, version.min(4), &qh, index);

        if let Some(manager) = &manager {
            xdg_outputs.push(manager.get_xdg_output(&output, &qh, index));
        }

        outputs.push(output);
    }

    for _ in 0..2 {
        if let Err(err) = queue.roundtrip(&mut state) {
            warn!("MONITOR: wayland roundtrip failed: {}", err);
            break;
        }
    }

    state
        .outputs
        .into_iter()
        .enumerate()
        .filter_map(|(index, raw)| convert(index, raw))
        .collect()
}
