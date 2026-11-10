// Expected: rectangular nest, time expression n * m, O(n m), Proven.
#include <iostream>

int main() {
  int n = 0;
  int m = 0;
  std::cin >> n >> m;
  long long cells = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < m; ++j) {
      cells += i ^ j;
    }
  }
  std::cout << cells << '\n';
}
