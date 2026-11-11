// Expected: nested rectangular loop over a grid of size n x m. With
// n <= 10^6, m <= 10^6 and product constraint n * m <= 10^6, the workload
// must be 10^6 (not 10^12) with method product-constraint-bound, Proven.
#include <iostream>

int main() {
  int n = 0;
  int m = 0;
  std::cin >> n >> m;
  long long acc = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < m; ++j) {
      acc += i ^ j;
    }
  }
  std::cout << acc << '\n';
}
