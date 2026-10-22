//! German/English interface strings. English source text is the key, so a
//! missing translation falls back to English instead of showing a placeholder.
use crate::desktop::{language, Language};

const DE: &[(&str, &str)] = &[
    // Navigation and window
    ("Home", "Start"),
    ("Search", "Suche"),
    ("Installed", "Installiert"),
    ("Updates", "Updates"),
    ("Settings", "Einstellungen"),
    ("Store", "Store"),
    ("Apps & updates", "Apps & Updates"),
    ("One transaction at a time", "Immer nur ein Vorgang gleichzeitig"),
    ("Queue ({})", "Warteschlange ({})"),
    ("Queue (0)", "Warteschlange (0)"),
    // Home
    ("Loading package summary...", "Paketübersicht wird geladen …"),
    ("Browse Packages", "Pakete durchsuchen"),
    ("Open Updates", "Updates öffnen"),
    ("View Installed", "Installierte anzeigen"),
    ("Featured", "Empfohlen"),
    ("Popular", "Beliebt"),
    ("Our picks", "Unsere Empfehlungen"),
    ("Graphics", "Grafik"),
    ("Development", "Entwicklung"),
    ("Media", "Medien"),
    ("Games", "Spiele"),
    ("Utilities", "Werkzeuge"),
    ("Discover Apps", "Apps entdecken"),
    ("Search and install applications from repo and AUR.", "Programme aus den Paketquellen und dem AUR suchen und installieren."),
    ("Installed packages: {} (Pacman/AUR), {} Flatpak apps", "Installierte Pakete: {} (Pacman/AUR), {} Flatpak-Apps"),
    // Search
    ("Search packages", "Pakete suchen"),
    ("All Sources", "Alle Quellen"),
    ("All States", "Alle Zustände"),
    ("Not Installed", "Nicht installiert"),
    ("Type a package name to search.", "Gib einen Paketnamen ein, um zu suchen."),
    ("See More", "Mehr anzeigen"),
    ("Searching for \"{}\"...", "Suche nach „{}“ …"),
    ("No results found for selected filters.", "Keine Ergebnisse für die gewählten Filter."),
    ("{} of {} results", "{} von {} Ergebnissen"),
    // Cards and details
    ("Install", "Installieren"),
    ("Details", "Details"),
    ("Remove", "Entfernen"),
    ("Back", "Zurück"),
    ("Update", "Update"),
    ("Open Homepage", "Webseite öffnen"),
    ("View Logs", "Protokoll anzeigen"),
    ("Version: -", "Version: -"),
    ("Installed: no", "Installiert: nein"),
    ("Size: -", "Größe: -"),
    ("Homepage: -", "Webseite: -"),
    ("Version: {}", "Version: {}"),
    ("Installed: {}", "Installiert: {}"),
    ("yes", "ja"),
    ("no", "nein"),
    ("Size: {}", "Größe: {}"),
    ("Homepage: {}", "Webseite: {}"),
    ("Failed to open homepage", "Webseite konnte nicht geöffnet werden"),
    ("No screenshots available", "Keine Screenshots verfügbar"),
    // Installed
    ("Search installed packages", "Installierte Pakete durchsuchen"),
    ("All", "Alle"),
    ("Update All", "Alles aktualisieren"),
    ("Refresh installed", "Liste aktualisieren"),
    ("Package", "Paket"),
    ("Version", "Version"),
    ("Source", "Quelle"),
    ("Status", "Status"),
    ("Actions", "Aktionen"),
    ("Action", "Aktion"),
    // Updates
    ("Check and apply updates. System upgrades run through the helper.", "Updates prüfen und gemeinsam installieren. Systemupdates laufen über den Helfer."),
    ("No update data", "Keine Update-Daten"),
    ("Filter updates", "Updates filtern"),
    ("Check Updates", "Nach Updates suchen"),
    ("{} updates available", "{} Updates verfügbar"),
    ("System is up to date", "Das System ist aktuell"),
    ("No updates match selected filters", "Keine Updates für die gewählten Filter"),
    ("{} updates shown ({} total)", "{} Updates angezeigt ({} insgesamt)"),
    // Queue and transactions
    ("Queued {} update(s), skipped {} duplicates", "{} Update(s) eingereiht, {} doppelte übersprungen"),
    ("OK", "OK"),
    ("Cancel", "Abbrechen"),
    ("Execute", "Ausführen"),
    ("Yes", "Ja"),
    ("No", "Nein"),
    ("Abort", "Abbruch"),
    ("Enter response (e.g., y, n, 1)", "Antwort eingeben (z. B. y, n, 1)"),
    ("A transaction is already running. Wait for it to finish.", "Es läuft bereits ein Vorgang. Bitte warte, bis er abgeschlossen ist."),
    ("Failed to check package manager status", "Status der Paketverwaltung konnte nicht geprüft werden"),
    ("Failed to check active package managers: {}", "Aktive Paketverwaltungen konnten nicht geprüft werden: {}"),
    ("Another package manager process is already running", "Eine andere Paketverwaltung läuft bereits"),
    ("Refusing to start: active package manager process detected: {}", "Start abgelehnt, aktive Paketverwaltung erkannt: {}"),
    ("Pacman lock file present", "Pacman-Sperrdatei vorhanden"),
    ("Pacman lock file detected", "Pacman-Sperrdatei erkannt"),
    ("Transactions complete", "Vorgänge abgeschlossen"),
    ("Launching command in external terminal ({})", "Befehl wird im externen Terminal gestartet ({})"),
    ("Failed to start command", "Befehl konnte nicht gestartet werden"),
    ("Failed to start command: {}", "Befehl konnte nicht gestartet werden: {}"),
    ("Command failed ({})", "Befehl fehlgeschlagen ({})"),
    // Logs
    ("Logs", "Protokoll"),
    ("Cancel the running transaction", "Laufenden Vorgang abbrechen"),
    ("Copy", "Kopieren"),
    ("Save", "Speichern"),
    ("Clear", "Leeren"),
    ("Clear Lock", "Sperre lösen"),
    ("Save Logs", "Protokoll speichern"),
    ("Safety check failed: {}", "Sicherheitsprüfung fehlgeschlagen: {}"),
    ("Refusing to clear pacman lock because these processes are active: {}", "Pacman-Sperre wird nicht gelöst, weil diese Prozesse aktiv sind: {}"),
    ("Failed to clear pacman lock: {}", "Pacman-Sperre konnte nicht gelöst werden: {}"),
    // Settings
    ("Appearance", "Darstellung"),
    ("Theme", "Design"),
    ("Command Output", "Befehlsausgabe"),
    ("Integrated logs or external terminal window", "Integriertes Protokoll oder externes Terminalfenster"),
    ("Terminal Emulator", "Terminalprogramm"),
    ("Used when Command Output is External Terminal", "Wird verwendet, wenn die Befehlsausgabe im externen Terminal erfolgt"),
    ("General", "Allgemein"),
    ("AUR Helper", "AUR-Helfer"),
    ("Allow --noconfirm", "--noconfirm erlauben"),
    ("Applies to external terminal mode. Integrated logs are always non-interactive.", "Gilt für den externen Terminalmodus. Das integrierte Protokoll ist immer nicht interaktiv."),
    ("Cache", "Cache"),
    ("Clear screenshots cache", "Screenshot-Cache leeren"),
    ("Screenshots", "Screenshots"),
    ("About", "Über"),
    ("About WinDoesNot Store", "Über WinDoesNot Store"),
    ("System", "System"),
    ("Light", "Hell"),
    ("Dark", "Dunkel"),
    ("Ocean", "Ozean"),
    ("Emerald", "Smaragd"),
    ("Sunset", "Abendrot"),
    ("Graphite", "Graphit"),
    ("Integrated Logs", "Integriertes Protokoll"),
    ("External Terminal", "Externes Terminal"),
];

pub fn t(english: &'static str) -> &'static str {
    if language() == Language::De {
        if let Some((_, german)) = DE.iter().find(|(key, _)| *key == english) {
            return german;
        }
    }
    english
}

/// Translates a template and fills its `{}` slots in order.
pub fn tf(english: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut parts = t(english).split("{}");
    let mut out = parts.next().unwrap_or("").to_string();
    for (part, arg) in parts.zip(args.iter().map(|arg| arg.to_string()).chain(std::iter::repeat(String::new()))) {
        out.push_str(&arg);
        out.push_str(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::DE;

    #[test]
    fn translations_keep_placeholders() {
        for (english, german) in DE {
            assert_eq!(english.matches("{}").count(), german.matches("{}").count(), "{english}");
        }
        let mut keys: Vec<_> = DE.iter().map(|(english, _)| english).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), DE.len(), "duplicate English key");
    }
}
