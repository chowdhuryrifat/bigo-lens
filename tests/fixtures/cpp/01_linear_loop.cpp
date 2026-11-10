// Expected: single linear loop, time expression n, O(n), Proven.
#include <iostream>

int main() {
  int n = 0;
  std::cin >> n;
  long long sum = 0;
  for (int i = 0; i < n; ++i) {
    sum += i;
  }
  std::cout << sum << '\n';
}
