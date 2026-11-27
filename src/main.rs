// Zweck:    Einstiegspunkt von LS10-NT: Messungen am Leica LS10 auslösen und abholen,
//           Darstellung als virtuelles LS10-Display
// Hardware: Leica LS10 / LS15 über COM-Port (Bluetooth-SPP bzw. RS-232 beim LS15)
// Autor:    Johann Gerner

#![windows_subsystem = "windows"]

mod app;
mod display;
mod fehler_log;
mod job;
mod protocol;
mod staff;
mod update;
mod version;
// Unverändert aus Feldbuch-NT übernommen (u. a. Port-Analyse); ungenutzte Teile bleiben für den Abgleich erhalten
#[allow(dead_code)]
mod verbindung;

/// Kommandozeilenargument für den Demo-Modus.
const DEMO_ARGUMENT: &str = "--demo";
/// Fenstertitel.
const WINDOW_TITLE: &str = concat!("LS10-NT ", env!("CARGO_PKG_VERSION"), " – Leica LS10 Fernbedienung");
/// Anfangsgröße des Fensters in Pixeln (Breite, Höhe).
const WINDOW_SIZE: [f32; 2] = [1560.0, 600.0];
/// Umgebungsvariable, mit der wgpu das Grafik-Backend wählt.
const BACKEND_ENV: &str = "WGPU_BACKEND";
/// Bevorzugte Grafik-Backends: DirectX 12, sonst OpenGL (alte Intel-Vulkan-Treiber stürzen ab).
const BACKEND_DEFAULT: &str = "dx12,gl";

fn main() -> eframe::Result {
    // Vulkan vermeiden, sofern der Benutzer nichts anderes eingestellt hat
    if std::env::var_os(BACKEND_ENV).is_none() {
        // SAFETY: wird am Programmanfang gesetzt, bevor weitere Threads existieren
        unsafe { std::env::set_var(BACKEND_ENV, BACKEND_DEFAULT) };
    }
    // "--demo" zeigt einen Beispielwert ohne Gerät (zum Ansehen der Darstellung)
    let demo = std::env::args().any(|a| a == DEMO_ARGUMENT);
    let viewport = eframe::egui::ViewportBuilder::default().with_inner_size(WINDOW_SIZE);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(WINDOW_TITLE, options, Box::new(move |cc| {
        let mut app = app::Ls10App::new(demo);
        app.updater.search_quietly(&cc.egui_ctx);
        Ok(Box::new(app))
    }))
}
