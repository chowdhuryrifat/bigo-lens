// Expected: the input loop is linear (O(n)); the std::sort call is costed via the
// STL cost model as O(n log n) (stl-algorithm). A sized vector allocation of n ints
// is reported.
#include <algorithm>
#include <iostream>
#include <vector>

int main() {
  int n = 0;
  std::cin >> n;
  std::vector<int> values(n);
  for (int i = 0; i < n; ++i) {
    std::cin >> values[i];
  }
  std::sort(values.begin(), values.end());
  std::cout << values.front() << '\n';
}
