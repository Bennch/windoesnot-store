use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageSource {
    Repo,
    Aur,
    Flatpak,
}

impl PackageSource {
    pub fn label(self) -> &'static str {
        match self {
            PackageSource::Repo => "Pacman",
            PackageSource::Aur => "AUR",
            PackageSource::Flatpak => "Flatpak",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSummary {
    pub name: String,
    pub summary: String,
    pub version: String,
    pub source: PackageSource,
    pub installed: bool,
    pub origin: Option<String>,
    /// Where the app is installed, which may be another source than this
    /// result (e.g. a repo hit for an app installed as Flatpak).
    #[serde(default)]
    pub installed_via: Option<PackageSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDetails {
    pub name: String,
    pub summary: String,
    pub description: String,
    pub version: String,
    pub source: PackageSource,
    pub installed: bool,
    pub size: Option<String>,
    pub home: Option<String>,
    pub screenshots: Vec<String>,
    pub icon_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    Install,
    Remove,
    Upgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionAction {
    pub name: String,
    pub source: PackageSource,
    pub kind: ActionKind,
    pub origin: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransactionQueue {
    pub actions: Vec<TransactionAction>,
}

impl TransactionQueue {
    pub fn push(&mut self, action: TransactionAction) -> bool {
        let exists = self.actions.iter().any(|existing| {
            existing.name == action.name
                && existing.source == action.source
                && existing.kind == action.kind
                && existing.origin == action.origin
        });
        if exists {
            return false;
        }
        self.actions.push(action);
        true
    }

    /// Removes one queued action; true if it was queued.
    pub fn remove(&mut self, name: &str, source: PackageSource, kind: ActionKind) -> bool {
        let before = self.actions.len();
        self.actions.retain(|action| !(action.name == name && action.source == source && action.kind == kind));
        self.actions.len() != before
    }

    pub fn clear(&mut self) {
        self.actions.clear();
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AurHelperKind {
    Yay,
    Paru,
}

impl AurHelperKind {
    /// Prefer the saved choice, then fall back to the helper actually installed.
    /// No global environment mutation: availability can be injected in tests.
    pub fn resolve_with(self, available: impl Fn(&str) -> bool) -> Option<Self> {
        let other = match self {
            Self::Yay => Self::Paru,
            Self::Paru => Self::Yay,
        };
        [self, other].into_iter().find(|helper| available(helper.as_str()))
    }

    pub fn installed(self) -> Option<Self> {
        self.resolve_with(crate::core::cache::command_exists)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AurHelperKind::Yay => "yay",
            AurHelperKind::Paru => "paru",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
    Ocean,
    Emerald,
    Sunset,
    Graphite,
}

impl ThemeMode {
    pub fn all() -> &'static [ThemeMode] {
        static THEMES: [ThemeMode; 7] = [
            ThemeMode::System,
            ThemeMode::Light,
            ThemeMode::Dark,
            ThemeMode::Ocean,
            ThemeMode::Emerald,
            ThemeMode::Sunset,
            ThemeMode::Graphite,
        ];
        &THEMES
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::Ocean => "Ocean",
            ThemeMode::Emerald => "Emerald",
            ThemeMode::Sunset => "Sunset",
            ThemeMode::Graphite => "Graphite",
        }
    }

    pub fn to_index(self) -> u32 {
        Self::all()
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> ThemeMode {
        Self::all()
            .get(index as usize)
            .copied()
            .unwrap_or(ThemeMode::System)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalMode {
    Integrated,
    External,
}

impl TerminalMode {
    pub fn all() -> &'static [TerminalMode] {
        static MODES: [TerminalMode; 2] = [TerminalMode::Integrated, TerminalMode::External];
        &MODES
    }

    pub fn label(self) -> &'static str {
        match self {
            TerminalMode::Integrated => "Integrated Logs",
            TerminalMode::External => "External Terminal",
        }
    }

    pub fn to_index(self) -> u32 {
        Self::all()
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> TerminalMode {
        Self::all()
            .get(index as usize)
            .copied()
            .unwrap_or(TerminalMode::Integrated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalEmulator {
    Auto,
    Kitty,
    Konsole,
    Alacritty,
}

impl TerminalEmulator {
    pub fn all() -> &'static [TerminalEmulator] {
        static EMULATORS: [TerminalEmulator; 4] = [
            TerminalEmulator::Auto,
            TerminalEmulator::Kitty,
            TerminalEmulator::Konsole,
            TerminalEmulator::Alacritty,
        ];
        &EMULATORS
    }

    pub fn label(self) -> &'static str {
        match self {
            TerminalEmulator::Auto => "Auto",
            TerminalEmulator::Kitty => "Kitty",
            TerminalEmulator::Konsole => "Konsole",
            TerminalEmulator::Alacritty => "Alacritty",
        }
    }

    pub fn to_index(self) -> u32 {
        Self::all()
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> TerminalEmulator {
        Self::all()
            .get(index as usize)
            .copied()
            .unwrap_or(TerminalEmulator::Auto)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub aur_helper: AurHelperKind,
    pub allow_noconfirm: bool,
    pub theme: ThemeMode,
    pub terminal_mode: TerminalMode,
    pub terminal_emulator: TerminalEmulator,
    pub aur_enabled: bool,
}

impl Settings {
    pub fn aur_available(&self) -> bool {
        self.aur_enabled && self.aur_helper.installed().is_some()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aur_helper: AurHelperKind::Yay,
            allow_noconfirm: false,
            theme: ThemeMode::System,
            terminal_mode: TerminalMode::Integrated,
            terminal_emulator: TerminalEmulator::Auto,
            aur_enabled: false,
        }
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    fn action(name: &str, source: PackageSource, kind: ActionKind) -> TransactionAction {
        TransactionAction { name: name.into(), source, kind, origin: None }
    }

    #[test]
    fn remove_takes_out_only_the_matching_action() {
        let mut queue = TransactionQueue::default();
        queue.push(action("gimp", PackageSource::Repo, ActionKind::Install));
        queue.push(action("org.gimp.GIMP", PackageSource::Flatpak, ActionKind::Remove));
        assert!(!queue.remove("gimp", PackageSource::Flatpak, ActionKind::Install));
        assert!(queue.remove("gimp", PackageSource::Repo, ActionKind::Install));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.actions[0].name, "org.gimp.GIMP");
    }
}
