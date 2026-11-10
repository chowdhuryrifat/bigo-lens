// Expected: std::vector<long long> of n elements => 8 * n bytes, O(n) space,
// plus a static array whose size comes from the compiler's layout.
#include <iostream>
#include <vector>

int lookup[1000];

int main() {
  int n = 0;
  std::cin >> n;
  std::vector<long long> prefix(n + 1);
  for (int i = 0; i < n; ++i) {
    prefix[i + 1] = prefix[i] + lookup[i % 1000];
  }
  std::cout << prefix[n] << '\n';
}
