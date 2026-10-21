use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

// A 1080p desktop uses 1280x720; larger screens scale the entire design canvas.
fn fitted_size(width: u32, height: u32, screen_width: u32, screen_height: u32) -> (u32, u32, f64) {
    let scale = ((width.saturating_sub(48) as f64) / 1280.0)
        .min((height.saturating_sub(48) as f64) / 720.0)
        .min((screen_width as f64 / 1920.0).min(screen_height as f64 / 1080.0)).max(0.01);
    ((1280.0 * scale).floor() as u32, (720.0 * scale).floor() as u32, scale)
}

pub fn apply(window: &WebviewWindow) -> tauri::Result<()> {
    if let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) {
        let area = monitor.work_area();
        let (width, height, scale) = fitted_size(area.size.width, area.size.height, monitor.size().width, monitor.size().height);
        // Keep one 1280x720 design canvas on every monitor, including 250% DPI.
        window.set_zoom(scale / monitor.scale_factor())?;
        window.set_size(PhysicalSize::new(width, height))?;
        window.set_position(PhysicalPosition::new(
            area.position.x + (area.size.width.saturating_sub(width) / 2) as i32,
            area.position.y + (area.size.height.saturating_sub(height) / 2) as i32,
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_hd_and_4k_scale_the_entire_canvas() {
        assert_eq!(fitted_size(1920, 1040, 1920, 1080), (1280, 720, 1.0));
        assert_eq!(fitted_size(3840, 2080, 3840, 2160), (2560, 1440, 2.0));
    }
    #[test]
    fn smaller_work_area_preserves_ratio_and_margin() {
        let (w, h, scale) = fitted_size(1366, 728, 1366, 768);
        assert!(w <= 1318 && h <= 680 && scale < 1.0);
        assert!((w as f64 / h as f64 - 16.0 / 9.0).abs() < 0.01);
    }
}
