// Expected: `while (t--)` test-case loop with two sequential inner loops
// over n and m. With t <= 10000, n, m <= 200000, sum(n) <= 200000 and sum(m) <= 300000,
// the workload must be 200000 + 300000 = 500000, Proven, method sum-constraint-packing.
#include <iostream>

int main() {
  int t = 0;
  if (!(std::cin >> t))
    return 0;
  while (t--) {
    int n = 0;
    int m = 0;
    std::cin >> n >> m;
    long long acc = 0;
    for (int i = 0; i < n; ++i) {
      acc += i;
    }
    for (int j = 0; j < m; ++j) {
      acc += j;
    }
    std::cout << acc << '\n';
  }
}
