// Expected: `while (t--)` test-case loop around an O(n^2) body. With
// t <= 10, n <= 200000 and sum(n) <= 200000 the workload must be
// 200000^2 = 4e10, not t * n_max^2 = 4e11.
#include <iostream>

int main() {
  int t = 0;
  std::cin >> t;
  while (t--) {
    int n = 0;
    std::cin >> n;
    long long acc = 0;
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        acc += i * j;
      }
    }
    std::cout << acc << '\n';
  }
}
