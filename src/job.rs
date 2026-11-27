// Zweck:    Ablauf eines Messauftrags (Zustandsautomat ohne Blockieren, im Stil von Messablauf aus Feldbuch-NT):
//           Einzelmessung (GET/M) und Dauermessung (GET/C mit Stopp "c").
//           Die Oberfläche holt Befehle ab, füttert empfangene Zeilen ein und prüft die Zeitlimits.
// Hardware: Leica LS10 / LS15
// Autor:    Johann Gerner

use std::time::{Duration, Instant};

use crate::protocol::{self, DeviceError, LevelReading, Response};

/// Wartezeit auf die Antwort einer Einzelmessung (Mittelwert-Modi dauern mehrere Sekunden).
pub const SINGLE_TIMEOUT: Duration = Duration::from_secs(15);
/// Längste Pause zwischen zwei Ablesungen der Dauermessung.
pub const CONTINUOUS_TIMEOUT: Duration = Duration::from_secs(15);
/// Wartezeit auf die Quittung nach dem Stoppbefehl.
pub const STOP_GRACE: Duration = Duration::from_millis(1500);

/// Art des Auftrags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Messung auslösen und Werte abholen.
    Measure,
    /// Dauermessung.
    Continuous,
}

impl JobKind {
    fn command(self) -> &'static str {
        match self {
            Self::Measure => protocol::CMD_MEASURE,
            Self::Continuous => protocol::CMD_CONTINUOUS,
        }
    }

    fn status_text(self) -> &'static str {
        match self {
            Self::Measure => "Messung läuft …",
            Self::Continuous => "Dauermessung läuft …",
        }
    }
}

/// Ergebnis eines Auftrags.
#[derive(Clone, Debug, PartialEq)]
pub enum JobOutcome {
    /// Einzelmessung.
    Reading(Box<LevelReading>),
    /// Dauermessung beendet.
    Stopped,
    /// Fehlertext für die Statuszeile.
    Failed(String),
}

fn device_error_text(e: &DeviceError) -> String {
    format!("{}: {}", e.code, e.text)
}

/// Zustand eines laufenden Auftrags.
pub struct Job {
    kind: JobKind,
    /// Zeitpunkt der letzten Aktivität (Start bzw. letzte Ablesung).
    last_activity: Instant,
    pending_command: Option<String>,
    /// Teilweise eingetroffene Ablesung (Wörter auf mehrere Zeilen verteilt).
    partial: Option<LevelReading>,
    /// Fertige Ablesungen der Dauermessung, noch nicht abgeholt.
    readings: Vec<LevelReading>,
    /// Zeitpunkt des Stoppbefehls der Dauermessung.
    stop_requested: Option<Instant>,
    outcome: Option<JobOutcome>,
}

impl Job {
    pub fn new(kind: JobKind, now: Instant) -> Self {
        Self {
            kind,
            last_activity: now,
            pending_command: Some(kind.command().to_string()),
            partial: None,
            readings: Vec::new(),
            stop_requested: None,
            outcome: None,
        }
    }

    pub fn kind(&self) -> JobKind {
        self.kind
    }

    /// Nächster zu sendender Befehl (einmalig).
    pub fn next_command(&mut self) -> Option<String> {
        self.pending_command.take()
    }

    pub fn is_finished(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn outcome(&self) -> Option<&JobOutcome> {
        self.outcome.as_ref()
    }

    /// Text für die Statuszeile.
    pub fn status_text(&self) -> &'static str {
        if self.stop_requested.is_some() { "Dauermessung wird beendet …" } else { self.kind.status_text() }
    }

    /// Fertige Ablesungen der Dauermessung abholen.
    pub fn take_readings(&mut self) -> Vec<LevelReading> {
        std::mem::take(&mut self.readings)
    }

    /// Beendet die Dauermessung (sendet "c"); bei anderen Aufträgen ohne Wirkung.
    pub fn request_stop(&mut self, now: Instant) {
        if self.kind == JobKind::Continuous && self.stop_requested.is_none() && !self.is_finished() {
            self.stop_requested = Some(now);
            self.pending_command = Some(protocol::CMD_STOP.to_string());
        }
    }

    fn fail(&mut self, text: String) {
        self.outcome = Some(JobOutcome::Failed(text));
    }

    /// Prüft die Zeitlimits.
    pub fn check_time(&mut self, now: Instant) {
        if self.is_finished() {
            return;
        }
        if let Some(stop) = self.stop_requested {
            if now.saturating_duration_since(stop) >= STOP_GRACE {
                self.outcome = Some(JobOutcome::Stopped);
            }
            return;
        }
        let limit = if self.kind == JobKind::Continuous { CONTINUOUS_TIMEOUT } else { SINGLE_TIMEOUT };
        if now.saturating_duration_since(self.last_activity) >= limit {
            self.fail(format!(
                "Zeitüberschreitung: keine Antwort vom Nivellier (nach {} s). Bluetooth-Verbindung, COM-Port, Baudrate und \
                 Schnittstelle am Gerät prüfen.",
                limit.as_secs()
            ));
        }
    }

    /// Verarbeitet eine empfangene Zeile.
    pub fn on_line(&mut self, line: &str, now: Instant) {
        if self.is_finished() {
            return;
        }
        match protocol::parse_line(line) {
            Response::Ack => {
                if self.stop_requested.is_some() {
                    self.outcome = Some(JobOutcome::Stopped);
                }
            }
            Response::Error(e) => {
                if self.stop_requested.is_some() {
                    self.outcome = Some(JobOutcome::Stopped);
                } else {
                    self.fail(device_error_text(&e));
                }
            }
            Response::Reading(r) => self.on_reading(*r, now),
            Response::Ignored => {}
        }
    }

    fn on_reading(&mut self, reading: LevelReading, now: Instant) {
        let merged = match self.partial.take() {
            Some(mut p) => {
                p.merge(reading);
                p
            }
            None => reading,
        };
        if !merged.is_complete() {
            self.partial = Some(merged);
            return;
        }
        self.last_activity = now;
        match self.kind {
            JobKind::Continuous => self.readings.push(merged),
            JobKind::Measure => self.outcome = Some(JobOutcome::Reading(Box::new(merged))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "110014+00000124 32...6+00241234 330.06+00010509";

    #[test]
    fn einzelmessung_liefert_ablesung() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Measure, t0);
        assert_eq!(j.next_command().as_deref(), Some("GET/M/WI11/WI32/WI330"));
        assert!(j.next_command().is_none());
        j.on_line("?", t0);
        j.on_line("", t0);
        assert!(!j.is_finished());
        j.on_line(LINE, t0);
        let Some(JobOutcome::Reading(r)) = j.outcome() else {
            panic!("Ablesung erwartet");
        };
        assert!(r.is_complete());
    }

    #[test]
    fn geraetefehler_beendet_mit_klartext() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Measure, t0);
        j.on_line("@E439", t0);
        let Some(JobOutcome::Failed(t)) = j.outcome() else {
            panic!("Fehler erwartet");
        };
        assert!(t.contains("@E439") && t.contains("Latte"));
    }

    #[test]
    fn verteilte_zeilen_werden_zusammengesetzt() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Measure, t0);
        j.on_line("110014+00000124 32...6+00241234", t0);
        assert!(!j.is_finished());
        j.on_line("330.06+00010509", t0);
        assert!(j.is_finished());
    }

    #[test]
    fn zeitueberschreitung_nach_limit() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Measure, t0);
        j.check_time(t0 + SINGLE_TIMEOUT - Duration::from_secs(1));
        assert!(!j.is_finished());
        j.check_time(t0 + SINGLE_TIMEOUT);
        assert!(matches!(j.outcome(), Some(JobOutcome::Failed(t)) if t.contains("Zeitüberschreitung")));
    }

    #[test]
    fn dauermessung_sammelt_und_stoppt() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Continuous, t0);
        assert_eq!(j.next_command().as_deref(), Some("GET/C/WI11/WI32/WI330"));
        j.on_line(LINE, t0);
        j.on_line(LINE, t0);
        assert!(!j.is_finished());
        assert_eq!(j.take_readings().len(), 2);
        assert!(j.take_readings().is_empty());
        j.request_stop(t0);
        assert_eq!(j.next_command().as_deref(), Some("c"));
        j.check_time(t0 + Duration::from_millis(100));
        assert!(!j.is_finished());
        j.on_line("?", t0);
        assert_eq!(j.outcome(), Some(&JobOutcome::Stopped));
    }

    #[test]
    fn stopp_ohne_quittung_endet_nach_frist() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Continuous, t0);
        j.next_command();
        j.request_stop(t0);
        j.check_time(t0 + STOP_GRACE);
        assert_eq!(j.outcome(), Some(&JobOutcome::Stopped));
    }

    #[test]
    fn stopp_bei_einzelmessung_wirkungslos() {
        let t0 = Instant::now();
        let mut j = Job::new(JobKind::Measure, t0);
        j.next_command();
        j.request_stop(t0);
        assert!(j.next_command().is_none());
    }
}
