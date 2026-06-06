# Maintainer: ahmoodio <ahmoodio at github dot com>

pkgname=aurora
pkgver=0.1.0
pkgrel=1
pkgdesc="A modern, Wayland-first GUI for Arch Linux package management"
arch=('x86_64')
url="https://github.com/ahmoodio/aurora"
license=('MIT')
depends=('gtk4' 'libadwaita' 'polkit')
makedepends=('cargo' 'clang' 'perl')
install=aurora.install
source=("$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname-$pkgver"
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked --bins
}

package() {
  cd "$pkgname-$pkgver"

  install -Dm755 target/release/aurora "$pkgdir/usr/bin/aurora"
  install -Dm755 target/release/aurora-helper "$pkgdir/usr/bin/aurora-helper"

  install -Dm644 resources/io.github.ahmoodio.aurora.desktop \
    "$pkgdir/usr/share/applications/io.github.ahmoodio.aurora.desktop"
  install -Dm644 resources/io.github.ahmoodio.aurora.metainfo.xml \
    "$pkgdir/usr/share/metainfo/io.github.ahmoodio.aurora.metainfo.xml"
  install -Dm644 resources/io.github.ahmoodio.aurora.policy \
    "$pkgdir/usr/share/polkit-1/actions/io.github.ahmoodio.aurora.policy"

  install -Dm644 assets/icons/hicolor/256x256/apps/io.github.ahmoodio.aurora.png \
    "$pkgdir/usr/share/icons/hicolor/256x256/apps/io.github.ahmoodio.aurora.png"
  install -Dm644 assets/icons/hicolor/scalable/apps/io.github.ahmoodio.aurora.png \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.github.ahmoodio.aurora.png"
}
