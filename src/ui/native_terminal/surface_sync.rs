use crate::state::SessionId;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

use super::paint::NativeTerminalPaintBridge;

const SURFACE_SYNC_POLL_MS: u64 = 120;

// The WGPU paint bridge computes surface metrics off the Dioxus render path and
// currently exposes them as sampled getters only. Until the native renderer has a
// push callback for canvas resize updates, this hook owns the boundary polling and
// mirrors only changed values back into app signals.
pub(crate) fn use_native_surface_metric_sync(
    session_id: SessionId,
    bridge: NativeTerminalPaintBridge,
    native_terminal_surface_cells: Signal<HashMap<SessionId, (u16, u16)>>,
    native_terminal_surface_sizes: Signal<HashMap<SessionId, (f64, f64)>>,
) {
    {
        let bridge = bridge.clone();
        let mut native_terminal_surface_cells = native_terminal_surface_cells;
        let mut native_terminal_surface_sizes = native_terminal_surface_sizes;
        use_future(move || {
            let bridge = bridge.clone();
            async move {
                let mut last_surface_cells = None;
                let mut last_surface_size = None;

                loop {
                    tokio::time::sleep(Duration::from_millis(SURFACE_SYNC_POLL_MS)).await;

                    let next_surface_cells = bridge.surface_cells();
                    if next_surface_cells != last_surface_cells {
                        last_surface_cells = next_surface_cells;
                        let mut surface_cells = native_terminal_surface_cells.write();
                        if let Some(cells) = next_surface_cells {
                            surface_cells.insert(session_id, cells);
                        } else {
                            surface_cells.remove(&session_id);
                        }
                    }

                    let next_surface_size = bridge.surface_size_px();
                    if next_surface_size != last_surface_size {
                        last_surface_size = next_surface_size;
                        let mut surface_sizes = native_terminal_surface_sizes.write();
                        if let Some(size) = next_surface_size {
                            surface_sizes.insert(session_id, size);
                        } else {
                            surface_sizes.remove(&session_id);
                        }
                    }
                }
            }
        });
    }

    {
        let mut native_terminal_surface_cells = native_terminal_surface_cells;
        let mut native_terminal_surface_sizes = native_terminal_surface_sizes;
        use_drop(move || {
            native_terminal_surface_cells.write().remove(&session_id);
            native_terminal_surface_sizes.write().remove(&session_id);
        });
    }
}
