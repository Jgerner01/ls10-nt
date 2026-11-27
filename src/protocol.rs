// Zweck:    GSI-Online-Protokoll der Leica-Digitalnivelliere (LS10/LS15, DNA03): Befehlsstrings,
//           Parser für GSI-8/GSI-16-Datenwörter (Punkt-ID, Entfernung, Lattenablesung, Mittelung)
//           und Klartexte der Gerätefehler.
//           Quelle: Leica "GSI ONLINE for Leica TPS and DNA", Abschnitt DNA (Word Indices, Einheiten-Flags)
//           Wortaufbau: WI (2-3 Zeichen) + Info (bis Stelle 6, Stelle 6 = Einheit) + Vorzeichen + Daten (8/16 Ziffern)
// Hardware: Leica LS10 / LS15 (Bluetooth-SPP, beim LS15 auch RS-232)
// Autor:    Johann Gerner

use chrono::{Local, NaiveDateTime};

// ── Befehle (PC → Gerät) ──────────────────────────────────────────────────────

/// Messung auslösen und Punkt-ID, Entfernung und Lattenablesung abholen.
pub const CMD_MEASURE: &str = "GET/M/WI11/WI32/WI330";
/// Dauermessung starten (läuft, bis `CMD_STOP` gesendet oder am Gerät [CE] gedrückt wird).
pub const CMD_CONTINUOUS: &str = "GET/C/WI11/WI32/WI330";
/// Dauermessung beenden.
pub const CMD_STOP: &str = "c";

// ── Word Indices ──────────────────────────────────────────────────────────────

const WI_POINT_ID: u16 = 11;
const WI_DISTANCE: u16 = 32;
const WI_HEIGHT: u16 = 83;
/// Lattenablesungen: 330 = nur messen, 331 = Rückblick, 332 = Vorblick, 333 = Zwischenblick, 334 = Absteckung, 335/336 = B2/F2.
const WI_STAFF_FIRST: u16 = 330;
const WI_STAFF_LAST: u16 = 336;
/// Anzahl der Wiederholungsmessungen.
const WI_COUNT: u16 = 390;
/// Standardabweichung der Einzelmessung (Mittelwert-Modus).
const WI_STD_DEV: u16 = 391;
/// Alle dreistelligen Word Indices, die der Parser kennt (alle anderen werden zweistellig gelesen).
const THREE_DIGIT_WIS: [u16; 14] = [330, 331, 332, 333, 334, 335, 336, 374, 390, 391, 392, 571, 573, 574];

// ── Wortaufbau ────────────────────────────────────────────────────────────────

/// Kürzeste mögliche Wortlänge (GSI-8 ohne Trennzeichen).
const MIN_WORD_LEN: usize = 15;
/// Stelle (0-basiert) des Einheiten-Flags.
const UNIT_POS: usize = 5;
/// Stelle (0-basiert) des Vorzeichens.
const SIGN_POS: usize = 6;
/// Stelle (0-basiert), ab der die Daten beginnen.
const DATA_POS: usize = 7;
/// Länge des Präfixfeldes zur Erkennung dreistelliger Word Indices.
const WI_PROBE_LEN: usize = 3;
/// Zweistelliger Word Index.
const WI_SHORT_LEN: usize = 2;

/// Millimeter pro Meter (Standardabweichung wird in mm angezeigt).
const MM_PER_METER: f64 = 1000.0;

/// Längeneinheit eines Datenworts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Meter,
    Feet,
}

impl LengthUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Meter => "m",
            Self::Feet => "ft",
        }
    }
}

/// Länge mit Einheit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    pub value: f64,
    pub unit: LengthUnit,
    /// Nachkommastellen entsprechend der Auflösung des Geräts.
    pub decimals: usize,
}

impl Length {
    /// Wert mit Dezimalkomma in der Geräteauflösung, ohne Einheit, z. B. "1,25972".
    pub fn number_text(&self) -> String {
        format!("{:.*}", self.decimals, self.value).replace('.', ",")
    }

    /// Wert mit Einheit, z. B. "1,25972 m".
    pub fn text(&self) -> String {
        format!("{} {}", self.number_text(), self.unit.label())
    }
}

/// Dekodiert den Rohwert nach dem Einheiten-Flag (Stelle 6 des Wortes).
/// 0 = m (1 mm), 6 = m (0,1 mm), 8 = m (0,01 mm), 1 = ft (0,001), 7 = ft (0,0001).
fn decode_length(raw: i64, unit_flag: char) -> Option<Length> {
    let (decimals, unit) = match unit_flag {
        '0' => (3, LengthUnit::Meter),
        '6' => (4, LengthUnit::Meter),
        '8' => (5, LengthUnit::Meter),
        '1' => (3, LengthUnit::Feet),
        '7' => (4, LengthUnit::Feet),
        _ => return None,
    };
    Some(Length { value: raw as f64 / 10f64.powi(decimals as i32), unit, decimals })
}

/// Zerlegtes Datenwort.
struct Word<'a> {
    wi: u16,
    unit_flag: char,
    negative: bool,
    data: &'a str,
}

fn detect_wi(token: &str) -> Option<u16> {
    if let Ok(n) = token[..WI_PROBE_LEN].parse::<u16>() {
        if THREE_DIGIT_WIS.contains(&n) {
            return Some(n);
        }
    }
    token[..WI_SHORT_LEN].parse().ok()
}

fn split_word(token: &str) -> Option<Word<'_>> {
    // GSI-16 beginnt mit "*"
    let t = token.strip_prefix('*').unwrap_or(token);
    if t.len() < MIN_WORD_LEN || !t.is_ascii() {
        return None;
    }
    let bytes = t.as_bytes();
    Some(Word {
        wi: detect_wi(t)?,
        unit_flag: bytes[UNIT_POS] as char,
        negative: bytes[SIGN_POS] == b'-',
        data: &t[DATA_POS..],
    })
}

impl Word<'_> {
    fn number(&self) -> Option<i64> {
        let n: i64 = self.data.trim().parse().ok()?;
        Some(if self.negative { -n } else { n })
    }

    fn length(&self) -> Option<Length> {
        decode_length(self.number()?, self.unit_flag)
    }
}

/// Punkt-ID ohne führende Nullen ("0000A110" → "A110").
fn point_id_text(data: &str) -> String {
    let trimmed = data.trim_end().trim_start_matches('0');
    if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
}

// ── Messwerte ─────────────────────────────────────────────────────────────────

/// Eine Ablesung des Nivelliers; nicht gelieferte Felder bleiben `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelReading {
    pub timestamp: NaiveDateTime,
    pub raw: String,
    pub point_id: Option<String>,
    /// Horizontale Entfernung zur Latte.
    pub distance: Option<Length>,
    /// Lattenablesung.
    pub staff: Option<Length>,
    /// Word Index der Lattenablesung (330 = nur messen, 331 = Rückblick, …).
    pub staff_wi: Option<u16>,
    /// Höhe des Punktes (WI 83), falls geliefert.
    pub height: Option<Length>,
    /// Anzahl der Einzelmessungen (Mittelwert-Modus).
    pub count: Option<u32>,
    /// Standardabweichung der Einzelmessung [mm].
    pub std_dev_mm: Option<f64>,
}

impl LevelReading {
    fn empty(raw: &str) -> Self {
        Self {
            timestamp: Local::now().naive_local(),
            raw: raw.to_string(),
            point_id: None,
            distance: None,
            staff: None,
            staff_wi: None,
            height: None,
            count: None,
            std_dev_mm: None,
        }
    }

    /// Enthält die Ablesung mindestens einen Messwert?
    fn has_value(&self) -> bool {
        self.distance.is_some() || self.staff.is_some() || self.height.is_some()
    }

    /// Ist die Lattenablesung vorhanden (Voraussetzung für eine vollständige Messung)?
    pub fn is_complete(&self) -> bool {
        self.staff.is_some()
    }

    /// Ergänzt fehlende Felder aus einer weiteren Zeile (das Gerät kann die Wörter auf mehrere Zeilen verteilen).
    pub fn merge(&mut self, other: LevelReading) {
        self.point_id = self.point_id.take().or(other.point_id);
        self.distance = self.distance.or(other.distance);
        if self.staff.is_none() {
            self.staff = other.staff;
            self.staff_wi = other.staff_wi;
        }
        self.height = self.height.or(other.height);
        self.count = self.count.or(other.count);
        self.std_dev_mm = self.std_dev_mm.or(other.std_dev_mm);
        if !other.raw.is_empty() {
            self.raw = if self.raw.is_empty() { other.raw } else { format!("{} | {}", self.raw, other.raw) };
        }
    }
}

/// Fehler- oder Warnmeldung des Geräts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceError {
    pub code: String,
    pub text: &'static str,
}

fn describe_error(code: &str) -> &'static str {
    match code {
        "@W400" => "Gerät beschäftigt – ein anderes Gerät greift noch auf das Nivellier zu",
        "@W427" => "Ungültiger Befehl – Syntax prüfen",
        "@E458" => "Neigungssensor außerhalb des Bereichs – Gerät nicht horizontiert",
        "@E439" => "Messung nicht möglich – keine Latte im Bild, Latte verkehrt herum oder Bild zu dunkel",
        _ => "Unbekannter Gerätefehler",
    }
}

/// Bedeutung einer empfangenen Zeile.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    /// "?" – Befehl quittiert.
    Ack,
    Error(DeviceError),
    Reading(Box<LevelReading>),
    /// Zeile ohne Bedeutung für die Messung.
    Ignored,
}

/// Wertet eine Zeile der GSI-Online-Schnittstelle aus.
pub fn parse_line(line: &str) -> Response {
    let text = line.trim();
    if text.is_empty() {
        return Response::Ignored;
    }
    if text == "?" {
        return Response::Ack;
    }
    if text.starts_with("@E") || text.starts_with("@W") {
        let code = text.split_whitespace().next().unwrap_or(text);
        return Response::Error(DeviceError { code: code.to_string(), text: describe_error(code) });
    }
    let mut reading = LevelReading::empty(text);
    for token in text.split_whitespace() {
        let Some(word) = split_word(token) else {
            continue;
        };
        match word.wi {
            WI_POINT_ID => reading.point_id = Some(point_id_text(word.data)),
            WI_DISTANCE => reading.distance = word.length(),
            WI_HEIGHT => reading.height = word.length(),
            WI_STAFF_FIRST..=WI_STAFF_LAST => {
                reading.staff = word.length();
                reading.staff_wi = reading.staff.map(|_| word.wi);
            }
            WI_COUNT => reading.count = word.number().and_then(|n| u32::try_from(n).ok()),
            WI_STD_DEV => reading.std_dev_mm = word.length().map(|l| l.value * MM_PER_METER),
            _ => {}
        }
    }
    if reading.has_value() { Response::Reading(Box::new(reading)) } else { Response::Ignored }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(line: &str) -> LevelReading {
        match parse_line(line) {
            Response::Reading(r) => *r,
            anderes => panic!("Ablesung erwartet, erhalten: {anderes:?}"),
        }
    }

    #[test]
    fn gsi8_nur_messen() {
        let r = reading("110014+00000124 32...6+00241234 330.06+00010509");
        assert_eq!(r.point_id.as_deref(), Some("124"));
        assert!((r.distance.unwrap().value - 24.1234).abs() < 1e-9);
        assert!((r.staff.unwrap().value - 1.0509).abs() < 1e-9);
        assert_eq!(r.staff_wi, Some(330));
        assert!(r.is_complete());
    }

    #[test]
    fn gsi16_rueckblick_mit_hundertstel_millimeter() {
        let r = reading("*110004+0000000000000001 32...8+0000000003417147 331.08+0000000000147534");
        assert_eq!(r.point_id.as_deref(), Some("1"));
        assert!((r.distance.unwrap().value - 34.17147).abs() < 1e-9);
        assert!((r.staff.unwrap().value - 1.47534).abs() < 1e-9);
        assert_eq!(r.staff_wi, Some(331));
    }

    #[test]
    fn mittelwert_mit_anzahl_und_standardabweichung() {
        let r = reading("110024+00000016 32...6+00241234 330.06+00012054 390...+00000005 391.06+00000012 71....+0SURFACE");
        assert_eq!(r.count, Some(5));
        assert!((r.std_dev_mm.unwrap() - 1.2).abs() < 1e-9);
    }

    #[test]
    fn alphanumerische_punkt_id_und_vorzeichen() {
        let r = reading("110001+0000A110 32...0+00014940 330.00-00001500");
        assert_eq!(r.point_id.as_deref(), Some("A110"));
        assert!((r.staff.unwrap().value + 1.5).abs() < 1e-9);
        assert!((r.distance.unwrap().value - 14.94).abs() < 1e-9);
    }

    #[test]
    fn fuss_wird_als_fuss_gekennzeichnet() {
        let r = reading("330.01+00004921");
        assert_eq!(r.staff.unwrap().unit, LengthUnit::Feet);
        assert_eq!(r.staff.unwrap().text(), "4,921 ft");
    }

    #[test]
    fn unbekannte_einheit_liefert_keinen_wert() {
        assert_eq!(parse_line("330.09+00004921"), Response::Ignored);
    }

    #[test]
    fn quittung_fehler_und_rauschen() {
        assert_eq!(parse_line("?"), Response::Ack);
        assert_eq!(parse_line("  "), Response::Ignored);
        assert_eq!(parse_line("0030/0001"), Response::Ignored);
        let Response::Error(e) = parse_line("@E439") else {
            panic!("Fehler erwartet");
        };
        assert!(e.text.contains("Latte"));
        let Response::Error(e) = parse_line("@W400") else {
            panic!("Fehler erwartet");
        };
        assert!(e.text.contains("beschäftigt"));
    }

    #[test]
    fn zeilen_werden_zusammengefuehrt() {
        let mut a = reading("110014+00000124 32...6+00241234");
        assert!(!a.is_complete());
        a.merge(reading("330.06+00010509"));
        assert!(a.is_complete());
        assert_eq!(a.point_id.as_deref(), Some("124"));
        assert!(a.raw.contains(" | "));
    }
}
