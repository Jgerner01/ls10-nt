// Zweck:    Virtuelles Display des Leica LS10: Gerätegehäuse mit Bildschirm, der die letzte Ablesung
//           (Punkt-ID, Lattenablesung, Entfernung, Mittelung) im Stil des Gerätedisplays zeigt
// Hardware: Leica LS10 / LS15 (Nachbildung der Anzeige)
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::protocol::LevelReading;

// ── Abmessungen [px] ──────────────────────────────────────────────────────────

pub const BODY_SIZE: Vec2 = Vec2::new(560.0, 400.0);
const BODY_RADIUS: u8 = 18;
const SCREEN_MARGIN: f32 = 20.0;
const SCREEN_RADIUS: u8 = 4;
const TITLE_HEIGHT: f32 = 34.0;
const SOFTKEY_HEIGHT: f32 = 30.0;
const STATUS_HEIGHT: f32 = 24.0;
const ROW_MARGIN: f32 = 18.0;
const INDICATOR_RADIUS: f32 = 6.0;

// ── Farben ────────────────────────────────────────────────────────────────────

const COLOR_BODY: Color32 = Color32::from_rgb(52, 56, 62);
const COLOR_BODY_EDGE: Color32 = Color32::from_rgb(25, 27, 30);
const COLOR_SCREEN: Color32 = Color32::from_rgb(226, 234, 241);
const COLOR_TITLE: Color32 = Color32::from_rgb(0, 82, 147);
const COLOR_TITLE_TEXT: Color32 = Color32::WHITE;
const COLOR_TEXT: Color32 = Color32::from_rgb(10, 25, 45);
const COLOR_LABEL: Color32 = Color32::from_rgb(70, 90, 115);
const COLOR_SEPARATOR: Color32 = Color32::from_rgb(170, 185, 200);
const COLOR_SOFTKEY: Color32 = Color32::from_rgb(190, 205, 220);
const COLOR_OK: Color32 = Color32::from_rgb(70, 200, 90);
const COLOR_OFF: Color32 = Color32::from_rgb(200, 70, 70);
const COLOR_BUSY: Color32 = Color32::from_rgb(255, 190, 40);
const COLOR_ERROR: Color32 = Color32::from_rgb(180, 30, 30);

// ── Schriftgrößen ─────────────────────────────────────────────────────────────

const FONT_TITLE: f32 = 17.0;
const FONT_LABEL: f32 = 15.0;
const FONT_VALUE_BIG: f32 = 44.0;
const FONT_VALUE: f32 = 26.0;
const FONT_UNIT: f32 = 18.0;
const FONT_SMALL: f32 = 14.0;

/// Platzhalter ohne Messwert.
const PLACEHOLDER: &str = "-,-----";
/// Breite, die für die Einheit hinter dem Wert freigehalten wird [px].
const UNIT_WIDTH: f32 = 34.0;
/// Beschriftung der Softkeys (entspricht den Schaltflächen unter dem Gerät).
const SOFTKEYS: [&str; 3] = ["MESS", "DAUER", "STOPP"];

/// Was das virtuelle Display zeigt.
pub struct ScreenState<'a> {
    pub connected: bool,
    pub busy: bool,
    pub continuous: bool,
    /// Meldung der Statuszeile.
    pub status: &'a str,
    pub status_is_error: bool,
    pub reading: Option<&'a LevelReading>,
}

/// Bezeichnung und Kurzname der Ablesungsart nach Word Index.
fn staff_labels(wi: Option<u16>) -> (&'static str, &'static str) {
    match wi {
        Some(331) => ("Rückblick", "Rb"),
        Some(332) => ("Vorblick", "Vb"),
        Some(333) => ("Zwischenblick", "Zb"),
        Some(334) => ("Absteckung", "Abst."),
        Some(335) => ("Rückblick 2", "Rb2"),
        Some(336) => ("Vorblick 2", "Vb2"),
        _ => ("Nur messen", "Latte"),
    }
}

/// Klartext der Ablesungsart (für Verlauf und Titelzeile).
pub fn staff_kind_name(wi: Option<u16>) -> &'static str {
    staff_labels(wi).0
}

/// Zeichnet das Gerät und liefert das Rechteck des Bildschirms zurück.
pub fn show(ui: &mut egui::Ui, state: &ScreenState<'_>) -> Rect {
    let (response, painter) = ui.allocate_painter(BODY_SIZE, Sense::hover());
    let body = response.rect;
    painter.rect_filled(body, CornerRadius::same(BODY_RADIUS), COLOR_BODY);
    painter.rect_stroke(body, CornerRadius::same(BODY_RADIUS), Stroke::new(2.0, COLOR_BODY_EDGE), StrokeKind::Inside);

    let screen = body.shrink(SCREEN_MARGIN);
    painter.rect_filled(screen, CornerRadius::same(SCREEN_RADIUS), COLOR_SCREEN);

    draw_title(&painter, screen, state);
    draw_values(&painter, screen, state);
    draw_status_and_softkeys(&painter, screen, state);
    screen
}

fn draw_title(painter: &egui::Painter, screen: Rect, state: &ScreenState<'_>) {
    let bar = Rect::from_min_size(screen.min, Vec2::new(screen.width(), TITLE_HEIGHT));
    painter.rect_filled(bar, CornerRadius { nw: SCREEN_RADIUS, ne: SCREEN_RADIUS, sw: 0, se: 0 }, COLOR_TITLE);
    let (mode_name, _) = staff_labels(state.reading.and_then(|r| r.staff_wi));
    let title = if state.continuous { "Messen – Dauer" } else { "Messen" };
    painter.text(
        Pos2::new(bar.left() + ROW_MARGIN, bar.center().y),
        Align2::LEFT_CENTER,
        format!("{title}  ·  {mode_name}"),
        FontId::proportional(FONT_TITLE),
        COLOR_TITLE_TEXT,
    );
    let (color, text) = if state.busy {
        (COLOR_BUSY, "MESSUNG")
    } else if state.connected {
        (COLOR_OK, "verbunden")
    } else {
        (COLOR_OFF, "getrennt")
    };
    let text_pos = Pos2::new(bar.right() - ROW_MARGIN, bar.center().y);
    let galley_rect = painter.text(text_pos, Align2::RIGHT_CENTER, text, FontId::proportional(FONT_SMALL), COLOR_TITLE_TEXT);
    painter.circle_filled(Pos2::new(galley_rect.left() - INDICATOR_RADIUS * 2.0, bar.center().y), INDICATOR_RADIUS, color);
}

/// Zeichnet eine Zeile aus Beschriftung links und rechtsbündigem Wert mit Einheit.
fn draw_row(painter: &egui::Painter, screen: Rect, y: f32, label: &str, value: &str, unit: &str, big: bool) {
    let left = screen.left() + ROW_MARGIN;
    let right = screen.right() - ROW_MARGIN;
    painter.text(Pos2::new(left, y), Align2::LEFT_CENTER, label, FontId::proportional(FONT_LABEL), COLOR_LABEL);
    let font = FontId::monospace(if big { FONT_VALUE_BIG } else { FONT_VALUE });
    painter.text(Pos2::new(right - UNIT_WIDTH, y), Align2::RIGHT_CENTER, value, font, COLOR_TEXT);
    painter.text(Pos2::new(right, y), Align2::RIGHT_CENTER, unit, FontId::proportional(FONT_UNIT), COLOR_LABEL);
}

fn draw_separator(painter: &egui::Painter, screen: Rect, y: f32) {
    painter.hline((screen.left() + ROW_MARGIN)..=(screen.right() - ROW_MARGIN), y, Stroke::new(1.0, COLOR_SEPARATOR));
}

fn draw_values(painter: &egui::Painter, screen: Rect, state: &ScreenState<'_>) {
    let top = screen.top() + TITLE_HEIGHT;
    let reading = state.reading;
    let (_, short) = staff_labels(reading.and_then(|r| r.staff_wi));

    let point_id = reading.and_then(|r| r.point_id.as_deref()).unwrap_or("-");
    draw_row(painter, screen, top + 28.0, "Pkt-ID", point_id, "", false);
    draw_separator(painter, screen, top + 52.0);

    let (staff, staff_unit) = reading
        .and_then(|r| r.staff)
        .map_or((PLACEHOLDER.to_string(), "m"), |l| (l.number_text(), l.unit.label()));
    draw_row(painter, screen, top + 98.0, short, &staff, staff_unit, true);
    draw_separator(painter, screen, top + 142.0);

    let (distance, distance_unit) = reading
        .and_then(|r| r.distance)
        .map_or((PLACEHOLDER.to_string(), "m"), |l| (l.number_text(), l.unit.label()));
    draw_row(painter, screen, top + 178.0, "Entfernung", &distance, distance_unit, false);
    draw_separator(painter, screen, top + 204.0);

    // Mittelung und Zeitstempel in der Fußzeile der Werte
    let detail = reading.map(|r| {
        let mut parts = Vec::new();
        if let Some(n) = r.count {
            parts.push(format!("n = {n}"));
        }
        if let Some(s) = r.std_dev_mm {
            parts.push(format!("s = {:.1} mm", s).replace('.', ","));
        }
        parts.push(r.timestamp.format("%H:%M:%S").to_string());
        parts.join("   ")
    });
    painter.text(
        Pos2::new(screen.left() + ROW_MARGIN, top + 224.0),
        Align2::LEFT_CENTER,
        detail.unwrap_or_default(),
        FontId::proportional(FONT_SMALL),
        COLOR_LABEL,
    );
}

fn draw_status_and_softkeys(painter: &egui::Painter, screen: Rect, state: &ScreenState<'_>) {
    let status_y = screen.bottom() - SOFTKEY_HEIGHT - STATUS_HEIGHT / 2.0;
    let color = if state.status_is_error { COLOR_ERROR } else { COLOR_TEXT };
    painter.text(
        Pos2::new(screen.left() + ROW_MARGIN, status_y),
        Align2::LEFT_CENTER,
        state.status,
        FontId::proportional(FONT_SMALL),
        color,
    );

    let strip = Rect::from_min_size(Pos2::new(screen.left(), screen.bottom() - SOFTKEY_HEIGHT), Vec2::new(screen.width(), SOFTKEY_HEIGHT));
    painter.rect_filled(strip, CornerRadius::ZERO, COLOR_SOFTKEY);
    let cell_width = strip.width() / SOFTKEYS.len() as f32;
    for (i, label) in SOFTKEYS.iter().enumerate() {
        let x = strip.left() + cell_width * i as f32;
        if i > 0 {
            painter.vline(x, strip.y_range(), Stroke::new(1.0, COLOR_SEPARATOR));
        }
        painter.text(Pos2::new(x + cell_width / 2.0, strip.center().y), Align2::CENTER_CENTER, *label, FontId::proportional(FONT_LABEL), COLOR_TEXT);
    }
}
