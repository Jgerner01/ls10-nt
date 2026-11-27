# LS10-NT

Fernbedienung für das Leica-Digitalnivellier LS10/LS15: Messung auslösen, Werte abholen,
Darstellung als virtuelles LS10-Display.

## Bedienung

1. LS10 per Bluetooth koppeln (Windows legt einen ausgehenden COM-Port an), am Gerät die
   Schnittstelle „Bluetooth“ und das Protokoll GSI Online wählen.
2. COM-Port und Baudrate wählen, **Verbinden**.
3. Tasten unter dem Display:
   - **MESS** (auch Leertaste): `GET/M/WI11/WI32/WI330` – Messung auslösen, Punkt-ID, Entfernung, Lattenablesung abholen
   - **DAUER** / **STOPP**: `GET/C/…` bzw. `c` – Dauermessung
4. Rechts steht der Messverlauf; „CSV kopieren“ legt ihn in die Zwischenablage.

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/verbindung.rs` | serielle Verbindung, aus Feldbuch-NT (`src/tachymeter/verbindung.rs`) übernommen |
| `src/protocol.rs` | GSI-Online-Befehle und Parser (GSI-8/16, Einheiten, Fehlercodes) |
| `src/job.rs` | Zustandsautomat für Messen/Abholen/Dauermessung (wie `Messablauf` im Feldbuch) |
| `src/display.rs` | virtuelles LS10-Display |
| `src/staff.rs` | analoge E-Teilungs-Latte und Strichcodelatte (Ausschnitt 1 m + Übersicht) |
| `src/app.rs` | Fenster, Verbindungsleiste, Verlauf |

Quelle des Protokolls: Leica „GSI ONLINE for Leica TPS and DNA“ (Abschnitt DNA).
