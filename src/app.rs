// Zweck:    Hauptfenster: Verbindung zum LS10 (COM-Port/Bluetooth-SPP), virtuelles Gerätedisplay,
//           Tasten zum Auslösen (MESS) und für die Dauermessung, Nivellierlatten und ein Messverlauf
// Hardware: Leica LS10 / LS15
// Autor:    Johann Gerner

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::display::{self, ScreenState};
use crate::job::{Job, JobKind, JobOutcome};
use crate::protocol::{self, LevelReading};
use crate::staff::{self, StaffKind};
use crate::verbindung::{Ereignis, SeriellEinstellung, Verbindung, verfuegbare_ports};

/// Auswählbare Baudraten (Bluetooth-SPP ignoriert den Wert meist, RS-232 beim LS15 nicht).
const BAUD_RATES: [u32; 5] = [9600, 19_200, 38_400, 57_600, 115_200];
/// Vorgabe der Baudrate (GSI-Online-Standard der Leica-Geräte).
const DEFAULT_BAUD: u32 = 9600;
/// Höchste Zahl der Einträge im Verlauf.
const MAX_HISTORY: usize = 1000;
/// Aktualisierungsintervall bei aktiver Verbindung bzw. laufendem Auftrag.
const REPAINT_ACTIVE: Duration = Duration::from_millis(50);
/// Aktualisierungsintervall im Leerlauf.
const REPAINT_IDLE: Duration = Duration::from_millis(250);
/// Größe der Bedientasten unter dem Display [px].
const KEY_SIZE: egui::Vec2 = egui::vec2(130.0, 44.0);
/// Spaltenüberschriften des Verlaufs (auch Kopfzeile der CSV-Ausgabe).
const HISTORY_COLUMNS: [&str; 6] = ["Zeit", "Pkt-ID", "Art", "Latte", "Entfernung", "s [mm]"];
/// Zeitformat im Verlauf und in der CSV-Ausgabe (ISO 8601).
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
/// Beispielzeile des Demo-Modus (Punkt 124, Entfernung 24,1234 m, Latte 1,50893 m).
const DEMO_LINE: &str = "*110014+0000000000000124 32...8+0000000002412340 330.08+0000000000150893";
/// Spaltentrenner der CSV-Ausgabe (deutsches Excel).
const CSV_SEPARATOR: &str = ";";

pub struct Ls10App {
    /// Update-Suche und -Fenster
    pub updater: crate::update::Updater,
    connection: Verbindung,
    events: Receiver<Ereignis>,
    ports: Vec<String>,
    selected_port: String,
    baud: u32,
    connecting: bool,
    job: Option<Job>,
    last_reading: Option<LevelReading>,
    history: Vec<LevelReading>,
    status: String,
    status_is_error: bool,
}

impl Ls10App {
    pub fn new(demo: bool) -> Self {
        let connection = Verbindung::default();
        let events = connection.abonnieren();
        let ports = verfuegbare_ports();
        Self {
            updater: crate::update::Updater::default(),
            connection,
            events,
            selected_port: ports.first().cloned().unwrap_or_default(),
            ports,
            baud: DEFAULT_BAUD,
            connecting: false,
            job: None,
            last_reading: demo.then(demo_reading).flatten(),
            history: Vec::new(),
            status: "Nicht verbunden – COM-Port wählen und verbinden.".to_string(),
            status_is_error: false,
        }
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = text.into();
        self.status_is_error = is_error;
    }

    fn connect(&mut self) {
        let settings = SeriellEinstellung::standard(&self.selected_port, self.baud);
        match self.connection.starte_verbinden(&settings) {
            Ok(()) => {
                self.connecting = true;
                self.set_status(format!("Verbinde mit {} ({}) …", settings.port, settings.kurz()), false);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn disconnect(&mut self) {
        self.connection.abbrechen_verbinden();
        self.connection.trennen();
        self.connecting = false;
        self.job = None;
        self.set_status("Verbindung getrennt.", false);
    }

    fn start_job(&mut self, kind: JobKind) {
        if self.job.is_none() && self.connection.ist_verbunden() {
            self.job = Some(Job::new(kind, Instant::now()));
        }
    }

    fn add_reading(&mut self, reading: LevelReading) {
        self.last_reading = Some(reading.clone());
        self.history.push(reading);
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    /// Verbindungsaufbau abfragen und empfangene Ereignisse verarbeiten.
    fn poll_connection(&mut self) {
        if self.connecting {
            match self.connection.pruefe_verbinden() {
                Some(Ok(())) => {
                    self.connecting = false;
                    self.set_status(format!("Verbunden mit {}.", self.selected_port), false);
                }
                Some(Err(e)) => {
                    self.connecting = false;
                    self.set_status(e, true);
                }
                None => {}
            }
        }
        while let Ok(event) = self.events.try_recv() {
            match event {
                Ereignis::Zeile(line) => {
                    if let Some(job) = self.job.as_mut() {
                        job.on_line(&line, Instant::now());
                    }
                }
                Ereignis::Getrennt => {
                    self.job = None;
                    self.set_status("Verbindung zum Nivellier verloren.", true);
                }
                Ereignis::Daten(_) | Ereignis::Bytes { .. } => {}
            }
        }
    }

    /// Befehle senden, Zeitlimits prüfen und Ergebnisse übernehmen.
    fn drive_job(&mut self) {
        let Some(job) = self.job.as_mut() else {
            return;
        };
        if let Some(command) = job.next_command() {
            if let Err(e) = self.connection.senden(&command) {
                self.job = None;
                self.set_status(e, true);
                return;
            }
        }
        job.check_time(Instant::now());
        let readings = job.take_readings();
        let outcome = job.outcome().cloned();
        let status = job.status_text();
        let finished = job.is_finished();
        if !finished {
            self.set_status(status, false);
        }
        readings.into_iter().for_each(|r| self.add_reading(r));
        if finished {
            self.job = None;
            match outcome {
                Some(JobOutcome::Reading(r)) => {
                    self.add_reading(*r);
                    self.set_status("Messwert übernommen.", false);
                }
                Some(JobOutcome::Stopped) => self.set_status("Dauermessung beendet.", false),
                Some(JobOutcome::Failed(text)) => self.set_status(text, true),
                None => {}
            }
        }
    }

    fn connection_bar(&mut self, ui: &mut egui::Ui) {
        let connected = self.connection.ist_verbunden();
        ui.horizontal(|ui| {
            ui.label("COM-Port:");
            ui.add_enabled_ui(!connected && !self.connecting, |ui| {
                egui::ComboBox::from_id_salt("port").selected_text(self.selected_port.clone()).show_ui(ui, |ui| {
                    for port in &self.ports {
                        ui.selectable_value(&mut self.selected_port, port.clone(), port);
                    }
                });
                if ui.button("Ports suchen").clicked() {
                    self.ports = verfuegbare_ports();
                    if !self.ports.contains(&self.selected_port) {
                        self.selected_port = self.ports.first().cloned().unwrap_or_default();
                    }
                }
                ui.label("Baudrate:");
                egui::ComboBox::from_id_salt("baud").selected_text(self.baud.to_string()).show_ui(ui, |ui| {
                    for baud in BAUD_RATES {
                        ui.selectable_value(&mut self.baud, baud, baud.to_string());
                    }
                });
            });
            if connected || self.connecting {
                if ui.button(if connected { "Trennen" } else { "Abbrechen" }).clicked() {
                    self.disconnect();
                }
            } else if ui.add_enabled(!self.selected_port.is_empty(), egui::Button::new("Verbinden")).clicked() {
                self.connect();
            }
            ui.separator();
            ui.menu_button("Hilfe", |ui| {
                self.updater.menu_entry(ui);
                ui.separator();
                ui.label(format!("{} {}", crate::version::PROGRAMM, crate::version::FULL));
            });
        });
    }

    fn device_keys(&mut self, ui: &mut egui::Ui) {
        let connected = self.connection.ist_verbunden();
        let idle = connected && self.job.is_none();
        let continuous = self.job.as_ref().is_some_and(|j| j.kind() == JobKind::Continuous);
        let space = ui.input(|i| i.key_pressed(egui::Key::Space));
        ui.horizontal(|ui| {
            let key = |text: &str| egui::Button::new(egui::RichText::new(text).size(18.0).strong()).min_size(KEY_SIZE);
            if ui.add_enabled(idle, key("MESS")).on_hover_text("Messung auslösen und Werte abholen (Leertaste)").clicked() || (space && idle) {
                self.start_job(JobKind::Measure);
            }
            if ui.add_enabled(idle, key("DAUER")).on_hover_text("Dauermessung starten").clicked() {
                self.start_job(JobKind::Continuous);
            }
            if ui.add_enabled(continuous, key("STOPP")).on_hover_text("Dauermessung beenden").clicked() {
                if let Some(job) = self.job.as_mut() {
                    job.request_stop(Instant::now());
                }
            }
        });
    }

    fn history_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Messverlauf");
            if ui.button("CSV kopieren").on_hover_text("Verlauf als CSV in die Zwischenablage").clicked() {
                let csv = history_csv(&self.history);
                ui.ctx().copy_text(csv);
                self.set_status("Verlauf als CSV in die Zwischenablage kopiert.", false);
            }
            if ui.button("Löschen").clicked() {
                self.history.clear();
                self.last_reading = None;
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            egui::Grid::new("history").striped(true).num_columns(HISTORY_COLUMNS.len()).show(ui, |ui| {
                for title in HISTORY_COLUMNS {
                    ui.strong(title);
                }
                ui.end_row();
                for reading in &self.history {
                    for cell in history_row(reading) {
                        ui.monospace(cell);
                    }
                    ui.end_row();
                }
            });
        });
    }
}

/// Beispielablesung für den Demo-Modus.
fn demo_reading() -> Option<LevelReading> {
    match protocol::parse_line(DEMO_LINE) {
        protocol::Response::Reading(r) => Some(*r),
        _ => None,
    }
}

/// Zellen einer Verlaufszeile.
fn history_row(r: &LevelReading) -> [String; 6] {
    [
        r.timestamp.format(TIME_FORMAT).to_string(),
        r.point_id.clone().unwrap_or_default(),
        display::staff_kind_name(r.staff_wi).to_string(),
        r.staff.map(|l| l.text()).unwrap_or_default(),
        r.distance.map(|l| l.text()).unwrap_or_default(),
        r.std_dev_mm.map(|s| format!("{s:.1}").replace('.', ",")).unwrap_or_default(),
    ]
}

/// CSV-Text des Verlaufs.
fn history_csv(history: &[LevelReading]) -> String {
    let mut text = HISTORY_COLUMNS.join(CSV_SEPARATOR);
    text.push('\n');
    for reading in history {
        text.push_str(&history_row(reading).join(CSV_SEPARATOR));
        text.push('\n');
    }
    text
}

impl eframe::App for Ls10App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_connection();
        self.drive_job();
        let active = self.connecting || self.connection.ist_verbunden() || self.job.is_some();
        ctx.request_repaint_after(if active { REPAINT_ACTIVE } else { REPAINT_IDLE });

        egui::Panel::top("connection").show(ui, |ui| {
            ui.add_space(4.0);
            self.connection_bar(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let reading = self.last_reading.as_ref().and_then(|r| r.staff);
                staff::show(ui, StaffKind::Analog, reading);
                ui.add_space(6.0);
                staff::show(ui, StaffKind::Barcode, reading);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    let state = ScreenState {
                        connected: self.connection.ist_verbunden(),
                        busy: self.job.is_some(),
                        continuous: self.job.as_ref().is_some_and(|j| j.kind() == JobKind::Continuous),
                        status: &self.status,
                        status_is_error: self.status_is_error,
                        reading: self.last_reading.as_ref(),
                    };
                    display::show(ui, &state);
                    ui.add_space(8.0);
                    self.device_keys(ui);
                });
                ui.add_space(12.0);
                ui.vertical(|ui| self.history_view(ui));
            });
        });
        self.updater.show(&ctx);
    }
}
