//! Curated content of the home page. Packages come from the official/CachyOS
//! repositories; the Flathub id is only used to find an icon.
use crate::desktop::{language, Language};

pub struct HomeApp {
    pub package: &'static str,
    pub flathub: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub summary_de: &'static str,
    pub summary_en: &'static str,
}

impl HomeApp {
    pub fn summary(&self) -> &'static str {
        if language() == Language::De { self.summary_de } else { self.summary_en }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Create,
    Work,
    Games,
    Social,
    Media,
    Develop,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Create,
        Category::Work,
        Category::Games,
        Category::Social,
        Category::Media,
        Category::Develop,
    ];

    pub fn label(self) -> &'static str {
        let (de, en) = match self {
            Category::Create => ("Erstellen", "Create"),
            Category::Work => ("Arbeit", "Work"),
            Category::Games => ("Spiele", "Games"),
            Category::Social => ("Sozial", "Social"),
            Category::Media => ("Medien", "Media"),
            Category::Develop => ("Programmieren", "Develop"),
        };
        if language() == Language::De { de } else { en }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Category::Create => "applications-graphics",
            Category::Work => "applications-office",
            Category::Games => "applications-games",
            Category::Social => "applications-internet",
            Category::Media => "applications-multimedia",
            Category::Develop => "applications-development",
        }
    }

    /// CSS class with the tile gradient.
    pub fn css_class(self) -> &'static str {
        match self {
            Category::Create => "cat-create",
            Category::Work => "cat-work",
            Category::Games => "cat-games",
            Category::Social => "cat-social",
            Category::Media => "cat-media",
            Category::Develop => "cat-develop",
        }
    }
}

/// Large carousel banners: (package, CSS class with the banner colours).
pub const BANNERS: &[(&str, &str)] = &[
    ("steam", "banner-steam"),
    ("gimp", "banner-gimp"),
    ("obs-studio", "banner-obs"),
    ("libreoffice-fresh", "banner-office"),
    ("localsend", "banner-localsend"),
];

/// Shown under "Our picks", in this order.
pub const PICKS: &[&str] = &[
    "localsend", "gimp", "obs-studio", "keepassxc", "kdenlive", "thunderbird",
    "inkscape", "vlc", "signal-desktop", "prismlauncher", "handbrake", "xournalpp",
];

pub const APPS: &[HomeApp] = &[
    HomeApp { package: "gimp", flathub: "org.gimp.GIMP", name: "GIMP", category: Category::Create, summary_de: "Fotos bearbeiten und Grafiken gestalten", summary_en: "Edit photos and create graphics" },
    HomeApp { package: "inkscape", flathub: "org.inkscape.Inkscape", name: "Inkscape", category: Category::Create, summary_de: "Vektorgrafiken, Logos und Illustrationen", summary_en: "Vector graphics, logos and illustrations" },
    HomeApp { package: "krita", flathub: "org.kde.krita", name: "Krita", category: Category::Create, summary_de: "Digital malen und zeichnen", summary_en: "Digital painting and drawing" },
    HomeApp { package: "kdenlive", flathub: "org.kde.kdenlive", name: "Kdenlive", category: Category::Create, summary_de: "Videos schneiden wie ein Profi", summary_en: "Edit videos like a pro" },
    HomeApp { package: "blender", flathub: "org.blender.Blender", name: "Blender", category: Category::Create, summary_de: "3D-Modelle, Animation und Rendering", summary_en: "3D modelling, animation and rendering" },
    HomeApp { package: "audacity", flathub: "org.audacityteam.Audacity", name: "Audacity", category: Category::Create, summary_de: "Audio aufnehmen und bearbeiten", summary_en: "Record and edit audio" },
    HomeApp { package: "obs-studio", flathub: "com.obsproject.Studio", name: "OBS Studio", category: Category::Create, summary_de: "Bildschirm aufnehmen und streamen", summary_en: "Record your screen and stream" },
    HomeApp { package: "darktable", flathub: "org.darktable.Darktable", name: "darktable", category: Category::Create, summary_de: "RAW-Fotos entwickeln und verwalten", summary_en: "Develop and manage RAW photos" },
    HomeApp { package: "libreoffice-fresh", flathub: "org.libreoffice.LibreOffice", name: "LibreOffice", category: Category::Work, summary_de: "Texte, Tabellen und Präsentationen", summary_en: "Documents, spreadsheets and slides" },
    HomeApp { package: "thunderbird", flathub: "org.mozilla.Thunderbird", name: "Thunderbird", category: Category::Work, summary_de: "E-Mails, Kontakte und Kalender", summary_en: "Email, contacts and calendars" },
    HomeApp { package: "keepassxc", flathub: "org.keepassxc.KeePassXC", name: "KeePassXC", category: Category::Work, summary_de: "Passwörter sicher verwalten", summary_en: "Manage passwords securely" },
    HomeApp { package: "nextcloud-client", flathub: "com.nextcloud.desktopclient.nextcloud", name: "Nextcloud", category: Category::Work, summary_de: "Dateien mit deiner Cloud abgleichen", summary_en: "Sync files with your cloud" },
    HomeApp { package: "xournalpp", flathub: "com.github.xournalpp.xournalpp", name: "Xournal++", category: Category::Work, summary_de: "Handschriftliche Notizen und PDFs", summary_en: "Handwritten notes and PDFs" },
    HomeApp { package: "okular", flathub: "org.kde.okular", name: "Okular", category: Category::Work, summary_de: "PDFs lesen und kommentieren", summary_en: "Read and annotate PDFs" },
    HomeApp { package: "steam", flathub: "com.valvesoftware.Steam", name: "Steam", category: Category::Games, summary_de: "Tausende Spiele, auch für Windows", summary_en: "Thousands of games, Windows ones too" },
    HomeApp { package: "lutris", flathub: "net.lutris.Lutris", name: "Lutris", category: Category::Games, summary_de: "Alle Spiele an einem Ort", summary_en: "All your games in one place" },
    HomeApp { package: "heroic-games-launcher", flathub: "com.heroicgameslauncher.hgl", name: "Heroic", category: Category::Games, summary_de: "Epic-, GOG- und Amazon-Spiele", summary_en: "Epic, GOG and Amazon games" },
    HomeApp { package: "prismlauncher", flathub: "org.prismlauncher.PrismLauncher", name: "Prism Launcher", category: Category::Games, summary_de: "Minecraft mit Mods und Profilen", summary_en: "Minecraft with mods and profiles" },
    HomeApp { package: "retroarch", flathub: "org.libretro.RetroArch", name: "RetroArch", category: Category::Games, summary_de: "Klassische Konsolen emulieren", summary_en: "Emulate classic consoles" },
    HomeApp { package: "discord", flathub: "com.discordapp.Discord", name: "Discord", category: Category::Social, summary_de: "Chat, Sprache und Video für Gruppen", summary_en: "Chat, voice and video for groups" },
    HomeApp { package: "telegram-desktop", flathub: "org.telegram.desktop", name: "Telegram", category: Category::Social, summary_de: "Schneller Messenger", summary_en: "Fast messenger" },
    HomeApp { package: "signal-desktop", flathub: "org.signal.Signal", name: "Signal", category: Category::Social, summary_de: "Privat und verschlüsselt schreiben", summary_en: "Private, encrypted messaging" },
    HomeApp { package: "element-desktop", flathub: "im.riot.Riot", name: "Element", category: Category::Social, summary_de: "Offener Chat mit Matrix", summary_en: "Open chat with Matrix" },
    HomeApp { package: "vlc", flathub: "org.videolan.VLC", name: "VLC", category: Category::Media, summary_de: "Spielt praktisch jedes Video ab", summary_en: "Plays almost every video" },
    HomeApp { package: "mpv", flathub: "io.mpv.Mpv", name: "mpv", category: Category::Media, summary_de: "Schlanker Videoplayer", summary_en: "Lean video player" },
    HomeApp { package: "spotify-launcher", flathub: "com.spotify.Client", name: "Spotify", category: Category::Media, summary_de: "Musik und Podcasts streamen", summary_en: "Stream music and podcasts" },
    HomeApp { package: "strawberry", flathub: "org.strawberrymusicplayer.strawberry", name: "Strawberry", category: Category::Media, summary_de: "Deine Musiksammlung abspielen", summary_en: "Play your music collection" },
    HomeApp { package: "handbrake", flathub: "fr.handbrake.ghb", name: "HandBrake", category: Category::Media, summary_de: "Videos umwandeln und verkleinern", summary_en: "Convert and shrink videos" },
    HomeApp { package: "localsend", flathub: "org.localsend.localsend_app", name: "LocalSend", category: Category::Media, summary_de: "Dateien an Geräte in der Nähe senden", summary_en: "Send files to nearby devices" },
    HomeApp { package: "code", flathub: "com.visualstudio.code", name: "Code - OSS", category: Category::Develop, summary_de: "Code-Editor mit Erweiterungen", summary_en: "Code editor with extensions" },
    HomeApp { package: "geany", flathub: "org.geany.Geany", name: "Geany", category: Category::Develop, summary_de: "Kleiner, schneller Editor", summary_en: "Small, fast editor" },
    HomeApp { package: "kate", flathub: "org.kde.kate", name: "Kate", category: Category::Develop, summary_de: "Vielseitiger Texteditor", summary_en: "Versatile text editor" },
    HomeApp { package: "meld", flathub: "org.gnome.meld", name: "Meld", category: Category::Develop, summary_de: "Dateien und Ordner vergleichen", summary_en: "Compare files and folders" },
    HomeApp { package: "qbittorrent", flathub: "org.qbittorrent.qBittorrent", name: "qBittorrent", category: Category::Media, summary_de: "Torrents herunterladen", summary_en: "Download torrents" },
];

pub fn app(package: &str) -> Option<&'static HomeApp> {
    APPS.iter().find(|app| app.package == package)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_references_exist() {
        for (package, _) in BANNERS {
            assert!(app(package).is_some(), "{package}");
        }
        for package in PICKS {
            assert!(app(package).is_some(), "{package}");
        }
        for category in Category::ALL {
            assert!(APPS.iter().any(|app| app.category == category));
        }
    }
}
