// Expected: Near miss: std::vector filled by push_back in a loop.
// Does not produce a sized dynamic allocation estimate.
#include <iostream>
#include <vector>

int main() {
  int n = 0;
  std::cin >> n;
  std::vector<int> v;
  for (int i = 0; i < n; ++i) {
    v.push_back(i);
  }
  std::cout << v.size() << '\n';
}
