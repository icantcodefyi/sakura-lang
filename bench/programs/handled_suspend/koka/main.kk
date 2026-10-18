import kernels
import runner

fun main() { bench-main(kernels/handled-suspend, 1000000) }
