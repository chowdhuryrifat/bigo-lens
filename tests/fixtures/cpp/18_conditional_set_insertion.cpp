// Expected: Conditional insertion into std::set<int> => upper bound 36 * n bytes (Estimated).
#include <iostream>
#include <set>

int main() {
  int n = 0;
  std::cin >> n;
  std::set<int> s;
  for (int i = 0; i < n; ++i) {
    if (i % 2 == 0) {
      s.insert(i);
    }
  }
  std::cout << s.size() << '\n';
}
