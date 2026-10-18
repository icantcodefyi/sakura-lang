# Update list indices with List.set

Reassign the new list returned by `List.set` while preserving earlier versions.

Collections have value semantics. `List.set(values, index, value)` returns a new
list; ignoring the result leaves `values` unchanged. The loop computes the length
once, handles empty input, then uses the half-open range `0..n`. Every read and
write is in bounds because replacement preserves length. An external index needs
both the nonnegative and upper-bound checks, as `replace_if_present` demonstrates.

The tests check changed values, the unchanged original list, empty and singleton
inputs, and external indices below zero, at the length and inside the list.
The harness also runs `main` and checks `[3, 0, 7, 0]`. No effects other than its
console output, permissions or slice/concat reconstruction are needed.
