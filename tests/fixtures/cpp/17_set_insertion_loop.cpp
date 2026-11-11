// Expected: std::set<int> with n insertions => n * (32 + 4) = 36 * n bytes (Estimated).
#include <iostream>
#include <set>

int main() {
  int n = 0;
  std::cin >> n;
  std::set<int> s;
  for (int i = 0; i < n; ++i) {
    s.insert(i);
  }
  std::cout << s.size() << '\n';
}
