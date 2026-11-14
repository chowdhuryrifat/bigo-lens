// 12_sieve_of_eratosthenes.cpp
// Pattern: Sieve of Eratosthenes with variable stride j += i
// Expected: Unknown (Non-constant step loop)

bool is_prime[1000005];

void sieve(int n) {
  for (int i = 2; i <= n; ++i) {
    is_prime[i] = true;
  }
  for (int i = 2; i * i <= n; ++i) {
    if (is_prime[i]) {
      for (int j = i * i; j <= n; j += i) {
        is_prime[j] = false;
      }
    }
  }
}
