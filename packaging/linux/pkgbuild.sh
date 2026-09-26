#!/bin/sh
# Print the Arch Linux PKGBUILD for a release, from PKGBUILD.in.
#
#   pkgbuild.sh TAG AMD64_DEB ARM64_DEB > PKGBUILD
#
# TAG is the release's tag (the .debs are downloaded from it); the .debs are
# the release's packages, named myconnect_<version>_<arch>.deb.
set -eu

if [ $# -ne 3 ]; then
  echo "usage: $0 TAG AMD64_DEB ARM64_DEB" >&2
  exit 2
fi
tag=$1
amd64=$2
arm64=$3

debver=$(basename "$amd64" | sed -n 's/^myconnect_\(.*\)_amd64\.deb$/\1/p')
if [ -z "$debver" ] || [ "$(basename "$arm64")" != "myconnect_${debver}_arm64.deb" ]; then
  echo "pkgbuild.sh: expected myconnect_<version>_amd64.deb and _arm64.deb of the same version" >&2
  exit 1
fi
# pkgver can't contain "-", ":" or "/"; release versions are MAJOR.MINOR.PATCH.
pkgver=$debver
case $pkgver in
  *[-:/]*)
    echo "pkgbuild.sh: $pkgver isn't a valid pkgver" >&2
    exit 1
    ;;
esac

sed -e "s|@PKGVER@|$pkgver|" \
  -e "s|@DEBVER@|$debver|g" \
  -e "s|@TAG@|$tag|" \
  -e "s|@SHA256_AMD64@|$(sha256sum "$amd64" | cut -d' ' -f1)|" \
  -e "s|@SHA256_ARM64@|$(sha256sum "$arm64" | cut -d' ' -f1)|" \
  "$(dirname "$0")/PKGBUILD.in"
