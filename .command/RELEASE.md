cargo build --release -p tar-install
cargo build --release -p tarminal

cargo deb -p tar-install
cargo deb -p tarminal

strip -s target/release/tar-install
strip -s target/release/tarminal

cargo generate-rpm -p crates/tar-install
cargo generate-rpm -p crates/tarminal

git tag v*
git push
git push origin v*