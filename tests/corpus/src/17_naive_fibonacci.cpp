// 17_naive_fibonacci.cpp
// Pattern: Naive direct multiple recursion
// Expected: Unsupported (Multiple recursion with subtractive arguments)

long long naive_fib(int n) {
  if (n <= 1) {
    return n;
  }
  return naive_fib(n - 1) + naive_fib(n - 2);
}
