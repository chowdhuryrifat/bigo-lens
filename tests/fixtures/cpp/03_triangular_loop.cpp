// Expected: triangular nest, structural count n(n-1)/2, O(n^2), Proven.
#include <iostream>

int main() {
  int n = 0;
  std::cin >> n;
  long long pairs = 0;
  for (int i = 0; i < n; ++i)
    for (int j = 0; j < i; ++j)
      ++pairs;
  std::cout << pairs << '\n';
}
