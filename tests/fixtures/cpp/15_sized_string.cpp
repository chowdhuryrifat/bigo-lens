// Expected: std::string s(n, 'a') => n bytes (1 byte per char).
#include <iostream>
#include <string>

int main() {
  int n = 0;
  std::cin >> n;
  std::string s(n, 'a');
  std::cout << s.length() << '\n';
}
