kill 17118 2>/dev/null || true
sleep 2
find target/ -name '.fuse_hidden*' -delete 2>/dev/null || true
cargo clean
rm -rf target/

debuild clean

debuild -S -sa -d -kB64B156379154E5B6C176D0D6157503BFCD109A4
ls -lh ../tarminal-tar-install_0.1.4~noble1.tar.xz
ls -lh ../tarminal-tar-install_0.1.4~jammy1.tar.xz