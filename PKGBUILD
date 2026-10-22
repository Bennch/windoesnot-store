# Maintainer: WinDoesNot contributors <https://github.com/Bennch/windoesnot-store>
# Based on the Aurora PKGBUILD by ahmoodio.

pkgname=windoesnot-store
pkgver=0.2.0
pkgrel=1
pkgdesc="App store of the WinDoesNot desktop (GTK4/libadwaita, pacman, AUR, Flatpak)"
arch=('x86_64')
url="https://github.com/Bennch/windoesnot-store"
license=('MIT')
depends=('gtk4' 'libadwaita' 'polkit' 'pacman' 'appstream')
optdepends=('archlinux-appstream-data: icons for repository apps that are not installed'
            'flatpak: Flathub apps'
            'paru: AUR packages'
            'yay: AUR packages')
makedepends=('cargo' 'clang' 'perl')
# ring's C objects would be GCC-LTO bitcode, which the Rust linker cannot read.
options=('!lto')
install=windoesnot-store.install
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname-$pkgver"
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked --bins
}

check() {
  cd "$pkgname-$pkgver"
  cargo test --release --locked
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 target/release/windoesnot-store "$pkgdir/usr/bin/windoesnot-store"
  install -Dm755 target/release/windoesnot-store-helper "$pkgdir/usr/bin/windoesnot-store-helper"
  install -Dm644 resources/me.windoesnot.Store.desktop \
    "$pkgdir/usr/share/applications/me.windoesnot.Store.desktop"
  install -Dm644 resources/me.windoesnot.Store.metainfo.xml \
    "$pkgdir/usr/share/metainfo/me.windoesnot.Store.metainfo.xml"
  install -Dm644 resources/me.windoesnot.Store.policy \
    "$pkgdir/usr/share/polkit-1/actions/me.windoesnot.Store.policy"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
