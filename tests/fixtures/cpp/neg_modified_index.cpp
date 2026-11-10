// Negative fixture: the loop index is written inside the body, so the trip
// count cannot be derived from the loop header. Expected: Unsupported.
#include <iostream>

int main() {
  int n = 0;
  std::cin >> n;
  for (int i = 0; i < n; ++i) {
    if (i % 3 == 0) {
      i += 2;
    }
  }
}
