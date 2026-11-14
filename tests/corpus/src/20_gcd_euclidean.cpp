// 20_gcd_euclidean.cpp
// Pattern: Euclidean GCD algorithm (while loop with remainder update)
// Expected: Unknown (Data-dependent termination, Prime Directive)

long long compute_gcd(long long a, long long b) {
  while (b != 0) {
    long long rem = a % b;
    a = b;
    b = rem;
  }
  return a;
}
