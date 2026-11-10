// Expected: two sequential inner loops inside one outer loop => n * (n + m).
#include <iostream>

int main() {
  int n = 0;
  int m = 0;
  std::cin >> n >> m;
  long long acc = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      acc += j;
    }
    for (int k = 0; k < m; ++k) {
      acc -= k;
    }
  }
  std::cout << acc << '\n';
}
