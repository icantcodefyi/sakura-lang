import kernels
import runner

fun main() { bench-main(kernels/handled-reentry, 1000000) }
