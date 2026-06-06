
# AURORA
<div align="center">
  <img src="https://raw.githubusercontent.com/ahmoodio/aurora/main/assets/logo.png" alt="Aurora logo" width="180"/>

  <p><em>A modern, Wayland-first GUI for Arch Linux package management</em></p>

  [![last-commit](https://img.shields.io/github/last-commit/ahmoodio/aurora?style=flat&logo=git&logoColor=white&color=7C3AED)](https://github.com/ahmoodio/aurora)
  [![repo-top-language](https://img.shields.io/github/languages/top/ahmoodio/aurora?style=flat&color=7C3AED)](https://github.com/ahmoodio/aurora)
  [![license](https://img.shields.io/github/license/ahmoodio/aurora?style=flat&color=7C3AED)](https://github.com/ahmoodio/aurora/blob/main/LICENSE)
  [![Arch Linux](https://img.shields.io/badge/Arch%20Linux-1793D1?logo=arch-linux&logoColor=white)](https://archlinux.org/)
  [![Wayland](https://img.shields.io/badge/Wayland-ready-success)](https://wayland.freedesktop.org/)

  <p><em>Built with:</em></p>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-000000.svg?style=flat&logo=rust&logoColor=white">
  <img alt="GTK4" src="https://img.shields.io/badge/GTK4-4A86CF.svg?style=flat&logo=gtk&logoColor=white">
  <img alt="libadwaita" src="https://img.shields.io/badge/libadwaita-3D3D3D.svg?style=flat">
  <img alt="Pacman" src="https://img.shields.io/badge/pacman-1793D1.svg?style=flat&logo=arch-linux&logoColor=white">

<p>
  <a href="#-overview">🌌 Overview</a> •
  <a href="#-features">✨ Features</a> •
  <a href="#-why-aurora-vs-pamac--octopi">🌟 Why Aurora</a> •
  <a href="#-installation">📥 Installation</a> •
  <a href="#-security-model">🔐 Security</a> •
  <a href="#-development">🧩 Development</a>
</p>
</div>

---

## 🌌 Overview

**Aurora** is a **modern, native GUI package manager for Arch Linux**, designed from the ground up for **Wayland**, **GTK4**, and **libadwaita**.

Unlike traditional wrappers, Aurora focuses on:
- clarity over magic  
- safety over convenience  
- transparency over hidden automation  

Aurora integrates:
- **pacman** for official repositories  
- **AUR helpers (yay / paru)** for community packages  
- **AppStream metadata** for rich visuals (icons, screenshots, descriptions)

All wrapped in a clean, store-like interface inspired by modern Linux desktops.

---

## ✨ Features

- **Unified Package Management**
  - Manage official repo packages (pacman)
  - Manage AUR packages via yay or paru
  - Clear source badges: Repo / AUR

- **Wayland-First UI**
  - Built with GTK4 + libadwaita
  - Smooth animations and adaptive layouts
  - Native feel on GNOME, COSMIC, and modern Wayland compositors

- **Rich App Details**
  - AppStream integration for icons and screenshots
  - Clean app detail pages with versions, descriptions, and metadata

- **Transaction Queue & Review**
  - Queue installs, removals, and updates
  - Review all actions before execution
  - No hidden system changes

- **Live Logs & Feedback**
  - Real-time stdout/stderr streaming
  - Copy or save logs for debugging
  - Clear error messages (no silent failures)

- **Configurable AUR Backend**
  - Switch between `yay` and `paru`
  - Interactive by default (no forced `--noconfirm`)

---
## 🌟 Why Aurora vs Pamac / Octopi?

Aurora is **not** trying to replace Pamac or Octopi — it exists to solve a *different set of problems*.

### 🆚 Aurora vs Pamac

| Aurora | Pamac |
|------|-------|
| Wayland-first (GTK4 + libadwaita) | Primarily GTK3 / mixed backends |
| Focused on transparency and safety | Convenience-first abstractions |
| Explicit transaction review | Often hides low-level pacman details |
| Strict polkit helper model | Broader privileged execution |
| Designed for Arch users | Originally designed for Manjaro |

**Aurora’s philosophy:**  
> *Show the user exactly what will happen — and let them decide.*

---

### 🆚 Aurora vs Octopi

| Aurora | Octopi |
|------|-------|
| Modern, store-like UI | Traditional utility-style UI |
| AppStream icons & screenshots | Text-focused package lists |
| Designed for Wayland + GTK4 | Qt / X11-centric |
| Guided install & review flow | Power-user workflows |
| Visual clarity for discovery | Optimized for speed & control |

**Aurora’s philosophy:**  
> *Package management can be powerful **and** approachable.*

---

### 🎯 Who Aurora Is For

Aurora is ideal if you want:
- A **modern, native Wayland experience**
- Visual app discovery with real metadata
- Clear, reviewable system changes
- A GUI that respects Arch’s philosophy without hiding it

Aurora intentionally avoids:
- Running entire sessions as root
- Silent background changes
- “Magic” automation without visibility

---

### 🧠 Design Goal

Aurora aims to sit **between**:
- **Pamac** (convenience)
- **Octopi** (raw control)

…offering a **safe, modern middle ground** for Arch users who want clarity without friction.

---

## 🔐 Security Model

Aurora is designed with **least privilege** in mind:

* The GUI runs **unprivileged**
* All system-level operations are executed via:
  * a **dedicated helper binary** (`/usr/bin/aurora-helper`)
  * invoked through **polkit (pkexec)**
* Only **whitelisted pacman actions** are allowed
* No shell execution, no arbitrary commands

This makes Aurora **safer than traditional GUI wrappers** that run entire sessions as root.

### 🛡️ What the helper whitelists

The helper (`src/bin/aurora-helper.rs`) only forwards a fixed set of
pacman operations to the real binary. By default, these are:

| pacman op | Meaning                                                |
|-----------|--------------------------------------------------------|
| `-S`      | Sync (install) a package                               |
| `-Syu`    | Full system upgrade (no extra packages allowed)       |
| `-R`      | Remove a single package (keep deps and config files)   |
| `-Rns`    | Remove a package **and** its unused deps and configs   |
| `-U`      | Install a local `.pkg.tar.*` file (path-restricted)    |
| `-D`      | Modify the install reason (`asdeps` / `asexplicit`)    |

Any other operation (e.g. `-Q`, `-Ss`, `-Si`, `-F`, `-Rdd`, …) is
**rejected by the helper with** `operation not allowed: <op>`.
This is **intentional** — `-Rdd` in particular can break your system
because it skips dependency checks, and `-Q`/`-Ss` only read state
and do not need root at all, so the helper does not need to allow them.

If you call the helper directly, e.g. from an AUR helper that
shells out through `pkexec`, and you get a rejection, that is the
helper doing its job.

### 🧩 Why isn't `-R` (or `-Rns`) allowed by default?

It **is** allowed. The whitelist above is enforced by the helper
itself, not by polkit. Polkit only governs whether `aurora-helper`
may run as root at all (see `resources/io.github.ahmoodio.aurora.policy`).
The helper is the layer that decides **which pacman arguments** are
permitted.

If you see `aurora-helper error: operation not allowed: -R`, it means
you are running an older build that predates this whitelist entry.
Rebuild and reinstall:

```bash
git pull
makepkg -si
```

### ✏️ Customising the whitelist

If you want to allow additional pacman operations (for example
`-Rs` to remove a package and its dependencies but keep config files),
edit the `allowed_ops` array in `src/bin/aurora-helper.rs`:

```rust
let allowed_ops = ["-S", "-Syu", "-R", "-Rs", "-Rns", "-U", "-D"];
```

You should also extend the "no packages supplied" guard below it
to cover the new op, and rebuild:

```bash
cargo build --release
sudo install -Dm755 target/release/aurora-helper /usr/bin/aurora-helper
```

> ⚠️ **Be careful.** The whitelist is the only thing standing
> between an unprivileged GUI session and root. Adding `-Rdd`,
> `-Syuu --overwrite '*'`, or anything that bypasses pacman's
> safety checks will let the GUI (or anything that can talk to
> the helper) damage your install. Prefer doing destructive
> operations in a real root shell instead of relaxing the helper.

### 🧪 Calling the helper manually

For debugging or scripting, you can invoke it the same way the
GUI does:

```bash
pkexec /usr/bin/aurora-helper pacman -S --noconfirm ripgrep
pkexec /usr/bin/aurora-helper pacman -Rns --noconfirm ripgrep
pkexec /usr/bin/aurora-helper pacman pacman -Rns --noconfirm ripgrep
```

> The `pacman` argument may appear once or twice; the helper
> tolerates both. Other operations will be refused with
> `operation not allowed: <op>`.

---

## 📥 Installation

### From AUR (recommended)

Using your favorite AUR helper:

```bash
yay -S aurora-gui-git
```

or

```bash
paru -S aurora-gui-git
```

Prebuilt package (no local Rust build):

```bash
yay -S aurora-gui-bin
```

---

### Manual build (for testing)

```bash
git clone https://github.com/ahmoodio/aurora.git
cd aurora
cargo build --release
./target/release/aurora
```

> ⚠️ Manual builds do **not** install polkit rules or desktop files.

### Release tags for `aurora-gui-bin`

`aurora-gui-bin` uses GitHub release assets from this repo.  
When you push a tag like `v0.1.1`, CI builds and uploads:

- `aurora-linux-x86_64.tar.gz`
- `aurora-linux-x86_64.tar.gz.sha256`

Example:

```bash
git tag v0.1.1
git push origin v0.1.1
```

---

## 🧩 Development

### Requirements

* Rust (stable)
* GTK4
* libadwaita
* pkgconf

### Run in development mode

```bash
cargo run
```

---

## 📄 License

MIT License.
See [`LICENSE`](https://github.com/ahmoodio/aurora/blob/main/LICENSE).

---

## ⭐ Support

If Aurora helps you:

* ⭐ Star the repo
* 🐞 Report issues
* 💡 Suggest features
* 🔧 Open pull requests



