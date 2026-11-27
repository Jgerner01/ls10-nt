//! Zweck:    Programmname, Versions- und Buildnummer, GitHub-Repository (Grundlage für die Update-Suche)
//! Hardware: keine
//! Autor:    Johann Gerner

/// Anzeigename
pub const PROGRAMM: &str = "LS10-NT";
/// Präfix der Release-Pakete: <PAKET>-v<Version>-win-x64.zip
pub const PAKET: &str = "LS10-NT";
/// GitHub-Repository „Besitzer/Name“ (muss öffentlich sein, sonst findet die Update-Suche nichts)
pub const REPOSITORY: &str = "Jgerner01/ls10-nt";
/// Programmdatei, die im Paket enthalten sein muss und nach dem Update neu gestartet wird
pub const GUI_EXE: &str = "ls10-nt.exe";
/// Versionsnummer aus Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Buildnummer aus der Datei BUILDNUMMER (wird von tools/Release-Paket.ps1 erhöht)
#[allow(dead_code)]
pub const BUILD: &str = env!("APP_BUILD_NUMMER");
/// z. B. „0.5.0 (Build 3)“
pub const FULL: &str = concat!(env!("CARGO_PKG_VERSION"), " (Build ", env!("APP_BUILD_NUMMER"), ")");
