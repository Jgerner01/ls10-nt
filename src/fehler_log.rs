// Zweck:    Minimale Fehlerprotokollierung (Ersatz für grundlagen::fehler_log aus Feldbuch-NT):
//           hängt Einträge an Fehler.log neben der Programmdatei an, sonst Ausgabe auf stderr
// Hardware: keine
// Autor:    Johann Gerner

use std::fs::OpenOptions;
use std::io::Write;

use chrono::Local;

/// Name der Fehlerdatei.
const ERROR_LOG_FILE: &str = "Fehler.log";

/// Protokolliert einen Fehler mit Kontext.
pub fn log(kontext: &str, nachricht: &str) {
    let eintrag = format!("{}  [{kontext}]  {nachricht}\r\n", Local::now().format("%Y-%m-%d %H:%M:%S"));
    let path = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(ERROR_LOG_FILE)));
    let written = path
        .and_then(|p| OpenOptions::new().create(true).append(true).open(p).ok())
        .and_then(|mut f| f.write_all(eintrag.as_bytes()).ok());
    if written.is_none() {
        eprintln!("[LS10-NT FEHLER] {kontext}: {nachricht}");
    }
}
