git status
git log --oneline -5
git tag -d v0.1.5 2>/dev/null || true
git push origin :refs/tags/v0.1.5
git fetch --prune --tags
git tag -a v0.1.5 -m "Release v0.1.5"
git push origin v0.1.5