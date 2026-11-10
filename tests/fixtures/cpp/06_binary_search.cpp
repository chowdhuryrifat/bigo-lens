// Expected: the binary-search while loop has data-dependent termination and is
// reported as Unknown rather than guessed as O(log n).
#include <iostream>
#include <vector>

int main() {
  int n = 0;
  int target = 0;
  std::cin >> n >> target;
  std::vector<int> a(n);
  for (int i = 0; i < n; ++i) {
    a[i] = 2 * i;
  }
  int lo = 0;
  int hi = n;
  while (lo < hi) {
    const int mid = lo + (hi - lo) / 2;
    if (a[mid] < target) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  std::cout << lo << '\n';
}
