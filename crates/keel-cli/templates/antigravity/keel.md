# keel Code Graph Enforcement

This project uses keel. After EVERY file edit, run `keel compile <file> --json`.
Fix all errors before proceeding. Type hints and public docstrings are mandatory.
Before editing functions with upstream callers, run `keel discover <hash>`.
