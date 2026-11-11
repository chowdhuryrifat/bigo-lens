// Expected: nested rectangular loop over n and m.
// With n, m <= 10^6 and linear constraint n + m <= 200000,
// the workload must be 10^10 (not 10^12) with method linear-constraint-bound, Proven.
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
