// 11_binary_exponentiation.cpp
// Pattern: Halving while loop
// Expected: O(log n), Proven

long long power_by_squaring(long long base, int exp) {
  long long result = 1;
  while (exp > 0) {
    if (exp % 2 == 1) {
      result *= base;
    }
    base *= base;
    exp /= 2;
  }
  return result;
}
