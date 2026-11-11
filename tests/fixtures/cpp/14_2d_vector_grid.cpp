// Expected: 2D DP table std::vector<std::vector<int>> grid(n, std::vector<int>(m))
// => n * m * 4 + n * 24 bytes (on 64-bit systems with sizeof(int) = 4 and sizeof(vector) = 24).
#include <iostream>
#include <vector>

int main() {
  int n = 0;
  int m = 0;
  std::cin >> n >> m;
  std::vector<std::vector<int>> grid(n, std::vector<int>(m));
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < m; ++j) {
      grid[i][j] = i + j;
    }
  }
  std::cout << grid[n - 1][m - 1] << '\n';
}
