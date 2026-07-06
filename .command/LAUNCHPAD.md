cargo clean
rm -rf target/

kill 17118
sleep 2
rm -rf target/
find target/ -name '.fuse_hidden*' -delete
rm -rf target/

debuild -S -sa -d
ls -lh ../tarminal-tar-install_0.1.4~noble1.tar.xz
ls -lh ../tarminal-tar-install_0.1.4~jammy1.tar.xz


debsign -k 6157503BFCD109A4 ../tarminal-tar-install_0.1.4~noble1_source.changes
dput ppa:tamkungz/stable ../tarminal-tar-install_0.1.4~noble1_source.changes