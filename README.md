# WinDoesNot Store

The app store of the [WinDoesNot](https://windoesnot.me) desktop: curated
recommendations, search across the Arch/CachyOS repositories, the AUR and
Flathub, and system updates that stay safe for a Hyprland desktop.

**[Deutsch weiter unten](#deutsch)**

![Home page in dark mode](docs/screenshots/home-dark.png)

| Search | Light mode |
|---|---|
| ![Search results](docs/screenshots/search.png) | ![Home page in light mode](docs/screenshots/home-light.png) |

> Status: early preview (0.1.0). It is developed for WinDoesNot and tested
> there; on other Arch-based systems it runs, but the desktop integration
> (accent colour, light/dark, language) only applies with WinDoesNot.

## Features

- **Home page** with a banner carousel, category tiles and curated picks.
  Apps installed via pacman or as a Flatpak are marked as installed.
- **Search** in the official/CachyOS repositories, the AUR (paru or yay) and
  Flathub, ordered by relevance, with real app icons: icon theme, AppStream
  catalogues (`archlinux-appstream-data`, Flathub) or a letter tile.
- **Safe updates:** updates are only applied as a whole (`pacman -Syu`).
  Updating single packages is a partial upgrade on Arch and can break
  software built against exact library versions (e.g. Hyprland plugins).
- **Privileged operations** go through a small helper started with `pkexec`
  that only accepts an allowlist of pacman operations.
- **WinDoesNot integration:** accent colour and light/dark mode follow the
  desktop settings live; the interface is German or English depending on
  the desktop or system language.

## Build and install

Requirements: `gtk4`, `libadwaita`, `polkit`, `appstream`; for building
`cargo` and `clang`. Optional: `archlinux-appstream-data`, `flatpak`, `paru`
or `yay`.

```sh
cargo build --release --locked
cargo test --release --locked
```

A `PKGBUILD` is included (`makepkg -si`). It installs the app, the helper,
the polkit policy, the desktop entry and the AppStream metadata.

## Credits and licence

WinDoesNot Store is a fork of [Aurora](https://github.com/ahmoodio/aurora) by
ahmoodio. Thank you for the solid foundation.

MIT licence, see [LICENSE](LICENSE). App names, icons and screenshots shown
in the store belong to their respective projects.

---

## Deutsch

Der App-Store des [WinDoesNot](https://windoesnot.me)-Desktops: kuratierte
Empfehlungen, Suche in den Arch-/CachyOS-Paketquellen, im AUR und auf Flathub
sowie Systemupdates, die für einen Hyprland-Desktop sicher bleiben.

> Stand: frühe Vorschau (0.1.0). Entwickelt und getestet für WinDoesNot; auf
> anderen Arch-basierten Systemen läuft der Store ebenfalls, die
> Desktop-Anbindung (Akzentfarbe, Hell/Dunkel, Sprache) greift aber nur unter
> WinDoesNot.

### Funktionen

- **Startseite** mit Banner-Karussell, Kategorie-Kacheln und Empfehlungen.
  Bereits per pacman oder als Flatpak installierte Apps werden markiert.
- **Suche** in den offiziellen bzw. CachyOS-Paketquellen, im AUR (paru oder
  yay) und auf Flathub, nach Relevanz sortiert und mit echten App-Symbolen:
  Iconset, AppStream-Kataloge (`archlinux-appstream-data`, Flathub) oder eine
  Buchstabenkachel.
- **Sichere Updates:** Updates werden nur vollständig eingespielt
  (`pacman -Syu`). Einzelne Pakete zu aktualisieren ist unter Arch ein
  Teil-Upgrade und kann Software beschädigen, die gegen genaue
  Bibliotheksversionen gebaut ist (z. B. Hyprland-Plugins).
- **Root-Rechte** laufen über einen kleinen Helfer, der per `pkexec` startet
  und nur freigegebene pacman-Operationen annimmt.
- **WinDoesNot-Anbindung:** Akzentfarbe sowie Hell/Dunkel folgen live den
  Desktop-Einstellungen; die Oberfläche ist je nach Desktop- oder
  Systemsprache deutsch oder englisch.

### Bauen und installieren

Benötigt `gtk4`, `libadwaita`, `polkit` und `appstream`, zum Bauen `cargo` und
`clang`. Optional: `archlinux-appstream-data`, `flatpak`, `paru` oder `yay`.

```sh
cargo build --release --locked
cargo test --release --locked
```

Ein `PKGBUILD` liegt bei (`makepkg -si`). Es installiert App, Helfer,
polkit-Richtlinie, Desktop-Eintrag und AppStream-Metadaten.

### Herkunft und Lizenz

Der WinDoesNot Store ist ein Fork von [Aurora](https://github.com/ahmoodio/aurora)
von ahmoodio. Danke für die solide Grundlage.

MIT-Lizenz, siehe [LICENSE](LICENSE). Im Store gezeigte App-Namen, Symbole und
Screenshots gehören den jeweiligen Projekten.
