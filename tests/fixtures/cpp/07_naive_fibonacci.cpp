// Expected: direct recursion with two self-calls, reported as Unsupported
// (recurrence analysis is not implemented); no complexity is claimed.
#include <iostream>

long long fib(int n) {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}

int main() {
  int n = 0;
  std::cin >> n;
  std::cout << fib(n) << '\n';
}
