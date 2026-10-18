function fib(n) {
  return n < 2 ? n : fib(n - 1) + fib(n - 2);
}
export function run(n) {
  return fib(n);
}
