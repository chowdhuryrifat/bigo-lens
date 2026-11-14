// 16_collatz_iteration.cpp
// Pattern: Collatz conjecture iteration
// Expected: Unknown (Data-dependent termination, Prime Directive)

int collatz_steps(long long n) {
  int steps = 0;
  while (n > 1) {
    if (n % 2 == 0) {
      n /= 2;
    } else {
      n = 3 * n + 1;
    }
    steps++;
  }
  return steps;
}
