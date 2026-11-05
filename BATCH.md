# Batch 18b

Test data for the Linux gate runner. Not for landing: the batch lands as its individual PRs on develop.

Batch 18 (develop + #893 + #894 + #896, branch `batch/18`) does not compile: #896 moved mouse dispatch into a generic
`run_listeners<T>` and develop's #892 added a `debug!(..., target, ...)` line inside that block (E0425 at
crates/rustkit-engine/src/lib.rs:4333). #896 goes back to its author for a rebase. This is the same batch without #896.

- batch: 18b
- develop: 82893eac47c99a39ebe232318000f6cccfa66a9a
- PR #893: 2f694551308c67a7fdf0bfaf86206450f7c871d7
- PR #894: 0498dc847540a3aa64f070fc64b1bd883d4eb9a3
- merged tree (develop + #893 + #894, `--no-ff` in that order, before this file): 5c41a07d34337bd69a7f35f90913e9a1d4935ace
- merged commit (before this file): a71904a536b4dc3b4a82750aa735e19d544a6957
