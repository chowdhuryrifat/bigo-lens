// Expected: Collatz iteration; termination depends on runtime values, so the
// loop is reported as Unknown.
#include <iostream>

int main() {
  long long x = 0;
  std::cin >> x;
  int steps = 0;
  while (x != 1) {
    x = (x % 2 == 0) ? x / 2 : 3 * x + 1;
    ++steps;
  }
  std::cout << steps << '\n';
}
