// Expected: doubling loop, at most log2(n) + 1 iterations, O(log n), Proven.
#include <iostream>

int main() {
  int n = 0;
  std::cin >> n;
  int steps = 0;
  for (int i = 1; i < n; i *= 2) {
    ++steps;
  }
  std::cout << steps << '\n';
}
