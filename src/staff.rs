// Zweck:    Nachbildung zweier Nivellierlatten neben dem virtuellen Display: analoge Latte mit E-Teilung
//           (rot/schwarze Meterstücke, Dezimeterziffern) und Invar-Strichcodelatte im Stil der Leica GPCL2.
//           Beide zeigen einen scrollenden Ausschnitt von 1 m, in dem die Ablesung auf der Ablesemarke liegt,
//           daneben eine Übersicht der ganzen Latte. Der Strichcode ist eine Nachzeichnung, kein Leica-Code.
// Hardware: Leica LS10 mit Latte GPCL2 (Darstellung)
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::display::BODY_SIZE;
use crate::protocol::{Length, LengthUnit};

// ── Maße [px] und Längen [m] ──────────────────────────────────────────────────

const PANEL_WIDTH: f32 = 150.0;
const PANEL_HEIGHT: f32 = BODY_SIZE.y;
const PANEL_MARGIN: f32 = 8.0;
const HEADER_HEIGHT: f32 = 42.0;
const FOOTER_HEIGHT: f32 = 30.0;
const BOARD_WIDTH: f32 = 100.0;
const OVERVIEW_GAP: f32 = 8.0;
const OVERVIEW_WIDTH: f32 = 12.0;
/// Sichtbarer Ausschnitt der Latte [m]: bei der E-Teilung kleiner, damit die Felder gut erkennbar bleiben.
const ANALOG_VIEW_SPAN_M: f64 = 0.5;
const BARCODE_VIEW_SPAN_M: f64 = 1.0;
/// Länge der analogen Teleskoplatte [m].
const ANALOG_LENGTH_M: f64 = 4.0;
/// Länge der Strichcodelatte GPCL2 [m].
const BARCODE_LENGTH_M: f64 = 2.0;
/// Mitte des Ausschnitts ohne Messwert [m].
const DEFAULT_CENTER_M: f64 = 1.0;
/// Meter pro Fuß (internationaler Fuß) für die Darstellung von Fußwerten.
const METER_PER_FOOT: f64 = 0.3048;

// ── E-Teilung ─────────────────────────────────────────────────────────────────

// Maße der E-Teilung, abgemessen an einem Fernrohrbild einer Latte (Bildpunkte dieser Vorlage).
// Alle Breiten und Höhen werden mit dem Maßstab "Bildschirm-Dezimeter / Vorlage-Dezimeter" umgerechnet.

/// Höhe eines Dezimeters in der Vorlage [Bildpunkte].
const TEMPLATE_DM_PX: f32 = 59.75;
/// Breite der weißen Lattenfläche zwischen den schwarzen Randlinien [Vorlage-px] und Linienstärke.
const TEMPLATE_BOARD_WIDTH: f32 = 28.5;
const TEMPLATE_BORDER_WIDTH: f32 = 1.5;
/// Rücken des E und Breite der Zähne [Vorlage-px, von der linken Fläche aus].
const TEMPLATE_STEM_WIDTH: f32 = 4.5;
const TEMPLATE_TEETH_WIDTH: f32 = 14.0;
/// Die zwei kleinen Quadrate: Beginn und Ende von links [Vorlage-px].
const TEMPLATE_SQUARE_X: (f32, f32) = (5.0, 14.0);
/// Höhenlagen über der Dezimeterlinie [cm]: Quadrate, Zähne des E und Rücken des E.
const SQUARES_CM: [(f64, f64); 2] = [(1.0, 2.0), (3.0, 4.0)];
const TEETH_CM: [(f64, f64); 3] = [(5.0, 6.0), (7.0, 8.0), (9.0, 10.0)];
const STEM_CM: (f64, f64) = (5.0, 10.0);
/// Ziffern: linker Rand [Vorlage-px], Höhenlage über der Linie [cm], Breite, Strichstärke [Vorlage-px].
const TEMPLATE_DIGIT_X: f32 = 15.0;
const DIGIT_CM: (f64, f64) = (0.5, 4.0);
const TEMPLATE_DIGIT_WIDTH: f32 = 7.0;
const TEMPLATE_DIGIT_ONE_WIDTH: f32 = 2.0;
const TEMPLATE_DIGIT_GAP: f32 = 2.0;
const TEMPLATE_DIGIT_STROKE: f32 = 1.8;
/// Zentimeter pro Meter und pro Dezimeter.
const CM_PER_M: f64 = 100.0;
const CM_PER_DM: i64 = 10;

/// Strichschrift der Ziffern 0–9 (schmal wie auf der Latte): Linienzüge im Einheitsquadrat, y wächst nach unten.
type Glyph = &'static [&'static [(f32, f32)]];
const GLYPHS: [Glyph; 10] = [
    &[&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)]],
    &[&[(0.0, 0.0), (0.0, 1.0)]],
    &[&[(0.0, 0.0), (1.0, 0.0), (1.0, 0.5), (0.0, 0.5), (0.0, 1.0), (1.0, 1.0)]],
    &[&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)], &[(0.25, 0.5), (1.0, 0.5)]],
    &[&[(0.0, 0.0), (0.0, 0.55), (1.0, 0.55)], &[(1.0, 0.0), (1.0, 1.0)]],
    &[&[(1.0, 0.0), (0.0, 0.0), (0.0, 0.5), (1.0, 0.5), (1.0, 1.0), (0.0, 1.0)]],
    &[&[(1.0, 0.0), (0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.5), (0.0, 0.5)]],
    &[&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]],
    &[&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)], &[(0.0, 0.5), (1.0, 0.5)]],
    &[&[(1.0, 0.5), (0.0, 0.5), (0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]],
];

// ── Strichcode ────────────────────────────────────────────────────────────────

/// Breite des Invarbandes [px] und Höhe einer Codezelle [m].
const BAND_WIDTH: f32 = 40.0;
const CODE_CELL_M: f64 = 0.004;
/// Startwert der Pseudozufallsfolge des Codes.
const CODE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Länge der Teilstriche am gelben Rand [px].
const TICK_SHORT: f32 = 8.0;
const TICK_LONG: f32 = 16.0;

// ── Farben ────────────────────────────────────────────────────────────────────

const COLOR_PANEL: Color32 = Color32::from_rgb(42, 45, 50);
const COLOR_HEADER_TEXT: Color32 = Color32::from_rgb(225, 230, 236);
const COLOR_HEADER_SUB: Color32 = Color32::from_rgb(150, 160, 172);
const COLOR_BOARD: Color32 = Color32::WHITE;
const COLOR_BLACK: Color32 = Color32::from_rgb(20, 20, 20);
const COLOR_RED_E: Color32 = Color32::from_rgb(255, 0, 0);
const COLOR_YELLOW: Color32 = Color32::from_rgb(250, 214, 38);
const COLOR_EDGE: Color32 = Color32::from_rgb(90, 95, 100);
const COLOR_MARK: Color32 = Color32::from_rgb(30, 120, 255);
const COLOR_VIEW_BOX: Color32 = Color32::from_rgba_premultiplied(30, 120, 255, 60);

const FONT_HEADER: f32 = 14.0;
const FONT_SMALL: f32 = 12.0;
const FONT_READING: f32 = 15.0;

/// Art der Latte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffKind {
    /// Teleskoplatte mit E-Teilung.
    Analog,
    /// Invar-Strichcodelatte.
    Barcode,
}

impl StaffKind {
    fn length_m(self) -> f64 {
        match self {
            Self::Analog => ANALOG_LENGTH_M,
            Self::Barcode => BARCODE_LENGTH_M,
        }
    }

    fn view_span_m(self) -> f64 {
        match self {
            Self::Analog => ANALOG_VIEW_SPAN_M,
            Self::Barcode => BARCODE_VIEW_SPAN_M,
        }
    }

    fn title(self) -> (&'static str, &'static str) {
        match self {
            Self::Analog => ("E-Teilung", "Teleskoplatte 4 m"),
            Self::Barcode => ("Strichcode", "Invar GPCL2 · 2 m"),
        }
    }
}

/// Ablesung in Metern (Fußwerte werden umgerechnet).
fn to_meters(length: Length) -> f64 {
    match length.unit {
        LengthUnit::Meter => length.value,
        LengthUnit::Feet => length.value * METER_PER_FOOT,
    }
}

/// Abbildung zwischen Lattenhöhe und Bildschirm.
struct View {
    /// Untere Kante des Ausschnitts [m].
    low: f64,
    /// Sichtbare Höhe [m].
    span: f64,
    window: Rect,
}

impl View {
    fn y(&self, height_m: f64) -> f32 {
        self.window.bottom() - ((height_m - self.low) / self.span) as f32 * self.window.height()
    }

    fn high(&self) -> f64 {
        self.low + self.span
    }
}

/// Zeichnet eine Latte mit scrollendem Ausschnitt, Ablesemarke, Übersicht und Ablesetext.
pub fn show(ui: &mut egui::Ui, kind: StaffKind, reading: Option<Length>) {
    let (response, painter) = ui.allocate_painter(Vec2::new(PANEL_WIDTH, PANEL_HEIGHT), Sense::hover());
    let panel = response.rect;
    painter.rect_filled(panel, CornerRadius::same(8), COLOR_PANEL);

    let (title, subtitle) = kind.title();
    painter.text(Pos2::new(panel.center().x, panel.top() + 13.0), Align2::CENTER_CENTER, title, FontId::proportional(FONT_HEADER), COLOR_HEADER_TEXT);
    painter.text(Pos2::new(panel.center().x, panel.top() + 29.0), Align2::CENTER_CENTER, subtitle, FontId::proportional(FONT_SMALL), COLOR_HEADER_SUB);

    let window = Rect::from_min_max(
        Pos2::new(panel.left() + PANEL_MARGIN, panel.top() + HEADER_HEIGHT),
        Pos2::new(panel.left() + PANEL_MARGIN + BOARD_WIDTH, panel.bottom() - FOOTER_HEIGHT),
    );
    let staff_m = reading.map(to_meters);
    let length = kind.length_m();
    let center = staff_m.unwrap_or(DEFAULT_CENTER_M);
    let span = kind.view_span_m();
    let view = View { low: (center - span / 2.0).clamp(0.0, length - span), span, window };

    let clipped = painter.with_clip_rect(window);
    match kind {
        StaffKind::Analog => draw_analog(&clipped, &view),
        StaffKind::Barcode => draw_barcode(&clipped, &view),
    }
    painter.rect_stroke(window, CornerRadius::ZERO, Stroke::new(1.0, COLOR_EDGE), StrokeKind::Outside);

    if let Some(m) = staff_m {
        draw_reading_mark(&painter, &view, m);
    }
    draw_overview(&painter, &view, kind, staff_m);

    let text = reading.map_or_else(|| "– – –".to_string(), |l| l.text());
    painter.text(Pos2::new(panel.center().x, panel.bottom() - FOOTER_HEIGHT / 2.0), Align2::CENTER_CENTER, text, FontId::monospace(FONT_READING), COLOR_HEADER_TEXT);
}

/// Ablesemarke: waagerechte Linie mit Pfeil links, sodass der abgelesene Wert eindeutig erkennbar ist.
fn draw_reading_mark(painter: &egui::Painter, view: &View, reading_m: f64) {
    let y = view.y(reading_m).clamp(view.window.top(), view.window.bottom());
    let stroke = Stroke::new(2.0, COLOR_MARK);
    painter.line_segment([Pos2::new(view.window.left() - 4.0, y), Pos2::new(view.window.right() + 4.0, y)], stroke);
    let tip = Pos2::new(view.window.left() - 4.0, y);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, tip + Vec2::new(-8.0, -5.0), tip + Vec2::new(-8.0, 5.0)],
        COLOR_MARK,
        Stroke::NONE,
    ));
}

/// Übersicht der ganzen Latte mit Meterteilung, sichtbarem Ausschnitt und Ablesung.
fn draw_overview(painter: &egui::Painter, view: &View, kind: StaffKind, reading_m: Option<f64>) {
    let length = kind.length_m();
    let x = view.window.right() + OVERVIEW_GAP;
    let strip = Rect::from_min_max(Pos2::new(x, view.window.top()), Pos2::new(x + OVERVIEW_WIDTH, view.window.bottom()));
    let y_of = |h: f64| strip.bottom() - (h / length) as f32 * strip.height();

    let base = if kind == StaffKind::Analog { COLOR_BOARD } else { COLOR_YELLOW };
    painter.rect_filled(strip, CornerRadius::ZERO, base);
    // Meterstücke abwechselnd farbig (analoge Latte: rot/schwarz, Strichcodelatte: einheitlich gelb)
    for meter in 0..length as i64 {
        let color = match kind {
            StaffKind::Analog => COLOR_RED_E,
            StaffKind::Barcode => COLOR_BLACK,
        };
        let band = Rect::from_min_max(Pos2::new(strip.left(), y_of((meter + 1) as f64)), Pos2::new(strip.left() + 3.0, y_of(meter as f64)));
        painter.rect_filled(band, CornerRadius::ZERO, color);
        painter.hline(strip.x_range(), y_of(meter as f64), Stroke::new(1.0, COLOR_EDGE));
    }
    painter.rect_stroke(strip, CornerRadius::ZERO, Stroke::new(1.0, COLOR_EDGE), StrokeKind::Outside);

    let visible = Rect::from_min_max(Pos2::new(strip.left() - 2.0, y_of(view.high())), Pos2::new(strip.right() + 2.0, y_of(view.low)));
    painter.rect_filled(visible, CornerRadius::ZERO, COLOR_VIEW_BOX);
    painter.rect_stroke(visible, CornerRadius::ZERO, Stroke::new(1.0, COLOR_MARK), StrokeKind::Inside);
    if let Some(m) = reading_m {
        let y = y_of(m.clamp(0.0, length));
        painter.hline((strip.left() - 4.0)..=(strip.right() + 4.0), y, Stroke::new(2.0, COLOR_MARK));
    }
}

// ── Analoge Latte ─────────────────────────────────────────────────────────────

/// Rechteck zwischen zwei Lattenhöhen [m] und zwei Bildschirm-x-Werten.
fn rect_between(view: &View, x0: f32, x1: f32, low_m: f64, high_m: f64) -> Rect {
    Rect::from_min_max(Pos2::new(x0, view.y(high_m)), Pos2::new(x1, view.y(low_m)))
}

/// Zeichnet eine Ziffer der Strichschrift; `left`, `top_y`, `width` und `height` in Bildschirm-px.
fn draw_glyph(painter: &egui::Painter, digit: usize, left: f32, top_y: f32, width: f32, height: f32, stroke: Stroke) {
    for line in GLYPHS[digit] {
        let points: Vec<Pos2> = line.iter().map(|(x, y)| Pos2::new(left + x * width, top_y + y * height)).collect();
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// Zeichnet die Zahl (Meter, Dezimeter) wie "15"; die schmale "1" ist nur ein Strich.
fn draw_label(painter: &egui::Painter, text: &str, left: f32, top_y: f32, height: f32, scale: f32) {
    let stroke = Stroke::new(TEMPLATE_DIGIT_STROKE * scale, COLOR_BLACK);
    let mut x = left;
    for ch in text.chars() {
        let Some(digit) = ch.to_digit(10) else {
            continue;
        };
        let width = if digit == 1 { TEMPLATE_DIGIT_ONE_WIDTH } else { TEMPLATE_DIGIT_WIDTH } * scale;
        draw_glyph(painter, digit as usize, x + stroke.width / 2.0, top_y, width, height, stroke);
        x += width + TEMPLATE_DIGIT_GAP * scale;
    }
}

/// Analoge Latte exakt nach dem Fernrohrbild: weiße Fläche zwischen zwei schwarzen Linien; über jeder
/// Dezimeterlinie zwei kleine rote Quadrate (1–2 und 3–4 cm), darüber das rote E (5–10 cm, Rücken links, drei
/// Zähne); rechts neben den Quadraten die schmale Zahl "Meter + Dezimeter" (z. B. 15 für 1,50 m).
fn draw_analog(painter: &egui::Painter, view: &View) {
    painter.rect_filled(view.window, CornerRadius::ZERO, COLOR_BOARD);
    let dm_px = view.window.height() / (view.span * 10.0) as f32;
    let scale = dm_px / TEMPLATE_DM_PX;
    let left = view.window.center().x - TEMPLATE_BOARD_WIDTH * scale / 2.0;
    let right = left + TEMPLATE_BOARD_WIDTH * scale;
    let cm = |value: f64| value / CM_PER_M;

    let first_dm = (view.low * 10.0).floor() as i64 - 1;
    let last_dm = (view.high() * 10.0).ceil() as i64 + 1;
    for dm in first_dm..=last_dm {
        let base = dm as f64 / 10.0;
        let fill = |x0: f32, x1: f32, (from, to): (f64, f64)| {
            painter.rect_filled(rect_between(view, left + x0 * scale, left + x1 * scale, base + cm(from), base + cm(to)), CornerRadius::ZERO, COLOR_RED_E);
        };
        for range in SQUARES_CM {
            fill(TEMPLATE_SQUARE_X.0, TEMPLATE_SQUARE_X.1, range);
        }
        fill(0.0, TEMPLATE_STEM_WIDTH, STEM_CM);
        for range in TEETH_CM {
            fill(0.0, TEMPLATE_TEETH_WIDTH, range);
        }
        // Dezimeterlinie über die Breite der Fläche
        painter.hline((left - TEMPLATE_BORDER_WIDTH * scale)..=(right + TEMPLATE_BORDER_WIDTH * scale), view.y(base), Stroke::new(TEMPLATE_BORDER_WIDTH * scale, COLOR_BLACK));
        // Zahl über der Linie
        let label = format!("{}{}", dm.div_euclid(CM_PER_DM), dm.rem_euclid(CM_PER_DM));
        let top = view.y(base + cm(DIGIT_CM.1));
        let height = view.y(base + cm(DIGIT_CM.0)) - top;
        draw_label(painter, &label, left + TEMPLATE_DIGIT_X * scale, top, height, scale);
    }
    // Schwarze Randlinien links und rechts
    let border = Stroke::new(TEMPLATE_BORDER_WIDTH * scale, COLOR_BLACK);
    painter.vline(left, view.window.y_range(), border);
    painter.vline(right, view.window.y_range(), border);
}

// ── Strichcodelatte ───────────────────────────────────────────────────────────

/// Pseudozufälliger Wert für eine Codezelle (SplitMix64): gleiche Zelle ergibt immer dasselbe Bit.
fn code_bit(cell: i64) -> bool {
    let mut z = (cell as u64).wrapping_mul(CODE_SEED).wrapping_add(CODE_SEED);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) & 1 == 1
}

fn draw_barcode(painter: &egui::Painter, view: &View) {
    // Gelber Lattenkörper
    painter.rect_filled(view.window, CornerRadius::ZERO, COLOR_YELLOW);
    let band_left = view.window.right() - PANEL_MARGIN - BAND_WIDTH;
    let band_x = band_left..=(band_left + BAND_WIDTH);
    painter.rect_filled(Rect::from_x_y_ranges(band_x.clone(), view.window.y_range()), CornerRadius::ZERO, COLOR_BOARD);

    let first = (view.low / CODE_CELL_M).floor() as i64 - 1;
    let last = (view.high() / CODE_CELL_M).ceil() as i64 + 1;
    for cell in first..=last {
        if !code_bit(cell) {
            continue;
        }
        let rect = Rect::from_min_max(
            Pos2::new(band_left, view.y((cell + 1) as f64 * CODE_CELL_M)),
            Pos2::new(band_left + BAND_WIDTH, view.y(cell as f64 * CODE_CELL_M)),
        );
        painter.rect_filled(rect, CornerRadius::ZERO, COLOR_BLACK);
    }
    painter.rect_stroke(Rect::from_x_y_ranges(band_x, view.window.y_range()), CornerRadius::ZERO, Stroke::new(1.0, COLOR_EDGE), StrokeKind::Outside);

    // Teilstriche am gelben Rand: Dezimeter kurz, Meter lang mit Zahl
    let first_dm = (view.low * 10.0).floor() as i64;
    let last_dm = (view.high() * 10.0).ceil() as i64;
    for dm in first_dm..=last_dm {
        let y = view.y(dm as f64 / 10.0);
        let is_meter = dm % CM_PER_DM == 0;
        let length = if is_meter { TICK_LONG } else { TICK_SHORT };
        painter.hline(view.window.left()..=(view.window.left() + length), y, Stroke::new(1.5, COLOR_BLACK));
        if is_meter {
            painter.text(Pos2::new(view.window.left() + TICK_LONG + 3.0, y), Align2::LEFT_CENTER, format!("{}m", dm / CM_PER_DM), FontId::monospace(FONT_SMALL), COLOR_BLACK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_ist_reproduzierbar_und_gemischt() {
        assert_eq!(code_bit(123), code_bit(123));
        let schwarz = (0..1000).filter(|c| code_bit(*c)).count();
        assert!((400..600).contains(&schwarz), "{schwarz}");
    }

    #[test]
    fn fuss_wird_in_meter_umgerechnet() {
        let l = Length { value: 10.0, unit: LengthUnit::Feet, decimals: 3 };
        assert!((to_meters(l) - 3.048).abs() < 1e-9);
    }
}
