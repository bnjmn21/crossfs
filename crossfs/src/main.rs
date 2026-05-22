use crossfs::{Info, ToUi, xplane::XPlaneAdapter};
use eframe::egui;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "CrossFS",
        native_options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    )
    .unwrap();
}

pub enum ConnectionState {
    Disconnected,
    Connected,
    Loading,
}

struct MyEguiApp {
    x_plane: XPlaneAdapter,
    x_plane_state: ConnectionState,
    info: Info,
}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_global_style.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self {
            x_plane: XPlaneAdapter::new(),
            x_plane_state: ConnectionState::Disconnected,
            info: Info::default(),
        }
    }
}

impl eframe::App for MyEguiApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        loop {
            match self.x_plane.to_ui.try_recv() {
                Ok(ToUi::SimDisconnected) => self.x_plane_state = ConnectionState::Disconnected,
                Ok(ToUi::SimConnected) => self.x_plane_state = ConnectionState::Connected,
                Ok(ToUi::SimLoading) => self.x_plane_state = ConnectionState::Loading,
                Ok(ToUi::Info(info)) => self.info = info,
                _ => return,
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| match self.x_plane_state {
            ConnectionState::Disconnected => {
                ui.label("XPlane: Disconnected");
            }
            ConnectionState::Loading => {
                ui.label("XPlane: Loading...");
            }
            ConnectionState::Connected => {
                ui.label("XPlane: Connected");
                ui.label(format!("Info: {}", self.info));
            }
        });
        ui.request_repaint();
    }
}
