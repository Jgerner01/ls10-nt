// Zweck:    Serielle Verbindung zum Messgerät (COM/Bluetooth-SPP). Ein einziger Ein-/Ausgabe-Thread besitzt
//           den Port: Er sendet wartende Befehle und liest in kurzen Intervallen. (Unter Windows blockieren sich
//           Lesen und Schreiben auf einem gemeinsamen synchronen Handle gegenseitig; zwei Threads sind daher
//           ungeeignet.) Dazu Zeilenaufbereitung, Verteilung an mehrere Abonnenten und eine Analysefunktion,
//           die Port-Einstellungen durch Ausprobieren findet.
//           Übernommen aus Feldbuch-NT (src/tachymeter/verbindung.rs); gelöst von Einstellungen und Gerätemodell.
// Hardware: Leica LS10/LS15 (Bluetooth-SPP bzw. RS-232 beim LS15), ebenso Tachymeter
// Autor:    Johann Gerner

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serialport::{DataBits, Parity, SerialPort, StopBits};

use crate::fehler_log;

/// Lese-Timeout des Ein-/Ausgabe-Threads (kurz, damit gesendet werden kann).
const LESE_TIMEOUT_MS: u64 = 25;
/// Schreib-Timeout.
const SCHREIB_TIMEOUT_MS: u64 = 3000;
/// Größe des Lesepuffers.
const LESE_PUFFER: usize = 4096;
/// Längste erlaubte Zeile.
const MAX_ZEILENLAENGE: usize = 65_536;
/// Wartezeit nach dem Öffnen, bis das Gerät bereit ist (Bluetooth-Adapter).
const PAUSE_NACH_OEFFNEN_MS: u64 = 150;
/// Längste Wartezeit beim Öffnen des Ports. Ein Bluetooth-COM-Port kann beim Öffnen sonst sehr lange hängen,
/// wenn das Gerät ausgeschaltet oder außer Reichweite ist.
pub const VERBINDEN_TIMEOUT: Duration = Duration::from_secs(10);
/// Anfang des Fehlertextes bei Zeitüberschreitung.
pub const TEXT_ZEITUEBERSCHREITUNG: &str = "Zeitüberschreitung";

/// Öffnet den Port in einem Hilfs-Thread und wartet höchstens `limit`. Kommt das Öffnen später doch noch
/// zustande, wird der Port sofort wieder geschlossen.
fn oeffne_mit_zeitlimit(e: &SeriellEinstellung, limit: Duration) -> Result<Box<dyn SerialPort>, String> {
    let rx = starte_oeffnen(e);
    rx.recv_timeout(limit).unwrap_or_else(|_| Err(zeit_text(&e.port, limit)))
}

fn zeit_text(port: &str, limit: Duration) -> String {
    format!(
        "{TEXT_ZEITUEBERSCHREITUNG}: {port} lässt sich nicht innerhalb von {} s öffnen. Ist das Gerät eingeschaltet und \
         per Bluetooth verbunden (richtiger COM-Port, Richtung ausgehend)?",
        limit.as_secs()
    )
}

type OeffnenErgebnis = Result<Box<dyn SerialPort>, String>;

fn starte_oeffnen(e: &SeriellEinstellung) -> Receiver<OeffnenErgebnis> {
    let (tx, rx) = mpsc::channel();
    let einstellung = e.clone();
    thread::spawn(move || {
        let ergebnis = einstellung.oeffnen();
        // Ist der Empfänger weg (Zeitlimit überschritten), wird der Port hier geschlossen
        let _ = tx.send(ergebnis);
    });
    rx
}

/// Laufender Öffnungsversuch.
struct Oeffner {
    rx: Receiver<OeffnenErgebnis>,
    start: Instant,
    einstellung: SeriellEinstellung,
}

/// Zeilenende beim Senden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Zeilenende {
    /// Wagenrücklauf + Zeilenvorschub (wie das Original).
    #[default]
    CrLf,
    Cr,
    Lf,
}

impl Zeilenende {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::CrLf => b"\r\n",
            Self::Cr => b"\r",
            Self::Lf => b"\n",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CrLf => "CRLF",
            Self::Cr => "CR",
            Self::Lf => "LF",
        }
    }

    pub fn aus_name(name: &str) -> Self {
        [Self::CrLf, Self::Cr, Self::Lf].into_iter().find(|z| z.name().eq_ignore_ascii_case(name)).unwrap_or_default()
    }
}

/// Ereignis der Verbindung.
#[derive(Clone, Debug, PartialEq)]
pub enum Ereignis {
    /// Roher Empfangsblock als Text.
    Daten(String),
    /// Vollständige Zeile.
    Zeile(String),
    /// Rohe Bytes (gesendet oder empfangen) für die Hex-Ansicht.
    Bytes { gesendet: bool, daten: Vec<u8> },
    /// Verbindung wurde getrennt oder ist abgebrochen.
    Getrennt,
}

/// Setzt beliebig fragmentierte Empfangsblöcke zu Zeilen zusammen.
#[derive(Default)]
pub struct SerielleZeilen {
    puffer: String,
}

impl SerielleZeilen {
    /// Nimmt einen Block entgegen und liefert alle damit vollständig gewordenen Zeilen.
    pub fn empfange(&mut self, daten: &str) -> Result<Vec<String>, String> {
        let mut zeilen = Vec::new();
        for c in daten.chars() {
            if c == '\r' || c == '\n' {
                if !self.puffer.is_empty() {
                    zeilen.push(std::mem::take(&mut self.puffer));
                }
            } else {
                if self.puffer.len() >= MAX_ZEILENLAENGE {
                    self.puffer.clear();
                    return Err("Serielle Antwort überschreitet die maximale Zeilenlänge.".to_string());
                }
                self.puffer.push(c);
            }
        }
        Ok(zeilen)
    }
}

/// Einstellungen der seriellen Schnittstelle.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriellEinstellung {
    pub port: String,
    pub baud: u32,
    pub datenbits: u8,
    /// "None", "Odd" oder "Even".
    pub paritaet: String,
    /// "1", "1.5" oder "2".
    pub stopbits: String,
    /// Steuerleitung DTR setzen (Standard aus, wie das Original).
    pub dtr: bool,
    /// Steuerleitung RTS setzen (Standard aus, wie das Original).
    pub rts: bool,
    pub zeilenende: Zeilenende,
}

impl SeriellEinstellung {
    /// Standardformat 8N1 mit der gewünschten Baudrate (Zeilenende CR/LF).
    pub fn standard(port: &str, baud: u32) -> Self {
        Self {
            port: port.to_string(),
            baud,
            datenbits: 8,
            paritaet: "None".into(),
            stopbits: "1".into(),
            dtr: false,
            rts: false,
            zeilenende: Zeilenende::CrLf,
        }
    }

    fn parity(&self) -> Parity {
        match self.paritaet.to_lowercase().as_str() {
            "odd" => Parity::Odd,
            "even" => Parity::Even,
            _ => Parity::None,
        }
    }

    /// Hinweis: Die Bibliothek kennt kein 1,5 Stoppbits; es wird dann 2 verwendet.
    fn stop_bits(&self) -> StopBits {
        match self.stopbits.as_str() {
            "2" | "1.5" => StopBits::Two,
            _ => StopBits::One,
        }
    }

    fn data_bits(&self) -> DataBits {
        match self.datenbits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        }
    }

    /// Kurzform, z. B. "9600 8N1".
    pub fn kurz(&self) -> String {
        let p = match self.parity() {
            Parity::None => 'N',
            Parity::Odd => 'O',
            Parity::Even => 'E',
        };
        format!("{} {}{p}{}", self.baud, self.datenbits, self.stopbits)
    }

    /// Öffnet den Port mit diesen Einstellungen (inklusive DTR/RTS wie eingestellt).
    fn oeffnen(&self) -> Result<Box<dyn SerialPort>, String> {
        let mut port = serialport::new(&self.port, self.baud)
            .data_bits(self.data_bits())
            .parity(self.parity())
            .stop_bits(self.stop_bits())
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(LESE_TIMEOUT_MS))
            .open()
            .map_err(|e| format!("{}: {e}", self.port))?;
        // Steuerleitungen ausdrücklich setzen (das Original lässt sie aus)
        let _ = port.write_data_terminal_ready(self.dtr);
        let _ = port.write_request_to_send(self.rts);
        Ok(port)
    }
}

/// Namen aller vorhandenen COM-Ports, sortiert (COM2 vor COM10).
pub fn verfuegbare_ports() -> Vec<String> {
    let mut ports: Vec<String> = serialport::available_ports()
        .map(|liste| liste.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default();
    ports.sort_by_key(|name| {
        let zahl: String = name.chars().filter(char::is_ascii_digit).collect();
        (name.trim_end_matches(|c: char| c.is_ascii_digit()).to_string(), zahl.parse::<u32>().unwrap_or(0))
    });
    ports
}

type Abonnenten = Arc<Mutex<Vec<Sender<Ereignis>>>>;
type Warteschlange = Arc<Mutex<VecDeque<Vec<u8>>>>;

/// Verteilt ein Ereignis an alle Abonnenten und entfernt abgemeldete.
fn verteile(abonnenten: &Abonnenten, ereignis: &Ereignis) {
    if let Ok(mut liste) = abonnenten.lock() {
        liste.retain(|a| a.send(ereignis.clone()).is_ok());
    }
}

/// Die Verbindung zum Gerät.
pub struct Verbindung {
    ausgang: Warteschlange,
    verbunden: Arc<AtomicBool>,
    stopp: Arc<AtomicBool>,
    io_thread: Option<JoinHandle<()>>,
    abonnenten: Abonnenten,
    nummer: u32,
    letzte: Option<SeriellEinstellung>,
    zeilenende: Zeilenende,
    oeffner: Option<Oeffner>,
}

impl Default for Verbindung {
    fn default() -> Self {
        Self {
            ausgang: Arc::new(Mutex::new(VecDeque::new())),
            verbunden: Arc::new(AtomicBool::new(false)),
            stopp: Arc::new(AtomicBool::new(false)),
            io_thread: None,
            abonnenten: Arc::new(Mutex::new(Vec::new())),
            nummer: 0,
            letzte: None,
            zeilenende: Zeilenende::CrLf,
            oeffner: None,
        }
    }
}

impl Verbindung {
    /// Ist der Port offen und der Thread aktiv?
    pub fn ist_verbunden(&self) -> bool {
        self.io_thread.is_some() && self.verbunden.load(Ordering::SeqCst)
    }

    /// Zählt jede neu geöffnete Verbindung (für den Konfigurationsabgleich).
    pub fn verbindungs_nr(&self) -> u32 {
        self.nummer
    }

    /// Meldet einen Abonnenten an, der alle Ereignisse erhält.
    pub fn abonnieren(&self) -> Receiver<Ereignis> {
        let (tx, rx) = mpsc::channel();
        if let Ok(mut liste) = self.abonnenten.lock() {
            liste.push(tx);
        }
        rx
    }

    /// Zeilenende der folgenden Sendungen.
    pub fn setze_zeilenende(&mut self, z: Zeilenende) {
        self.zeilenende = z;
    }

    /// Wird gerade geöffnet (Hintergrundversuch läuft)?
    pub fn verbindet(&self) -> bool {
        self.oeffner.is_some()
    }

    /// Startet das Öffnen im Hintergrund (höchstens `VERBINDEN_TIMEOUT`). Das Ergebnis liefert `pruefe_verbinden`.
    pub fn starte_verbinden(&mut self, einstellung: &SeriellEinstellung) -> Result<(), String> {
        if self.ist_verbunden() || self.verbindet() {
            return Ok(());
        }
        if einstellung.port.is_empty() {
            return Err("Kein COM-Port ausgewählt.".to_string());
        }
        self.aufraeumen();
        self.oeffner = Some(Oeffner { rx: starte_oeffnen(einstellung), start: Instant::now(), einstellung: einstellung.clone() });
        Ok(())
    }

    /// Bricht einen laufenden Öffnungsversuch ab.
    pub fn abbrechen_verbinden(&mut self) {
        self.oeffner = None;
    }

    /// Fragt den Öffnungsversuch ab: `None` = läuft noch (oder keiner aktiv), sonst das Ergebnis.
    pub fn pruefe_verbinden(&mut self) -> Option<Result<(), String>> {
        let oeffner = self.oeffner.as_ref()?;
        match oeffner.rx.try_recv() {
            Ok(Ok(port)) => {
                let einstellung = oeffner.einstellung.clone();
                self.oeffner = None;
                self.starte_io(port, &einstellung);
                Some(Ok(()))
            }
            Ok(Err(fehler)) => {
                self.oeffner = None;
                Some(Err(fehler))
            }
            Err(mpsc::TryRecvError::Empty) => {
                if oeffner.start.elapsed() >= VERBINDEN_TIMEOUT {
                    let text = zeit_text(&oeffner.einstellung.port, VERBINDEN_TIMEOUT);
                    self.oeffner = None;
                    Some(Err(text))
                } else {
                    None
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.oeffner = None;
                Some(Err("Öffnen des Ports wurde unerwartet beendet.".to_string()))
            }
        }
    }

    /// Öffnet den Port (blockierend, höchstens `VERBINDEN_TIMEOUT`) und startet den Ein-/Ausgabe-Thread.
    pub fn verbinden(&mut self, einstellung: &SeriellEinstellung) -> Result<(), String> {
        if self.ist_verbunden() {
            return Ok(());
        }
        if einstellung.port.is_empty() {
            return Err("Kein COM-Port ausgewählt.".to_string());
        }
        self.aufraeumen();
        let port = oeffne_mit_zeitlimit(einstellung, VERBINDEN_TIMEOUT)?;
        self.starte_io(port, einstellung);
        Ok(())
    }

    fn starte_io(&mut self, mut port: Box<dyn SerialPort>, einstellung: &SeriellEinstellung) {
        let _ = port.set_timeout(Duration::from_millis(LESE_TIMEOUT_MS));
        thread::sleep(Duration::from_millis(PAUSE_NACH_OEFFNEN_MS));
        // Evtl. Altlasten im Empfangspuffer verwerfen
        let _ = port.clear(serialport::ClearBuffer::All);

        self.stopp = Arc::new(AtomicBool::new(false));
        self.verbunden = Arc::new(AtomicBool::new(true));
        if let Ok(mut q) = self.ausgang.lock() {
            q.clear();
        }
        self.zeilenende = einstellung.zeilenende;
        let (stopp, verbunden, abonnenten, ausgang) =
            (Arc::clone(&self.stopp), Arc::clone(&self.verbunden), Arc::clone(&self.abonnenten), Arc::clone(&self.ausgang));
        self.io_thread = Some(thread::spawn(move || io_schleife(port, &stopp, &verbunden, &abonnenten, &ausgang)));
        self.nummer += 1;
        self.letzte = Some(einstellung.clone());
    }

    /// Trennt die Verbindung und meldet es den Abonnenten.
    pub fn trennen(&mut self) {
        self.oeffner = None;
        let war_verbunden = self.io_thread.is_some();
        self.aufraeumen();
        if war_verbunden {
            verteile(&self.abonnenten, &Ereignis::Getrennt);
        }
    }

    fn aufraeumen(&mut self) {
        self.stopp.store(true, Ordering::SeqCst);
        self.verbunden.store(false, Ordering::SeqCst);
        if let Some(h) = self.io_thread.take() {
            let _ = h.join();
        }
    }

    /// Versucht, mit den zuletzt benutzten Einstellungen neu zu verbinden.
    pub fn wiederherstellen(&mut self) -> bool {
        let Some(einstellung) = self.letzte.clone() else {
            return false;
        };
        self.trennen();
        self.verbinden(&einstellung).is_ok()
    }

    /// Reiht einen Befehl samt Zeilenende zum Senden ein (leerer Befehl = nur Zeilenende).
    pub fn senden(&mut self, befehl: &str) -> Result<(), String> {
        self.senden_roh(&[befehl.as_bytes(), self.zeilenende.bytes()].concat())
    }

    /// Reiht beliebige Bytes zum Senden ein.
    pub fn senden_roh(&mut self, daten: &[u8]) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Kein COM-Port verbunden.".to_string());
        }
        self.ausgang.lock().map_err(|_| "Sendewarteschlange gesperrt.".to_string())?.push_back(daten.to_vec());
        Ok(())
    }
}

impl Drop for Verbindung {
    fn drop(&mut self) {
        self.aufraeumen();
    }
}

/// Schleife des Ein-/Ausgabe-Threads: erst alles Wartende senden, dann kurz lesen.
fn io_schleife(
    mut port: Box<dyn SerialPort>,
    stopp: &AtomicBool,
    verbunden: &AtomicBool,
    abonnenten: &Abonnenten,
    ausgang: &Warteschlange,
) {
    let mut zeilen = SerielleZeilen::default();
    let mut puffer = [0u8; LESE_PUFFER];
    let _ = port.set_timeout(Duration::from_millis(SCHREIB_TIMEOUT_MS));
    let mut fehler: Option<String> = None;

    'schleife: while !stopp.load(Ordering::SeqCst) {
        // Senden
        loop {
            let naechster = ausgang.lock().ok().and_then(|mut q| q.pop_front());
            let Some(daten) = naechster else {
                break;
            };
            let _ = port.set_timeout(Duration::from_millis(SCHREIB_TIMEOUT_MS));
            if let Err(e) = port.write_all(&daten).and_then(|()| port.flush()) {
                fehler = Some(format!("Schreibvorgang fehlgeschlagen: {e}"));
                break 'schleife;
            }
            verteile(abonnenten, &Ereignis::Bytes { gesendet: true, daten });
        }
        // Lesen
        let _ = port.set_timeout(Duration::from_millis(LESE_TIMEOUT_MS));
        match port.read(&mut puffer) {
            Ok(0) => {}
            Ok(n) => {
                let bytes = puffer[..n].to_vec();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                // Erst die Rohbytes, dann die daraus entstandenen Zeilen (natürliche Reihenfolge im Protokoll)
                verteile(abonnenten, &Ereignis::Bytes { gesendet: false, daten: bytes });
                match zeilen.empfange(&text) {
                    Ok(liste) => liste.into_iter().for_each(|z| verteile(abonnenten, &Ereignis::Zeile(z))),
                    Err(f) => fehler_log::log("Verbindung.Leser", &f),
                }
                verteile(abonnenten, &Ereignis::Daten(text));
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
            Err(e) => {
                fehler = Some(format!("Lesefehler: {e}"));
                break;
            }
        }
    }
    if let Some(f) = fehler {
        fehler_log::log("Verbindung.IO", &f);
    }
    if !stopp.load(Ordering::SeqCst) {
        verbunden.store(false, Ordering::SeqCst);
        verteile(abonnenten, &Ereignis::Getrennt);
    }
}

// ── Analyse der Port-Einstellungen ────────────────────────────────────────────

/// Ein Analyseergebnis: Einstellung, Befehl und die empfangenen Bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalyseTreffer {
    pub einstellung: String,
    /// Die Einstellung zum Übernehmen.
    pub werte: SeriellEinstellung,
    pub befehl: String,
    pub antwort_hex: String,
    pub antwort_text: String,
    /// Die Antwort enthält mehr als Leerzeichen/Zeilenenden.
    pub brauchbar: bool,
}

/// Befehle, die die Analyse ausprobiert (Bezeichnung, Bytes).
fn analyse_befehle() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("GeoCOM Ping", b"%R1Q,0,0:\r\n".to_vec()),
        ("GeoCOM Ping mit LF davor", b"\n%R1Q,0,0:\r\n".to_vec()),
        ("GSI Nivellier (Entfernung, Lattenablesung)", b"GET/I/WI32/WI330\r\n".to_vec()),
        ("GSI Winkel", b"GET/I/WI21\r\n".to_vec()),
        ("nur CRLF", b"\r\n".to_vec()),
    ]
}

/// Ist die Antwort mehr als Leerraum?
pub fn antwort_brauchbar(daten: &[u8]) -> bool {
    daten.iter().any(|b| !matches!(b, b' ' | b'\r' | b'\n' | 0 | b'\t'))
}

/// Bytes als Hex-Text, z. B. "20 0D 0A".
pub fn hex(daten: &[u8]) -> String {
    daten.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Bytes als lesbarer Text; Steuerzeichen erscheinen als `\r`, `\n` bzw. `·`.
pub fn sichtbar(daten: &[u8]) -> String {
    daten
        .iter()
        .map(|b| match b {
            b'\r' => "\\r".to_string(),
            b'\n' => "\\n".to_string(),
            0x20..=0x7E => (*b as char).to_string(),
            _ => "·".to_string(),
        })
        .collect()
}

/// Probiert Baudraten, Zeichenformate und DTR/RTS-Stellungen durch und meldet, womit das Gerät antwortet.
/// Der Port darf dabei nicht von der Verbindung belegt sein. `fortschritt` erhält Statuszeilen,
/// `abbrechen` beendet die Analyse vorzeitig.
pub fn analysiere(
    port_name: &str,
    abbrechen: &AtomicBool,
    mut fortschritt: impl FnMut(String),
    mut treffer: impl FnMut(AnalyseTreffer),
) {
    const BAUDRATEN: [u32; 8] = [9600, 19_200, 38_400, 4800, 2400, 1200, 57_600, 115_200];
    const FORMATE: [(u8, &str, &str); 3] = [(8, "None", "1"), (7, "Even", "1"), (8, "Even", "1")];
    /// Wartezeit auf die Antwort je Befehl.
    const ANTWORT_WARTEZEIT_MS: u64 = 450;

    let mut gefunden = 0;
    for baud in BAUDRATEN {
        for (bits, par, stop) in FORMATE {
            for (dtr, rts) in [(false, false), (true, true)] {
                if abbrechen.load(Ordering::SeqCst) {
                    fortschritt("Analyse abgebrochen.".to_string());
                    return;
                }
                let e = SeriellEinstellung {
                    port: port_name.to_string(),
                    baud,
                    datenbits: bits,
                    paritaet: par.to_string(),
                    stopbits: stop.to_string(),
                    dtr,
                    rts,
                    zeilenende: Zeilenende::CrLf,
                };
                let beschreibung = format!("{}  DTR={} RTS={}", e.kurz(), u8::from(dtr), u8::from(rts));
                fortschritt(format!("Prüfe {beschreibung} …"));
                let mut port = match oeffne_mit_zeitlimit(&e, VERBINDEN_TIMEOUT) {
                    Ok(p) => p,
                    Err(f) if f.starts_with(TEXT_ZEITUEBERSCHREITUNG) => {
                        fortschritt(format!("Analyse abgebrochen: {f}"));
                        return;
                    }
                    Err(f) => {
                        fortschritt(format!("  nicht möglich: {f}"));
                        continue;
                    }
                };
                thread::sleep(Duration::from_millis(PAUSE_NACH_OEFFNEN_MS));
                for (name, bytes) in analyse_befehle() {
                    let _ = port.clear(serialport::ClearBuffer::All);
                    if port.write_all(&bytes).and_then(|()| port.flush()).is_err() {
                        break;
                    }
                    let mut empfangen: Vec<u8> = Vec::new();
                    let ende = Instant::now() + Duration::from_millis(ANTWORT_WARTEZEIT_MS);
                    let mut puffer = [0u8; 512];
                    while Instant::now() < ende {
                        match port.read(&mut puffer) {
                            Ok(n) if n > 0 => empfangen.extend_from_slice(&puffer[..n]),
                            _ => {}
                        }
                    }
                    if !empfangen.is_empty() {
                        let brauchbar = antwort_brauchbar(&empfangen);
                        if brauchbar {
                            gefunden += 1;
                        }
                        treffer(AnalyseTreffer {
                            einstellung: beschreibung.clone(),
                            werte: e.clone(),
                            befehl: name.to_string(),
                            antwort_hex: hex(&empfangen),
                            antwort_text: sichtbar(&empfangen),
                            brauchbar,
                        });
                    }
                }
            }
        }
    }
    fortschritt(if gefunden == 0 {
        "Analyse beendet: Es kam keine verwertbare Antwort. Gerät, Kabel/Bluetooth-Port und Betriebsart (GeoCOM/GSI) prüfen."
            .to_string()
    } else {
        format!("Analyse beendet: {gefunden} verwertbare Antwort(en) gefunden.")
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeilen_aus_fragmenten() {
        let mut z = SerielleZeilen::default();
        assert!(z.empfange("%R1P,0,0:0,1").unwrap().is_empty());
        assert_eq!(z.empfange(".5\r\n%R1P,0").unwrap(), vec!["%R1P,0,0:0,1.5"]);
        assert_eq!(z.empfange(",0:0\r").unwrap(), vec!["%R1P,0,0:0"]);
        assert!(z.empfange("\r\n\r\n").unwrap().is_empty());
    }

    #[test]
    fn zu_lange_zeile_wird_verworfen() {
        let mut z = SerielleZeilen::default();
        let lang = "x".repeat(MAX_ZEILENLAENGE + 5);
        assert!(z.empfange(&lang).is_err());
        assert_eq!(z.empfange("ok\n").unwrap(), vec!["ok"]);
    }

    #[test]
    fn abonnenten_erhalten_ereignisse_und_abgemeldete_verschwinden() {
        let v = Verbindung::default();
        let a = v.abonnieren();
        let b = v.abonnieren();
        verteile(&v.abonnenten, &Ereignis::Zeile("x".into()));
        assert_eq!(a.try_recv().unwrap(), Ereignis::Zeile("x".into()));
        assert_eq!(b.try_recv().unwrap(), Ereignis::Zeile("x".into()));
        drop(b);
        verteile(&v.abonnenten, &Ereignis::Getrennt);
        assert_eq!(a.try_recv().unwrap(), Ereignis::Getrennt);
        assert_eq!(v.abonnenten.lock().unwrap().len(), 1);
    }

    #[test]
    fn senden_ohne_verbindung_meldet_fehler() {
        let mut v = Verbindung::default();
        assert!(!v.ist_verbunden());
        assert!(v.senden("x").is_err());
        assert!(v.verbinden(&SeriellEinstellung::standard("", 9600)).is_err());
        assert!(!v.wiederherstellen());
    }

    #[test]
    fn kurzform_und_paritaet() {
        let s = SeriellEinstellung {
            port: "COM7".into(),
            baud: 38_400,
            datenbits: 7,
            paritaet: "Even".into(),
            stopbits: "2".into(),
            dtr: true,
            rts: false,
            zeilenende: Zeilenende::Lf,
        };
        assert_eq!(s.parity(), Parity::Even);
        assert_eq!(s.kurz(), "38400 7E2");
        assert_eq!(SeriellEinstellung::standard("COM1", 115_200).kurz(), "115200 8N1");
    }

    #[test]
    fn zeilenende_und_hexdarstellung() {
        assert_eq!(Zeilenende::CrLf.bytes(), b"\r\n");
        assert_eq!(Zeilenende::aus_name("lf"), Zeilenende::Lf);
        assert_eq!(Zeilenende::aus_name("quatsch"), Zeilenende::CrLf);
        assert_eq!(hex(b" \r\n"), "20 0D 0A");
        assert_eq!(sichtbar(b"%R1P\r\n\x01"), "%R1P\\r\\n·");
    }

    #[test]
    fn leerraum_ist_keine_antwort() {
        assert!(!antwort_brauchbar(b" \r\n"));
        assert!(!antwort_brauchbar(b""));
        assert!(!antwort_brauchbar(&[0, 32, 13, 10]));
        assert!(antwort_brauchbar(b"%R1P,0,0:0\r\n"));
        assert!(antwort_brauchbar(b"?\r\n"));
    }

    #[test]
    fn portliste_ist_aufrufbar() {
        let _ = verfuegbare_ports();
    }

    #[test]
    fn analyse_ohne_vorhandenen_port_endet_ohne_absturz() {
        let abbrechen = AtomicBool::new(false);
        let mut meldungen = Vec::new();
        // Abbruch nach der ersten Meldung, damit der Test kurz bleibt
        analysiere("COM_GIBT_ES_NICHT", &abbrechen, |m| {
            meldungen.push(m);
            abbrechen.store(true, Ordering::SeqCst);
        }, |_| {});
        assert!(!meldungen.is_empty());
    }
}
