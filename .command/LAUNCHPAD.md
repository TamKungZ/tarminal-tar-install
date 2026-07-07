kill 17118 2>/dev/null || true
sleep 2
find target/ -name '.fuse_hidden*' -delete 2>/dev/null || true
cargo clean
rm -rf target/

debuild clean

debuild -S -sa -d -kB64B156379154E5B6C176D0D6157503BFCD109A4
dput -f ppa:tamkungz/stable ../tarminal-tar-install_*.*.*~**_source.changes